//! Final voltage telemetry. Sampling and chemistry conversion belong to the transmitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BatteryType {
    CoinCell = 1,
    Lithium = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    pub battery_type: BatteryType,
    pub millivolts: Option<u16>,
    /// Local receive time, not ADC sample time.
    pub received_at_ms: u64,
}
/// Application-owned RAM store. No allocation, radio, global state or percentage conversion.
pub struct BatteryStore<const SOURCES: usize = 2> {
    readings: [Option<Reading>; SOURCES],
}
impl<const SOURCES: usize> Default for BatteryStore<SOURCES> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const SOURCES: usize> BatteryStore<SOURCES> {
    pub const fn new() -> Self {
        Self {
            readings: [None; SOURCES],
        }
    }
    /// False for an invalid source; other sources are unchanged.
    pub fn record(&mut self, source: usize, reading: Reading) -> bool {
        if let Some(slot) = self.readings.get_mut(source) {
            *slot = Some(reading);
            true
        } else {
            false
        }
    }
    pub fn snapshot(&self) -> [Option<Reading>; SOURCES] {
        self.readings
    }
    pub fn clear(&mut self, source: usize) {
        if let Some(slot) = self.readings.get_mut(source) {
            *slot = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_source_count_is_independent() {
        let mut store = BatteryStore::<3>::new();
        let a = Reading {
            battery_type: BatteryType::CoinCell,
            millivolts: Some(2950),
            received_at_ms: 12,
        };
        let b = Reading {
            battery_type: BatteryType::Lithium,
            millivolts: None,
            received_at_ms: 20,
        };
        assert!(store.record(0, a));
        assert!(store.record(2, b));
        assert!(!store.record(3, b));
        assert_eq!(store.snapshot(), [Some(a), None, Some(b)]);
    }
}
