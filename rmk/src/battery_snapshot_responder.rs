//! Reply only to a versioned, rate-limited screen query; ordinary dongles get no extra traffic.
use crate::battery_snapshot as wire;
use crate::custom_message::{CustomMessage, CustomMessageTarget, send};
use crate::event::{BatteryStatusEvent, PeripheralBatteryEvent, PeripheralConnectedEvent};
use rmk_macro::processor;
use rmk_types::battery::BatteryStatus;
#[processor(subscribe=[CustomMessage, BatteryStatusEvent, PeripheralBatteryEvent, PeripheralConnectedEvent])]
pub struct ScreenBattery {
    left: u8,
    right: u8,
    right_connected: bool,
    last_reply: Option<embassy_time::Instant>,
    central_is_right: bool,
}
impl ScreenBattery {
    /// Select the physical side occupied by the central.
    pub fn new(central_is_right: bool) -> Self {
        Self {
            left: wire::UNKNOWN,
            right: wire::UNKNOWN,
            right_connected: false,
            last_reply: None,
            central_is_right,
        }
    }
    async fn on_battery_status_event(&mut self, e: BatteryStatusEvent) {
        self.left = level(e.0);
    }
    async fn on_peripheral_battery_event(&mut self, e: PeripheralBatteryEvent) {
        if e.id == 0 {
            self.right = level(e.state.0);
        }
    }
    async fn on_peripheral_connected_event(&mut self, e: PeripheralConnectedEvent) {
        if e.id == 0 {
            self.right_connected = e.connected;
        }
    }
    async fn on_custom_message_event(&mut self, e: CustomMessage) {
        if e.target != CustomMessageTarget::Central {
            return;
        }
        let Some(seq) = wire::query_seq(&e.data) else {
            return;
        };
        if self.last_reply.is_some_and(|t| t.elapsed().as_millis() < 500) {
            return;
        }
        self.last_reply = Some(embassy_time::Instant::now());
        let sides = wire::physical_sides(self.central_is_right, self.left, self.right, self.right_connected);
        if let Ok(message) = CustomMessage::new(&wire::reply(seq, sides), CustomMessageTarget::Dongle) {
            send(message);
        }
    }
}
fn level(s: BatteryStatus) -> u8 {
    match s {
        BatteryStatus::Available { level: Some(n), .. } if n <= 100 => n,
        _ => wire::UNKNOWN,
    }
}
