//! Opt-in WBAT voltage client shared by ordinary and display BLE receivers.
use super::battery::{BatteryStore, BatteryType, Reading};
use crate::battery_snapshot as wire;
use crate::custom_message::{CustomMessage, CustomMessageTarget, send};
use crate::event::{DongleState, DongleStateEvent, EventSubscriber, SubscribableEvent};
use core::cell::RefCell;
use embassy_futures::select::{Either3, select3};
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Instant, Timer};

static STORE: Mutex<crate::RawMutex, RefCell<BatteryStore>> = Mutex::new(RefCell::new(BatteryStore::new()));

pub fn snapshot() -> [Option<Reading>; 2] {
    STORE.lock(|store| store.borrow().snapshot())
}

/// Existing ring displays can continue showing a percentage, computed locally.
pub fn percentages() -> [u8; 2] {
    snapshot().map(|reading| match reading {
        None => 255,
        Some(Reading { millivolts: None, .. }) => 254,
        Some(Reading {
            millivolts: Some(mv), ..
        }) => (mv.saturating_sub(3600) / 6).min(100) as u8,
    })
}

pub struct BatteryClient;
impl BatteryClient {
    pub async fn run(&mut self) -> ! {
        let mut messages = CustomMessage::subscriber();
        let mut states = DongleStateEvent::subscriber();
        let mut client = wire::Client::new();
        let mut connected = false;
        let mut received_at = 0;
        loop {
            let now = Instant::now().as_millis();
            if let Some(query) = client.tick(connected, now)
                && let Ok(message) = CustomMessage::new(&query, CustomMessageTarget::Central)
            {
                send(message);
            }
            STORE.lock(|store| {
                let mut store = store.borrow_mut();
                for (i, mv) in client.sides.iter().copied().enumerate() {
                    if mv == wire::OFFLINE {
                        store.clear(i);
                    } else {
                        store.record(
                            i,
                            Reading {
                                battery_type: BatteryType::Lithium,
                                millivolts: (mv < wire::UNKNOWN).then_some(mv),
                                received_at_ms: received_at,
                            },
                        );
                    }
                }
            });
            match select3(states.next_event(), messages.next_event(), Timer::after_millis(100)).await {
                Either3::First(state) => {
                    connected = state.0 == DongleState::Connected;
                    // Any link transition invalidates the previous keyboard's readings.
                    client.tick(false, Instant::now().as_millis());
                }
                Either3::Second(message) if message.target == CustomMessageTarget::Dongle => {
                    let now = Instant::now().as_millis();
                    if client.accept(&message.data, connected, now) {
                        received_at = now;
                    }
                }
                _ => {}
            }
        }
    }
}
