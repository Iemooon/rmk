//! WBAT v2 voltage replies; ordinary dongles never trigger extra snapshots.
use crate::battery_snapshot as wire;
use crate::custom_message::{CustomMessage, CustomMessageTarget, send};
use rmk_macro::processor;
#[processor(subscribe=[CustomMessage])]
pub struct ScreenBattery {
    last_reply: Option<embassy_time::Instant>,
    central_is_right: bool,
}
impl ScreenBattery {
    pub fn new(central_is_right: bool) -> Self {
        Self {
            last_reply: None,
            central_is_right,
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
        let central = crate::input_device::battery::current_battery_voltage();
        let central = wire::Sample::from_voltage(central.millivolts, central.age_ms);
        let peripheral = crate::split::driver::current_peripheral_voltage(0).map_or(wire::Sample::OFFLINE, |v| {
            wire::Sample::from_voltage(v.millivolts, v.age_ms)
        });
        let sides = if self.central_is_right {
            [peripheral, central]
        } else {
            [central, peripheral]
        };
        if let Ok(message) = CustomMessage::new(&wire::reply(seq, sides), CustomMessageTarget::Dongle) {
            send(message);
        }
    }
}
