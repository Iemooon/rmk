//! Storage-format and timing policy, independent of hardware for fault tests.
pub const SLOT: u8 = 240;
pub const ARMED: &[u8] = b"WKDFU1:A";
pub const CLEAR: &[u8] = b"WKDFU1:C";
pub const ARM_MS: u64 = 15_000;
pub const CLEAR_MS: u64 = 50_000;

pub fn is_armed(value: Option<&[u8]>) -> bool {
    value == Some(ARMED)
}
pub fn clear_confirmed(value: Option<&[u8]>) -> bool {
    value == Some(CLEAR)
}
pub fn may_arm(elapsed_ms: u64) -> bool {
    (ARM_MS..CLEAR_MS).contains(&elapsed_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Simulate one powered session and then losing RAM at a chosen time.
    fn cut_power(at: u64) -> Option<&'static [u8]> {
        let mut flash = None;
        if at >= ARM_MS {
            flash = Some(ARMED);
        }
        if at >= CLEAR_MS {
            flash = Some(CLEAR);
        }
        flash
    }
    #[test]
    fn only_the_15_to_50_second_window_survives_power_loss_armed() {
        for (ms, expected) in [
            (0, false),
            (14_999, false),
            (15_000, true),
            (30_000, true),
            (49_999, true),
            (50_000, false),
            (90_000, false),
        ] {
            assert_eq!(is_armed(cut_power(ms)), expected, "{ms}");
        }
    }
    #[test]
    fn unknown_empty_and_torn_records_do_not_trigger() {
        for bytes in [
            None,
            Some(&b""[..]),
            Some(&b"A"[..]),
            Some(&b"WKDFU1:"[..]),
            Some(&b"WKDFU2:A"[..]),
            Some(CLEAR),
        ] {
            assert!(!is_armed(bytes));
        }
    }
    #[test]
    fn failed_clear_or_read_error_never_permits_a_jump() {
        for readback in [
            None,
            Some(ARMED),
            Some(&b""[..]),
            Some(&b"C"[..]),
            Some(&b"WKDFU1:"[..]),
        ] {
            assert!(!clear_confirmed(readback));
        }
        assert!(clear_confirmed(Some(CLEAR)));
    }
    #[test]
    fn consumed_marker_does_not_reenter_dfu_when_bootloader_returns_to_app() {
        let mut flash = cut_power(25_000);
        assert!(is_armed(flash));
        flash = Some(CLEAR);
        assert!(clear_confirmed(flash));
        assert!(!is_armed(flash));
    }
    #[test]
    fn a_delayed_storage_start_does_not_open_a_late_window() {
        assert!(!may_arm(14_999));
        assert!(may_arm(15_000));
        assert!(may_arm(49_999));
        assert!(!may_arm(50_000));
        assert!(!may_arm(120_000));
    }
}
