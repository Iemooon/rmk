//! Opt-in, versioned battery snapshots for display receivers.
//! Version 1 of the screen-only battery snapshot query. No settings or HID commands.
pub const MAGIC: &[u8; 4] = b"WBAT";
pub const UNKNOWN: u8 = 254;
pub const OFFLINE: u8 = 255;
/// Map a central and its first peripheral into physical left/right order.
pub fn physical_sides(central_is_right: bool, central: u8, peripheral: u8, connected: bool) -> [u8; 2] {
    let peripheral = if connected { peripheral } else { OFFLINE };
    if central_is_right { [peripheral, central] } else { [central, peripheral] }
}
pub fn query(seq: u32) -> [u8; 10] {
    let mut b = [0; 10];
    b[..4].copy_from_slice(MAGIC);
    b[4] = 1;
    b[5] = 1;
    b[6..10].copy_from_slice(&seq.to_le_bytes());
    b
}
pub fn query_seq(b: &[u8]) -> Option<u32> {
    if b.len() != 10 || &b[..4] != MAGIC || b[4..6] != [1, 1] {
        return None;
    }
    Some(u32::from_le_bytes(b[6..10].try_into().ok()?))
}
pub fn reply(seq: u32, sides: [u8; 2]) -> [u8; 12] {
    let mut b = [0; 12];
    b[..10].copy_from_slice(&query(seq));
    b[5] = 2;
    b[10..12].copy_from_slice(&sides);
    b
}
pub fn decode(b: &[u8], seq: u32) -> Option<[u8; 2]> {
    if b.len() != 12 || &b[..4] != MAGIC || b[4..6] != [1, 2] || b[6..10] != seq.to_le_bytes() {
        return None;
    }
    let sides = [b[10], b[11]];
    sides
        .iter()
        .all(|v| *v <= 100 || *v == UNKNOWN || *v == OFFLINE)
        .then_some(sides)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_reject_stale_malformed() {
        assert_eq!(physical_sides(true, 80, 30, true), [30, 80]);
        assert_eq!(physical_sides(false, 80, 30, true), [80, 30]);
        assert_eq!(physical_sides(true, 80, 30, false), [OFFLINE, 80]);
        assert_eq!(query_seq(&query(42)), Some(42));
        assert_eq!(decode(&reply(42, [25, 75]), 42), Some([25, 75]));
        assert_eq!(decode(&reply(41, [25, 75]), 42), None);
        assert_eq!(decode(&reply(42, [101, 75]), 42), None);
        for n in 0..12 {
            assert_eq!(decode(&reply(42, [0, 100])[..n], 42), None);
        }
        let mut b = reply(42, [UNKNOWN, OFFLINE]);
        b[4] = 2;
        assert_eq!(decode(&b, 42), None);
        assert_eq!(decode(&reply(42, [UNKNOWN, OFFLINE]), 42), Some([UNKNOWN, OFFLINE]));
    }
}

/// A snapshot is useful only while the current link and its reply lease are live.
pub struct Client {
    sequence: u32,
    pending: Option<u32>,
    last_query: Option<u64>,
    last_reply: Option<u64>,
    pub sides: [u8; 2],
}
impl Client {
    pub const fn new() -> Self {
        Self {
            sequence: 0,
            pending: None,
            last_query: None,
            last_reply: None,
            sides: [OFFLINE; 2],
        }
    }
    pub fn tick(&mut self, connected: bool, now: u64) -> Option<[u8; 10]> {
        if !connected {
            self.sides = [OFFLINE; 2];
            self.pending = None;
            self.last_query = None;
            self.last_reply = None;
            return None;
        }
        if self.last_reply.is_none_or(|t| now.saturating_sub(t) >= 35_000) {
            self.sides = [OFFLINE; 2];
        }
        if self.last_query.is_none_or(|t| now.saturating_sub(t) >= 30_000) {
            self.sequence = self.sequence.wrapping_add(1);
            self.pending = Some(self.sequence);
            self.last_query = Some(now);
            return Some(query(self.sequence));
        }
        None
    }
    pub fn accept(&mut self, b: &[u8], connected: bool, now: u64) {
        if !connected || self.last_query.is_none_or(|t| now.saturating_sub(t) > 1500) {
            return;
        }
        if let Some(s) = self.pending.and_then(|n| decode(b, n)) {
            self.sides = s;
            self.last_reply = Some(now);
            self.pending = None;
        }
    }
}
#[cfg(test)]
mod lease_tests {
    use super::*;
    #[test]
    fn disconnect_timeout_reconnect_and_replay() {
        let mut c = Client::new();
        assert_eq!(c.tick(false, 0), None);
        let seq = query_seq(&c.tick(true, 10).unwrap()).unwrap();
        c.accept(&reply(seq, [25, 75]), true, 20);
        assert_eq!(c.sides, [25, 75]);
        c.tick(false, 30);
        assert_eq!(c.sides, [OFFLINE; 2]);
        c.tick(true, 40);
        c.accept(&reply(seq, [25, 75]), true, 50);
        assert_eq!(c.sides, [OFFLINE; 2]);
        assert_eq!(c.tick(true, 30_039), None);
        let seq = query_seq(&c.tick(true, 30_040).unwrap()).unwrap();
        c.accept(&reply(seq, [UNKNOWN, 0]), true, 30_050);
        assert_eq!(c.sides, [UNKNOWN, 0]);
        c.tick(true, 65_049);
        assert_eq!(c.sides, [UNKNOWN, 0]);
        c.tick(true, 65_050);
        assert_eq!(c.sides, [OFFLINE; 2]);
        c.accept(&reply(seq, [100, 100]), true, 65_051);
        assert_eq!(c.sides, [OFFLINE; 2]);
    }
}
