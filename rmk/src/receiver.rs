//! Ordered full-state snapshots for Gazell and similar receiver applications.
//!
//! The application validates and decodes RF frames into fixed-size state bytes;
//! all-zero bytes must mean all keys released. Delta events and relative pointer
//! movement must not use this queue. Use monotonic millisecond receive times and
//! call `next_report` regularly even when no packet arrives. Drain each snapshot's
//! matrix changes before loading the next. The application owns RF FIFO draining,
//! synchronization, debounce, matrix mapping and delivery to RMK events.
//!
//! Overflow preserves current state, not lost taps. No heap or radio dependency
//! is required. Nothing uses this queue until the application instantiates it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot<const BYTES: usize> {
    pub source: usize,
    pub state: [u8; BYTES],
}

#[derive(Clone, Copy, Debug)]
pub struct Stats<const SOURCES: usize> {
    pub valid: [u32; SOURCES],
    pub invalid: [u32; SOURCES],
    pub changes: u32,
    pub overflows: u32,
    pub expired: [u32; SOURCES],
}

/// Bounded full-state queue. `BYTES` is the decoded snapshot size, `SOURCES`
/// is the independent transmitter count, and `CAP` must be at least
/// `max(SOURCES, 2)`. No RF frame headers belong in a snapshot.
pub struct SnapshotQueue<const BYTES: usize, const SOURCES: usize, const CAP: usize> {
    queue: [Snapshot<BYTES>; CAP],
    head: usize,
    len: usize,
    latest: [[u8; BYTES]; SOURCES],
    last_seen: [Option<u64>; SOURCES],
    // Screen reception history survives the shorter key-release timeout.
    last_received: [Option<u64>; SOURCES],
    pub stats: Stats<SOURCES>,
}

impl<const BYTES: usize, const SOURCES: usize, const CAP: usize> SnapshotQueue<BYTES, SOURCES, CAP> {
    /// Read-only recent reception indicator, independent of key-release timeout.
    pub fn seen_within(&self, source: usize, now: u64, within_ms: u64) -> bool {
        self.last_received
            .get(source)
            .and_then(|seen| *seen)
            .is_some_and(|seen| now.saturating_sub(seen) <= within_ms)
    }
    pub const fn new() -> Self {
        assert!(BYTES > 0 && SOURCES > 0 && CAP >= SOURCES && CAP >= 2);
        Self {
            queue: [Snapshot {
                source: 0,
                state: [0; BYTES],
            }; CAP],
            head: 0,
            len: 0,
            latest: [[0; BYTES]; SOURCES],
            last_seen: [None; SOURCES],
            last_received: [None; SOURCES],
            stats: Stats {
                valid: [0; SOURCES],
                invalid: [0; SOURCES],
                changes: 0,
                overflows: 0,
                expired: [0; SOURCES],
            },
        }
    }

