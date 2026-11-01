//! Gazell v4 ownership and handover. No global pending-half lock.
pub const NORMAL: u8 = 0;
pub const PREPARING: u8 = 1;
pub const PROBE: u8 = 2;
pub const CLAIM: u8 = 3;
pub const STATUS: u8 = 0;
pub const PREPARE: u8 = 1;
pub const COMMIT: u8 = 2;
pub const ACCEPT: u8 = 3;
pub const PACKET_LEN: usize = 25;
pub const ACK_LEN: usize = 18;

#[derive(Clone, Copy)]
pub struct Packet {
    pub rows: [u8; 4],
    pub radio: u8,
    pub phase: u8,
    pub session: u32,
    pub generation: u64,
    pub target: u8,
}
impl Packet {
    pub fn decode(p: &[u8]) -> Option<Self> {
        if p.len() != PACKET_LEN || p[4] != 0x52 || p[5] != 4 || p[6] > 1 || p[7] > 3 || p[20] > 1 {
            return None;
        }
        let generation = u64::from_le_bytes(p[12..20].try_into().ok()?);
        if generation != 0 && generation & 1 != p[20] as u64 {
            return None;
        }
        Some(Self {
            rows: p[..4].try_into().ok()?,
            radio: p[6],
            phase: p[7],
            session: u32::from_le_bytes(p[8..12].try_into().ok()?),
            generation,
            target: p[20],
        })
    }
    pub fn empty(&self) -> bool {
        self.rows == [0; 4]
    }
}

