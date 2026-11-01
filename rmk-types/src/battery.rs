//! Battery status types.

use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

/// Charge state of the battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum ChargeState {
    Charging,
    Discharging,
    Unknown,
}

impl From<bool> for ChargeState {
    /// `true` = Charging, `false` = Discharging.
    fn from(charging: bool) -> Self {
        if charging {
            ChargeState::Charging
        } else {
            ChargeState::Discharging
        }
    }
}

/// Battery status used for both status queries and event notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub enum BatteryStatus {
    Unavailable,
    Available {
        charge_state: ChargeState,
        level: Option<u8>,
    },
}

impl BatteryStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, BatteryStatus::Available { .. })
    }
}

/// Voltage on the split wire, after applying the board's battery divider.
/// Age is measured at transmission, never a remote monotonic timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BatteryVoltage {
    pub millivolts: Option<u16>,
    pub charge_state: ChargeState,
    pub age_ms: u32,
}

impl BatteryVoltage {
    pub const UNKNOWN: Self = Self {
        millivolts: None,
        charge_state: ChargeState::Unknown,
        age_ms: u32::MAX,
    };

    /// Standard BLE Battery Service still uses the existing 3.6–4.2 V curve.
    pub fn status(self) -> BatteryStatus {
        BatteryStatus::Available {
            charge_state: self.charge_state,
            level: self.millivolts.map(|mv| (mv.saturating_sub(3600) / 6).min(100) as u8),
        }
    }
}