    fn push(&mut self, report: Snapshot<BYTES>) {
        self.queue[(self.head + self.len) % CAP] = report;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Snapshot<BYTES>> {
        if self.len == 0 {
            return None;
        }
        let report = self.queue[self.head];
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        Some(report)
    }

    fn discard_source(&mut self, source: usize) {
        let count = self.len;
        for _ in 0..count {
            let report = self.pop().unwrap();
            if report.source != source {
                self.push(report);
            }
        }
    }

    /// Called for every fetched payload. Only valid reports keep a source alive.
    /// Returns true if the consumer should wake (a change or reconnection).
    pub fn receive(&mut self, source: usize, payload: &[u8], now: u64, timeout: u64) -> bool {
        if source >= SOURCES {
            return false;
        }
        if payload.len() != BYTES {
            self.stats.invalid[source] = self.stats.invalid[source].saturating_add(1);
            return false;
        }
        self.stats.valid[source] = self.stats.valid[source].saturating_add(1);
        self.last_received[source] = Some(now);
        let expired = self.last_seen[source].is_some_and(|t| now.saturating_sub(t) > timeout);
        let reconnect = self.last_seen[source].is_none() || expired;
        let changed = reconnect || payload != self.latest[source];
        self.latest[source].copy_from_slice(payload);
        self.last_seen[source] = Some(now);
        if !changed {
            return false;
        }
        self.stats.changes = self.stats.changes.saturating_add(1);
        if expired {
            self.discard_source(source);
            self.stats.expired[source] = self.stats.expired[source].saturating_add(1);
        }
        let needed = if expired { 2 } else { 1 };
        if self.len + needed > CAP {
            // Lost history cannot be reconstructed. Reconcile all sources to
            // their latest authoritative state, rather than leave a key stuck.
            self.stats.overflows = self.stats.overflows.saturating_add(1);
            self.head = 0;
            self.len = 0;
            for source in 0..SOURCES {
                if self.last_seen[source].is_some() {
                    self.push(Snapshot {
                        source,
                        state: self.latest[source],
                    });
                }
            }
        } else {
            if expired {
                self.push(Snapshot {
                    source,
                    state: [0; BYTES],
                });
            }
            self.push(Snapshot {
                source,
                state: self.latest[source],
            });
        }
        true
    }

    /// Expiry is based on receive time, not on delayed task processing time.
    /// Remove that source's stale queued history before issuing its release.
    pub fn next_report(&mut self, now: u64, timeout: u64) -> Option<Snapshot<BYTES>> {
        for source in 0..SOURCES {
            if self.last_seen[source].is_some_and(|t| now.saturating_sub(t) > timeout) {
                self.last_seen[source] = None;
                self.latest[source] = [0; BYTES];
                self.discard_source(source);
                self.stats.expired[source] = self.stats.expired[source].saturating_add(1);
                return Some(Snapshot {
                    source,
                    state: [0; BYTES],
                });
            }
        }
        self.pop()
    }
}

impl<const BYTES: usize, const SOURCES: usize, const CAP: usize> Default for SnapshotQueue<BYTES, SOURCES, CAP> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TIMEOUT: u64 = 500;

    #[test]
    fn press_release_order_and_duplicate_keepalive() {
        let mut q = SnapshotQueue::<1, 2, 8>::new();
        assert!(q.receive(0, &[1], 0, TIMEOUT));
        assert!(!q.receive(0, &[1], 400, TIMEOUT));
        assert_eq!(q.next_report(501, TIMEOUT).unwrap().state, [1]);
        assert!(q.receive(0, &[0], 502, TIMEOUT));
        assert_eq!(q.next_report(502, TIMEOUT).unwrap().state, [0]);
        assert!(q.next_report(502, TIMEOUT).is_none());
    }

    #[test]
    fn expiry_discards_stale_history_and_releases_once() {
        let mut q = SnapshotQueue::<2, 2, 8>::new();
        q.receive(0, &[1, 2], 0, TIMEOUT);
        q.receive(0, &[3, 4], 1, TIMEOUT);
        assert_eq!(
            q.next_report(502, TIMEOUT),
            Some(Snapshot {
                source: 0,
                state: [0, 0]
            })
        );
        assert!(q.next_report(503, TIMEOUT).is_none());
        assert!(q.receive(0, &[3, 4], 504, TIMEOUT));
        assert_eq!(q.next_report(504, TIMEOUT).unwrap().state, [3, 4]);
    }

    #[test]
    fn invalid_payload_and_source_do_not_refresh_liveness() {
        let mut q = SnapshotQueue::<1, 2, 8>::new();
        q.receive(0, &[1], 0, TIMEOUT);
        assert!(!q.receive(0, &[], 500, TIMEOUT));
        assert!(!q.receive(99, &[1], 500, TIMEOUT));
        assert_eq!(q.stats.invalid, [1, 0]);
        assert_eq!(q.next_report(501, TIMEOUT).unwrap().state, [0]);
    }

