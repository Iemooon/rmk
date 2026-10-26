//! Keys delivered by a peripheral but not yet released.
pub(super) struct PressedKeys {
    keys: [u16; 256],
    len: usize,
}
impl PressedKeys {
    pub const fn new() -> Self {
        Self { keys: [0; 256], len: 0 }
    }
    pub fn can_press(&self, row: u8, col: u8) -> bool {
        self.len < self.keys.len() || self.keys[..self.len].contains(&u16::from_be_bytes([row, col]))
    }
    pub fn update(&mut self, row: u8, col: u8, pressed: bool) -> bool {
        let key = u16::from_be_bytes([row, col]);
        let found = self.keys[..self.len].iter().position(|k| *k == key);
        match (pressed, found) {
            (true, None) => {
                if self.len == self.keys.len() {
                    return false;
                }
                self.keys[self.len] = key;
                self.len += 1;
            }
            (false, Some(i)) => {
                self.len -= 1;
                self.keys[i] = self.keys[self.len];
            }
            _ => {}
        }
        true
    }
    pub fn pop(&mut self) -> Option<(u8, u8)> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let [row, col] = self.keys[self.len].to_be_bytes();
        Some((row, col))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disconnect_releases_only_outstanding_keys() {
        let mut keys = PressedKeys::new();
        keys.update(1, 6, true);
        keys.update(2, 8, true);
        keys.update(1, 6, false);
        assert_eq!(keys.pop(), Some((2, 8)));
        assert_eq!(keys.pop(), None);
    }
    #[test]
    fn duplicate_press_and_reconnect_do_not_replay_keys() {
        let mut keys = PressedKeys::new();
        keys.update(0, 0, true);
        keys.update(0, 0, true);
        assert_eq!(keys.pop(), Some((0, 0)));
        assert_eq!(keys.pop(), None);
        keys.update(3, 11, true);
        assert_eq!(keys.pop(), Some((3, 11)));
        assert_eq!(keys.pop(), None);
    }
    #[test]
    fn capacity_refuses_untrackable_presses_and_keeps_other_half_separate() {
        let mut left = PressedKeys::new();
        let mut right = PressedKeys::new();
        for col in 0..=255 {
            assert!(right.update(0, col, true));
        }
        assert!(!right.can_press(1, 0));
        assert!(right.can_press(0, 0));
        assert!(!right.update(1, 0, true));
        left.update(1, 2, true);
        for _ in 0..256 {
            assert!(right.pop().is_some());
        }
        assert_eq!(right.pop(), None);
        assert_eq!(left.pop(), Some((1, 2)));
    }
}
