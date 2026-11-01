//! WBAT v2: final mV and sample age, within the 16-byte custom-message limit.
pub const MAGIC: &[u8; 4] = b"WBAT";
pub const UNKNOWN: u16 = 0xfffe;
pub const OFFLINE: u16 = 0xffff;
pub const MAX_SAMPLE_AGE_MS: u64 = 90_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub millivolts: u16,
    /// Rounded up so transport never makes a sample appear younger.
    pub age_seconds: u8,
}
impl Sample {
    pub const OFFLINE: Self = Self {
        millivolts: OFFLINE,
        age_seconds: 255,
    };
    pub fn from_voltage(mv: Option<u16>, age_ms: u32) -> Self {
        match mv {
            Some(mv) if mv < UNKNOWN && u64::from(age_ms) < MAX_SAMPLE_AGE_MS => Self {
                millivolts: mv,
                age_seconds: age_ms.div_ceil(1000).min(255) as u8,
            },
            _ => Self {
                millivolts: UNKNOWN,
                age_seconds: 255,
            },
        }
    }
}
pub fn query(seq: u32) -> [u8; 10] {
    let mut b = [0; 10];
    b[..4].copy_from_slice(MAGIC);
    b[4..6].copy_from_slice(&[2, 1]);
    b[6..10].copy_from_slice(&seq.to_le_bytes());
    b
}
pub fn query_seq(b: &[u8]) -> Option<u32> {
    if b.len() != 10 || &b[..4] != MAGIC || b[4..6] != [2, 1] {
        return None;
    }
    Some(u32::from_le_bytes(b[6..10].try_into().ok()?))
}
pub fn reply(seq: u32, sides: [Sample; 2]) -> [u8; 16] {
    let mut b = [0; 16];
    b[..10].copy_from_slice(&query(seq));
    b[5] = 2;
    for (i, s) in sides.iter().enumerate() {
        let at = 10 + i * 3;
        b[at..at + 2].copy_from_slice(&s.millivolts.to_le_bytes());
        b[at + 2] = s.age_seconds;
    }
    b
}
pub fn decode(b: &[u8], seq: u32) -> Option<[Sample; 2]> {
    if b.len() != 16 || &b[..4] != MAGIC || b[4..6] != [2, 2] || b[6..10] != seq.to_le_bytes() {
        return None;
    }
    Some(core::array::from_fn(|i| {
        let at = 10 + i * 3;
        Sample {
            millivolts: u16::from_le_bytes([b[at], b[at + 1]]),
            age_seconds: b[at + 2],
        }
    }))
}
pub struct Client {
    sequence: u32,
    pending: Option<u32>,
    last_query: Option<u64>,
    last_reply: Option<u64>,
    samples: [Sample; 2],
    pub sides: [u16; 2],
}
impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
impl Client {
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            pending: None,
            last_query: None,
            last_reply: None,
            samples: [Sample::OFFLINE; 2],
            sides: [OFFLINE; 2],
        }
    }
    pub fn tick(&mut self, connected: bool, now: u64) -> Option<[u8; 10]> {
        if !connected {
            self.sides = [OFFLINE; 2];
            self.samples = [Sample::OFFLINE; 2];
            self.pending = None;
            self.last_query = None;
            self.last_reply = None;
            return None;
        }
        if self.last_reply.is_none_or(|t| now.saturating_sub(t) >= 35_000) {
            self.sides = [OFFLINE; 2];
        } else if let Some(at) = self.last_reply {
            for (i, s) in self.samples.iter().enumerate() {
                self.sides[i] = if s.millivolts < UNKNOWN
                    && now.saturating_sub(at).saturating_add(u64::from(s.age_seconds) * 1000) >= MAX_SAMPLE_AGE_MS
                {
                    UNKNOWN
                } else {
                    s.millivolts
                };
            }
        }
        if self.last_query.is_none_or(|t| now.saturating_sub(t) >= 30_000) {
            self.sequence = self.sequence.wrapping_add(1);
            self.pending = Some(self.sequence);
            self.last_query = Some(now);
            return Some(query(self.sequence));
        }
        None
    }
    pub fn accept(&mut self, b: &[u8], connected: bool, now: u64) -> bool {
        if !connected || self.last_query.is_none_or(|t| now.saturating_sub(t) > 1500) {
            return false;
        }
        let Some(s) = self.pending.and_then(|n| decode(b, n)) else {
            return false;
        };
        self.samples = s;
        self.sides = s.map(|v| {
            if v.millivolts < UNKNOWN && u64::from(v.age_seconds) * 1000 >= MAX_SAMPLE_AGE_MS {
                UNKNOWN
            } else {
                v.millivolts
            }
        });
        self.last_reply = Some(now);
        self.pending = None;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voltage_roundtrip_rejects_percentage_protocol_and_wrong_sequence() {
        let sides = [
            Sample::from_voltage(Some(3780), 1001),
            Sample::from_voltage(Some(4200), 0),
        ];
        let b = reply(42, sides);
        assert_eq!(b.len(), 16);
        assert_eq!(&b[10..13], &[0xc4, 0x0e, 2]);
        assert_eq!(decode(&b, 42), Some(sides));
        assert_eq!(decode(&b, 41), None);
        let mut old = b;
        old[4] = 1;
        assert_eq!(decode(&old, 42), None);
        for n in 0..16 {
            assert_eq!(decode(&b[..n], 42), None);
        }
    }
    #[test]
    fn disconnect_lease_replay_and_sample_expiry() {
        let mut c = Client::new();
        let seq = query_seq(&c.tick(true, 10).unwrap()).unwrap();
        let b = reply(seq, [Sample::from_voltage(Some(3780), 80_000), Sample::OFFLINE]);
        assert!(c.accept(&b, true, 20));
        assert_eq!(c.sides, [3780, OFFLINE]);
        assert!(!c.accept(&b, true, 21));
        c.tick(true, 10_020);
        assert_eq!(c.sides, [UNKNOWN, OFFLINE]);
        c.tick(false, 10_021);
        assert_eq!(c.sides, [OFFLINE; 2]);
        c.tick(true, 10_022);
        assert!(!c.accept(&b, true, 10_023));
        let seq = query_seq(&c.tick(true, 40_022).unwrap()).unwrap();
        assert!(c.accept(&reply(seq, [Sample::from_voltage(Some(4000), 0); 2]), true, 40_023));
        c.tick(true, 75_023);
        assert_eq!(c.sides, [OFFLINE; 2]);
    }
    #[test]
    fn stale_and_unknown_voltage_are_never_invented() {
        assert_eq!(Sample::from_voltage(Some(3780), 90_000).millivolts, UNKNOWN);
        assert_eq!(Sample::from_voltage(None, 0).millivolts, UNKNOWN);
        assert_eq!(Sample::from_voltage(Some(OFFLINE), 0).millivolts, UNKNOWN);
    }
}