/// User action IDs are [toggle, select A, select B]; timeout is monotonic milliseconds.
#[derive(Clone, Copy)]
pub struct Config {
    pub user_keys: [u8; 3],
    pub link_timeout_ms: u64,
}
impl Config {
    pub const DEFAULT: Self = Self {
        user_keys: [0, 1, 2],
        link_timeout_ms: 700,
    };
}
pub struct Control {
    config: Config,
    id: u8,
    generation: u64,
    target: u8,
    held: [bool; 3],
    armed: [bool; 2],
    last_seen: [Option<u64>; 2],
    claim_session: [Option<u32>; 2],
    sessions: [Option<u32>; 2],
    waking: [bool; 2],
    reset_serial: u32,
    internal_clean: bool,
    hid_mask: u8,
    pub active: bool,
}
impl Control {
    pub const fn new(id: u8) -> Self {
        Self::with_config(id, Config::DEFAULT)
    }
    pub const fn with_config(id: u8, config: Config) -> Self {
        core::assert!(id < 2 && config.link_timeout_ms > 0);
        core::assert!(
            config.user_keys[0] != config.user_keys[1]
                && config.user_keys[1] != config.user_keys[2]
                && config.user_keys[0] != config.user_keys[2]
        );
        Self {
            config,
            id,
            generation: 0,
            target: id,
            held: [false; 3],
            armed: [false; 2],
            last_seen: [None; 2],
            claim_session: [None; 2],
            sessions: [None; 2],
            waking: [false; 2],
            reset_serial: 1,
            internal_clean: false,
            hid_mask: 0,
            active: false,
        }
    }
    fn clean(&self) -> bool {
        self.internal_clean && self.hid_mask == 15
    }
    fn refresh(&mut self) {
        self.active = self.target == self.id && self.clean() && self.armed.iter().any(|x| *x);
    }
    fn reset(&mut self) {
        self.reset_serial = self.reset_serial.wrapping_add(1);
        self.internal_clean = false;
        self.hid_mask = 0;
        self.armed = [false; 2];
        self.waking = [false; 2];
        self.held = [false; 3];
        self.active = false;
    }
    fn adopt(&mut self, generation: u64, target: u8) {
        self.generation = generation;
        self.target = target;
        self.reset();
    }
    fn next_generation(&self, target: u8) -> u64 {
        ((self.generation >> 1).saturating_add(1) << 1) | target as u64
    }
    pub fn reset_request(&self) -> Option<u32> {
        (!self.clean()).then_some(self.reset_serial)
    }
    pub fn serial(&self) -> u32 {
        self.reset_serial
    }
    pub fn allow_event(&self, pressed: bool) -> bool {
        self.active || (!pressed && self.target == self.id && self.clean())
    }
    pub fn reset_done(&mut self, serial: u32) {
        if serial == self.reset_serial {
            self.internal_clean = true;
            self.refresh();
        }
    }
    pub fn hid_done(&mut self, serial: u32, bit: u8) {
        if serial == self.reset_serial {
            self.hid_mask |= bit;
            self.refresh();
        }
    }
    pub fn user(&mut self, id: u8, pressed: bool) -> bool {
        let Some(i) = self.config.user_keys.iter().position(|&key| key == id) else {
            return false;
        };
        let rising = pressed && !self.held[i];
        self.held[i] = pressed;
        if !rising || !self.active {
            return false;
        }
        let target = match i {
            0 => 1 - self.id,
            1 => 0,
            _ => 1,
        };
        if target == self.id {
            return false;
        }
        self.adopt(self.next_generation(target), target);
        true
    }
    pub fn expire(&mut self, now: u64) {
        for half in 0..2 {
            if self.last_seen[half].is_some_and(|t| now.saturating_sub(t) > self.config.link_timeout_ms) {
                self.last_seen[half] = None;
                self.armed[half] = false;
            }
        }
        if self.active && !self.armed.iter().any(|x| *x) {
            self.reset();
        } else {
            self.refresh();
        }
    }
    pub fn matrix_allowed(&self, half: usize, p: &Packet) -> bool {
        half < 2
            && self.active
            && self.armed[half]
            && p.phase == NORMAL
            && p.radio == self.id
            && p.target == self.id
            && p.generation == self.generation
    }
    pub fn received(&mut self, half: usize, p: &Packet, now: u64) -> Option<[u8; ACK_LEN]> {
        if half > 1 || p.radio != self.id {
            return None;
        }
        // Probes answer discovery only: they never acquire ownership or feed a matrix.
        if p.phase != PROBE {
            // SYSTEMOFF restarts the transmitter with generation zero. This
            // is a wake on the saved radio, not a handover carrying held keys.
            if p.phase == NORMAL && self.sessions[half] != Some(p.session) {
                self.sessions[half] = Some(p.session);
                self.armed[half] = false;
                self.waking[half] = p.generation == 0 && p.target == self.id;
            }
            if p.phase == CLAIM && p.empty() && self.claim_session[half] != Some(p.session) {
                self.claim_session[half] = Some(p.session);
                if self.target != self.id || p.generation > self.generation || self.generation == 0 {
                    self.generation = self.generation.max(p.generation);
                    self.adopt(self.next_generation(self.id), self.id);
                }
            } else if p.phase == NORMAL && p.target == self.id && p.generation > self.generation {
                self.adopt(p.generation, self.id);
            }
            if p.phase == NORMAL && p.target == self.id && p.generation == self.generation {
                self.last_seen[half] = Some(now);
                if self.clean() && (p.empty() || self.waking[half]) {
                    self.armed[half] = true;
                    self.waking[half] = false;
                }
            }
            self.refresh();
        }
        let opcode = if p.phase == PROBE {
            STATUS
        } else if p.phase == CLAIM {
            if self.target != self.id {
                PREPARE
            } else if self.clean() && p.empty() {
                ACCEPT
            } else {
                STATUS
            }
        } else if self.target != self.id {
            if p.phase == PREPARING
                && p.generation == self.generation
                && p.target == self.target
                && p.empty()
                && self.clean()
            {
                COMMIT
            } else {
                PREPARE
            }
        } else {
            STATUS
        };
        let mut ack = [0; ACK_LEN];
        ack[0] = 0x52;
        ack[1] = 2;
        ack[2] = opcode;
        ack[3] = self.id;
        ack[4] = self.target;
        ack[6..10].copy_from_slice(&p.session.to_le_bytes());
        ack[10..18].copy_from_slice(&self.generation.to_le_bytes());
        Some(ack)
    }
}

