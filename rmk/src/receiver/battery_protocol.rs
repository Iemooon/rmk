//! EBAT v1: read-only, 32-byte request/reply on a dedicated HID collection.
use super::battery::Reading;

pub const SIZE: usize = 32;

pub fn reply(
    request: &[u8],
    readings: [Option<Reading>; 2],
    now_ms: u64,
    receiver: u8,
    active: bool,
) -> Option<[u8; SIZE]> {
    if request.len() != SIZE || &request[..4] != b"EBAT" {
        return None;
    }
    let mut out = [0; SIZE];
    out[..7].copy_from_slice(&request[..7]);
    // status: 0=OK, 1=unsupported protocol version, 2=unsupported command.
    out[7] = if request[4] != 1 {
        1
    } else if request[5] != 1 {
        2
    } else {
        0
    };
    out[8] = receiver; // 0=A, 1=B
    out[9] = u8::from(active);
    if out[7] != 0 {
        return Some(out);
    }
    for (index, reading) in readings.iter().enumerate() {
        let base = 12 + index * 10;
        let mut age = u32::MAX;
        if let Some(reading) = reading {
            out[base] = 1; // type received
            out[base + 1] = reading.battery_type as u8;
            if let Some(mv) = reading.millivolts {
                out[base] |= 2; // voltage available
                out[base + 2..base + 4].copy_from_slice(&mv.to_le_bytes());
            }
            age = now_ms.saturating_sub(reading.received_at_ms).min(u32::MAX as u64) as u32;
        }
        out[base + 4..base + 8].copy_from_slice(&age.to_le_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::super::battery::BatteryType;
    use super::*;
    fn request() -> [u8; 32] {
        let mut r = [0; 32];
        r[..7].copy_from_slice(b"EBAT\x01\x01\x29");
        r
    }
    #[test]
    fn independent_records_and_wire_layout() {
        let readings = [
            Some(Reading {
                battery_type: BatteryType::CoinCell,
                millivolts: Some(2950),
                received_at_ms: 100,
            }),
            Some(Reading {
                battery_type: BatteryType::Lithium,
                millivolts: Some(4210),
                received_at_ms: 200,
            }),
        ];
        let r = reply(&request(), readings, 300, 1, false).unwrap();
        assert_eq!(&r[..10], b"EBAT\x01\x01\x29\x00\x01\x00");
        assert_eq!(&r[12..20], &[3, 1, 0x86, 0x0b, 200, 0, 0, 0]);
        assert_eq!(&r[22..30], &[3, 2, 0x72, 0x10, 100, 0, 0, 0]);
        assert_eq!(&r[30..], &[0, 0]);
    }
    #[test]
    fn missing_invalid_and_old_records() {
        let unknown = Some(Reading {
            battery_type: BatteryType::CoinCell,
            millivolts: None,
            received_at_ms: 0,
        });
        let r = reply(&request(), [None, unknown], u64::MAX, 0, true).unwrap();
        assert_eq!(&r[12..20], &[0, 0, 0, 0, 255, 255, 255, 255]);
        assert_eq!(&r[22..30], &[1, 1, 0, 0, 255, 255, 255, 255]);
    }
    #[test]
    fn rejects_bad_frames_and_reports_unsupported_commands() {
        let mut req = request();
        assert!(reply(&req[..31], [None; 2], 0, 0, true).is_none());
        req[0] = 0;
        assert!(reply(&req, [None; 2], 0, 0, true).is_none());
        req = request();
        req[4] = 2;
        assert_eq!(reply(&req, [None; 2], 0, 0, true).unwrap()[7], 1);
        req = request();
        req[5] = 2;
        assert_eq!(reply(&req, [None; 2], 0, 0, true).unwrap()[7], 2);
    }
}