    #[test]
    fn timeout_boundary_and_activity_history_are_independent() {
        let mut q = SnapshotQueue::<1, 1, 2>::new();
        q.receive(0, &[1], 0, TIMEOUT);
        assert_eq!(q.next_report(500, TIMEOUT).unwrap().state, [1]);
        assert_eq!(q.next_report(501, TIMEOUT).unwrap().state, [0]);
        assert!(q.seen_within(0, 501, 1000));
        assert!(!q.seen_within(0, 1001, 1000));
        assert!(!q.seen_within(9, 0, 1000));
    }

    #[test]
    fn overflow_resynchronizes_three_sources_and_accepts_later_changes() {
        let mut q = SnapshotQueue::<1, 3, 3>::new();
        for source in 0..3 {
            q.receive(source, &[1 << source], 0, TIMEOUT);
        }
        q.receive(0, &[0], 1, TIMEOUT);
        assert_eq!(q.stats.overflows, 1);
        for source in 0..3 {
            assert_eq!(
                q.next_report(1, TIMEOUT),
                Some(Snapshot {
                    source,
                    state: [if source == 0 { 0 } else { 1 << source }]
                })
            );
        }
        q.receive(2, &[0], 2, TIMEOUT);
        assert_eq!(q.next_report(2, TIMEOUT).unwrap().state, [0]);
    }

    #[test]
    fn expiry_preserves_other_sources_queued_changes() {
        let mut q = SnapshotQueue::<1, 3, 8>::new();
        q.receive(0, &[1], 0, TIMEOUT);
        q.receive(1, &[2], 400, TIMEOUT);
        q.receive(2, &[4], 450, TIMEOUT);
        q.receive(1, &[0], 500, TIMEOUT);
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 0, state: [0] }));
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 1, state: [2] }));
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 2, state: [4] }));
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 1, state: [0] }));
        assert!(q.next_report(501, TIMEOUT).is_none());
    }

    #[test]
    fn reconnect_before_timeout_poll_keeps_release_boundary() {
        let mut q = SnapshotQueue::<1, 2, 8>::new();
        q.receive(0, &[1], 0, TIMEOUT);
        q.next_report(0, TIMEOUT);
        q.receive(0, &[1], 501, TIMEOUT);
        assert_eq!(q.next_report(501, TIMEOUT).unwrap().state, [0]);
        assert_eq!(q.next_report(501, TIMEOUT).unwrap().state, [1]);
        assert!(q.next_report(501, TIMEOUT).is_none());
    }

    #[test]
    fn overflow_does_not_replay_an_expired_source() {
        let mut q = SnapshotQueue::<1, 2, 2>::new();
        q.receive(0, &[1], 0, TIMEOUT);
        q.receive(1, &[2], 400, TIMEOUT);
        q.receive(1, &[0], 501, TIMEOUT);
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 0, state: [0] }));
        assert_eq!(q.next_report(501, TIMEOUT), Some(Snapshot { source: 1, state: [0] }));
        assert!(q.next_report(501, TIMEOUT).is_none());
    }

    #[test]
    fn ring_wrap_preserves_rapid_taps() {
        let mut q = SnapshotQueue::<1, 1, 3>::new();
        for i in 0..30 {
            q.receive(0, &[1], i * 2, TIMEOUT);
            q.receive(0, &[0], i * 2 + 1, TIMEOUT);
            assert_eq!(q.next_report(i * 2 + 1, TIMEOUT).unwrap().state, [1]);
            assert_eq!(q.next_report(i * 2 + 1, TIMEOUT).unwrap().state, [0]);
        }
        assert_eq!(q.stats.overflows, 0);
    }

    #[test]
    #[should_panic]
    fn capacity_must_hold_all_sources() {
        let _ = SnapshotQueue::<1, 3, 2>::new();
    }
}