/// Retry USB cleanup without continuously draining the keyboard event queue.
pub struct ResetRetry {
    last: Option<(u32, u64)>,
}
impl ResetRetry {
    pub const fn new() -> Self {
        Self { last: None }
    }
    pub fn due(&mut self, request: Option<u32>, now: u64) -> bool {
        let Some(serial) = request else {
            return false;
        };
        if self
            .last
            .is_none_or(|(s, t)| s != serial || now.saturating_sub(t) >= 100)
        {
            self.last = Some((serial, now));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(radio: u8, phase: u8, g: u64, target: u8, session: u32) -> Packet {
        Packet {
            rows: [0; 4],
            radio,
            phase,
            session,
            generation: g,
            target,
        }
    }
    fn clean(c: &mut Control) {
        let s = c.serial();
        c.reset_done(s);
        c.hid_done(s, 15);
    }
    fn online(id: u8) -> Control {
        let mut c = Control::new(id);
        clean(&mut c);
        c.received(0, &p(id, NORMAL, 0, id, 1), 0);
        c
    }
    #[test]
    fn configurable_user_ids_and_timeout() {
        let mut a = Control::with_config(
            0,
            Config {
                user_keys: [10, 11, 12],
                link_timeout_ms: 1500,
            },
        );
        clean(&mut a);
        a.received(0, &p(0, NORMAL, 0, 0, 1), 0);
        a.expire(800);
        assert!(a.active);
        assert!(!a.user(0, true));
        assert!(a.user(12, true));
        assert_eq!(a.target, 1);
    }
    #[test]
    fn commit_requires_software_and_usb_cleanup_and_all_up() {
        let mut a = online(0);
        assert!(a.user(0, true));
        let mut ready = p(0, PREPARING, 3, 1, 1);
        assert_eq!(a.received(0, &ready, 1).unwrap()[2], PREPARE);
        a.reset_done(a.serial());
        assert_eq!(a.received(0, &ready, 2).unwrap()[2], PREPARE);
        a.hid_done(a.serial(), 15);
        ready.rows[2] = 64;
        assert_eq!(a.received(0, &ready, 3).unwrap()[2], PREPARE);
        ready.rows = [0; 4];
        assert_eq!(a.received(0, &ready, 4).unwrap()[2], COMMIT);
    }
    #[test]
    fn return_supersedes_sleeping_half_task() {
        let mut a = online(0);
        assert!(a.user(0, true));
        clean(&mut a);
        let mut b = Control::new(1);
        b.received(0, &p(1, NORMAL, 3, 1, 2), 0);
        clean(&mut b);
        b.received(0, &p(1, NORMAL, 3, 1, 2), 1);
        assert!(b.user(0, true)); // generation 4, return to A while right still asleep on A
        a.received(0, &p(0, NORMAL, 4, 0, 3), 2);
        assert!(!a.active);
        clean(&mut a);
        a.received(0, &p(0, NORMAL, 4, 0, 3), 3);
        assert!(a.active);
        let ack = a.received(1, &p(0, NORMAL, 0, 0, 4), 4).unwrap();
        assert_eq!(ack[2], STATUS);
        assert_eq!(ack[4], 0);
        assert_eq!(u64::from_le_bytes(ack[10..18].try_into().unwrap()), 4);
    }
    #[test]
    fn probe_never_changes_ownership_or_admits_keys() {
        let mut a = online(0);
        assert!(a.user(2, true));
        clean(&mut a);
        let probe = p(0, PROBE, 99, 1, 9);
        assert_eq!(a.received(1, &probe, 3).unwrap()[2], STATUS);
        assert!(!a.active);
        assert_eq!(a.generation, 3);
    }
    #[test]
    fn fallback_claim_replaces_old_route_and_is_idempotent() {
        let mut a = online(0);
        a.user(2, true);
        let claim = p(0, CLAIM, 3, 1, 9);
        assert_eq!(a.received(1, &claim, 1).unwrap()[2], STATUS);
        assert_eq!(a.generation, 4);
        clean(&mut a);
        let ack = a.received(1, &claim, 2).unwrap();
        assert_eq!(ack[2], ACCEPT);
        assert_eq!(a.generation, 4);
        a.received(1, &p(0, NORMAL, 4, 0, 9), 3);
        assert!(a.active);
    }
    #[test]
    fn late_old_half_cannot_reactivate_departed_receiver() {
        let mut a = online(0);
        a.user(0, true);
        clean(&mut a);
        let old = p(0, NORMAL, 0, 0, 8);
        let ack = a.received(1, &old, 1).unwrap();
        assert_eq!(ack[2], PREPARE);
        assert!(!a.matrix_allowed(1, &old));
    }
    #[test]
    fn arriving_held_key_waits_for_full_release() {
        let mut b = Control::new(1);
        let mut held = p(1, NORMAL, 3, 1, 8);
        held.rows[0] = 1;
        b.received(0, &held, 0);
        clean(&mut b);
        b.received(0, &held, 1);
        assert!(!b.active);
        held.rows = [0; 4];
        b.received(0, &held, 2);
        assert!(b.matrix_allowed(0, &held));
    }
    #[test]
    fn stale_reset_completion_cannot_approve_new_handover() {
        let mut a = online(0);
        let s = a.serial();
        a.user(0, true);
        a.reset_done(s);
        a.hid_done(s, 15);
        assert!(!a.clean());
    }
    #[test]
    fn unplug_timeout_closes_gate_and_requests_reset() {
        let mut a = online(0);
        assert!(a.active);
        a.expire(701);
        assert!(!a.active);
        assert!(a.reset_request().is_some());
    }

    #[test]
    fn second_half_claim_does_not_clear_already_working_half() {
        let mut a = online(0);
        a.received(0, &p(0, CLAIM, 0, 0, 7), 1);
        clean(&mut a);
        a.received(0, &p(0, NORMAL, 2, 0, 7), 2);
        assert!(a.active);
        let serial = a.serial();
        let ack = a.received(1, &p(0, CLAIM, 0, 0, 8), 3).unwrap();
        assert_eq!(ack[2], ACCEPT);
        assert_eq!(a.serial(), serial);
        assert!(a.active);
    }
    #[test]
    fn newer_user_task_redirects_inflight_claim_without_loop() {
        let mut a = online(0);
        a.received(1, &p(0, CLAIM, 0, 0, 9), 1);
        clean(&mut a);
        a.received(0, &p(0, NORMAL, 2, 0, 7), 2);
        assert!(a.user(2, true));
        clean(&mut a);
        let ack = a.received(1, &p(0, CLAIM, 0, 0, 9), 3).unwrap();
        assert_eq!(ack[2], PREPARE);
        assert_eq!(ack[4], 1);
        assert_eq!(a.generation, 5);
    }
    #[test]
    fn usb_not_ready_retries_cleanup_instead_of_sticking_at_boot() {
        let mut a = Control::new(0);
        let serial = a.serial();
        a.reset_done(serial);
        assert_eq!(a.reset_request(), Some(serial));
        a.hid_done(serial, 7);
        assert_eq!(a.reset_request(), Some(serial));
        a.hid_done(serial, 8);
        assert_eq!(a.reset_request(), None);
    }
    #[test]
    fn wire_validation_rejects_bad_version_and_inconsistent_route() {
        let mut bytes = [0; PACKET_LEN];
        bytes[4] = 0x52;
        bytes[5] = 4;
        bytes[12] = 3;
        bytes[20] = 1;
        assert!(Packet::decode(&bytes).is_some());
        assert!(Packet::decode(&bytes[..24]).is_none());
        bytes[20] = 0;
        assert!(Packet::decode(&bytes).is_none());
        bytes[20] = 1;
        bytes[5] = 3;
        assert!(Packet::decode(&bytes).is_none());
    }

    #[test]
    fn handover_session_cannot_carry_a_held_key_through_old_ready_state() {
        let mut a = online(0);
        a.generation = 2;
        assert!(a.active);
        let mut waking = p(0, NORMAL, 2, 0, 99);
        waking.rows[2] = 64;
        a.received(0, &waking, 2);
        assert!(!a.matrix_allowed(0, &waking));
        waking.rows = [0; 4];
        a.received(0, &waking, 3);
        assert!(a.matrix_allowed(0, &waking));
    }
    #[test]
    fn idle_wake_preserves_first_press_on_both_halves() {
        for half in 0..2 {
            let mut a = online(0);
            a.generation = 4;
            a.expire(1000);
            clean(&mut a);
            let mut wake = p(0, NORMAL, 0, 0, 99);
            wake.rows[2] = 64;
            let serial = a.serial();
            a.received(half, &wake, 1001);
            wake.generation = 4; // Learned from STATUS ACK, key still down.
            a.received(half, &wake, 1006);
            assert!(a.matrix_allowed(half, &wake));
            assert_eq!(a.serial(), serial);
        }
    }
    #[test]
    fn cold_start_wake_keeps_first_press_but_waits_for_cleanup() {
        let mut a = Control::new(0);
        let mut wake = p(0, NORMAL, 0, 0, 99);
        wake.rows[0] = 1;
        a.received(0, &wake, 0);
        assert!(!a.matrix_allowed(0, &wake));
        clean(&mut a);
        a.received(0, &wake, 5);
        assert!(a.matrix_allowed(0, &wake));
    }
    #[test]
    fn cleanup_retry_is_bounded_and_new_switch_is_immediate() {
        let mut retry = ResetRetry::new();
        assert!(retry.due(Some(1), 0));
        for now in 1..100 {
            assert!(!retry.due(Some(1), now));
        }
        assert!(retry.due(Some(1), 100));
        assert!(retry.due(Some(2), 101));
        assert!(!retry.due(None, 1000));
    }
    #[test]
    fn departed_receiver_drops_stale_release_actions_after_reset() {
        let mut a = online(0);
        assert!(a.allow_event(false));
        a.user(0, true);
        assert!(!a.allow_event(false));
        clean(&mut a);
        assert!(!a.allow_event(false));
        assert!(!a.allow_event(true));
    }
}
