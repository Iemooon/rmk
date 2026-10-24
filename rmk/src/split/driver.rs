//! The abstracted driver layer of the split keyboard.
//!
use core::cell::Cell;

use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use futures::FutureExt;
use rmk_types::battery::BatteryStatus;
#[cfg(feature = "rynk")]
use rmk_types::protocol::rynk::PeripheralStatus;

use super::{PeripheralMatrixConfig, SplitMessage};
#[cfg(feature = "dfu_split")]
use crate::event::DfuCmdEvent;
#[cfg(feature = "_ble")]
use crate::event::{BatteryStatusEvent, PeripheralBatteryEvent};
use crate::event::{
    KeyboardEvent, KeyboardEventSource, PeripheralConnectedEvent, SubscribableEvent, publish_event, publish_event_async,
};

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub(crate) enum SplitDriverError {
    SerialError,
    EmptyMessage,
    DeserializeError,
    SerializeError,
    BleError(u8),
    Disconnected,
}

#[cfg(feature = "_ble")]
static PRESSED_KEYS: BlockingMutex<
    crate::RawMutex,
    core::cell::RefCell<[super::pressed::PressedKeys; crate::SPLIT_PERIPHERALS_NUM]>,
> = BlockingMutex::new(core::cell::RefCell::new(
    [const { super::pressed::PressedKeys::new() }; crate::SPLIT_PERIPHERALS_NUM],
));

#[cfg(feature = "_ble")]
pub(crate) async fn release_peripheral_keys(id: usize) {
    loop {
        let key = PRESSED_KEYS.lock(|keys| keys.borrow_mut().get_mut(id).and_then(|keys| keys.pop()));
        let Some((row, col)) = key else {
            break;
        };
        publish_event_async(KeyboardEvent::key(row, col, false)).await;
    }
}

/// Split message reader from other split devices
pub(crate) trait SplitReader {
    async fn read(&mut self) -> Result<SplitMessage, SplitDriverError>;
}

/// Split message writer to other split devices
pub(crate) trait SplitWriter {
    async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError>;
}

/// Live per-peripheral status. Latched here in the transport-agnostic split
/// layer so host services can read a current snapshot at any time, even when
/// no host session was active when the change happened. Wired peripherals
/// never report a battery, so theirs stays `Unavailable`.
#[derive(Copy, Clone, PartialEq, Eq)]
struct PeripheralSlot {
    connected: bool,
    battery: BatteryStatus,
}

static PERIPHERAL_SLOTS: BlockingMutex<crate::RawMutex, Cell<[PeripheralSlot; crate::SPLIT_PERIPHERALS_NUM]>> =
    BlockingMutex::new(Cell::new(
        [PeripheralSlot {
            connected: false,
            battery: BatteryStatus::Unavailable,
        }; crate::SPLIT_PERIPHERALS_NUM],
    ));

/// Read-modify-write peripheral `id`'s slot. Returns `false` when `id` is out
/// of range or the slot didn't change, so callers skip publishing.
fn update_slot(id: usize, f: impl FnOnce(&mut PeripheralSlot)) -> bool {
    PERIPHERAL_SLOTS.lock(|slots| {
        let mut all = slots.get();
        let Some(slot) = all.get_mut(id) else {
            return false;
        };
        let prev = *slot;
        f(slot);
        if *slot == prev {
            return false;
        }
        slots.set(all);
        true
    })
}

/// Latch peripheral `id`'s connected state and broadcast the change.
pub(crate) fn set_peripheral_connected(id: usize, connected: bool) {
    if update_slot(id, |s| s.connected = connected) {
        publish_event(PeripheralConnectedEvent { id, connected });
    }
}

/// Latch peripheral `id`'s battery status and broadcast the change.
#[cfg(feature = "_ble")]
pub(crate) fn set_peripheral_battery(id: usize, battery: BatteryStatus) {
    if update_slot(id, |s| s.battery = battery) {
        publish_event(PeripheralBatteryEvent {
            id,
            state: BatteryStatusEvent(battery),
        });
    }
}

/// Latest battery status reported by peripheral `id`.
#[cfg(feature = "_ble")]
pub(crate) fn current_peripheral_battery_status(id: usize) -> Option<BatteryStatus> {
    PERIPHERAL_SLOTS.lock(|slots| slots.get().get(id).map(|slot| slot.battery))
}

/// Latest snapshot for peripheral `id`, or `None` when `id` is out of range.
#[cfg(feature = "rynk")]
pub(crate) fn current_peripheral_status(id: usize) -> Option<PeripheralStatus> {
    PERIPHERAL_SLOTS.lock(|slots| {
        slots.get().get(id).map(|s| PeripheralStatus {
            connected: s.connected,
            battery: s.battery,
        })
    })
}

#[cfg(all(test, feature = "_ble"))]
mod tests {
    use rmk_types::battery::ChargeState;

    use super::{current_peripheral_battery_status, set_peripheral_battery};

    struct TestDriver;
    impl super::SplitReader for TestDriver {
        async fn read(&mut self) -> Result<super::SplitMessage, super::SplitDriverError> {
            core::future::pending().await
        }
    }
    impl super::SplitWriter for TestDriver {
        async fn write(&mut self, _: &super::SplitMessage) -> Result<usize, super::SplitDriverError> {
            Ok(0)
        }
    }
    #[test]
    fn disconnect_publishes_release_at_global_peripheral_coordinates() {
        use embassy_futures::block_on;

        use crate::event::{EventSubscriber, KeyboardEvent, SubscribableEvent};
        let mut events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        block_on(manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(2, 3, true))));
        assert_eq!(block_on(events.next_event()), KeyboardEvent::key(2, 9, true));
        block_on(super::release_peripheral_keys(0));
        let release = block_on(async { futures::poll!(core::pin::pin!(events.next_event())) });
        assert_eq!(release, core::task::Poll::Ready(KeyboardEvent::key(2, 9, false)));
        assert!(super::PRESSED_KEYS.lock(|keys| keys.borrow_mut()[0].pop()).is_none());
    }

    #[test]
    fn cancelled_release_keeps_key_for_disconnect_cleanup() {
        use embassy_futures::block_on;

        use crate::event::{EventSubscriber, KeyboardEvent, SubscribableEvent, publish_event_async};
        let mut events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        block_on(manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(2, 3, true))));
        let _ = block_on(events.next_event());
        for _ in 0..crate::KEYBOARD_EVENT_CHANNEL_SIZE {
            block_on(publish_event_async(KeyboardEvent::key(0, 0, false)));
        }
        {
            let mut release = core::pin::pin!(
                manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(2, 3, false)))
            );
            block_on(async {
                assert!(futures::poll!(release.as_mut()).is_pending());
            });
        }
        assert_eq!(
            super::PRESSED_KEYS.lock(|keys| keys.borrow_mut()[0].pop()),
            Some((2, 9))
        );
    }

    #[test]
    fn cancelled_press_does_not_invent_a_key_for_disconnect_cleanup() {
        use embassy_futures::block_on;

        use crate::event::{KeyboardEvent, SubscribableEvent, publish_event_async};
        let _events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        for _ in 0..crate::KEYBOARD_EVENT_CHANNEL_SIZE {
            block_on(publish_event_async(KeyboardEvent::key(0, 0, false)));
        }
        {
            let mut press = core::pin::pin!(
                manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(2, 3, true)))
            );
            block_on(async {
                assert!(futures::poll!(press.as_mut()).is_pending());
            });
        }
        assert_eq!(super::PRESSED_KEYS.lock(|keys| keys.borrow_mut()[0].pop()), None);
    }

    #[test]
    fn disconnect_preserves_local_input_and_does_not_replay_on_reconnect() {
        use embassy_futures::block_on;

        use crate::event::{EventSubscriber, KeyboardEvent, SubscribableEvent, publish_event_async};
        let mut events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        block_on(publish_event_async(KeyboardEvent::key(0, 0, true)));
        assert_eq!(block_on(events.next_event()), KeyboardEvent::key(0, 0, true));
        for (row, col, pressed) in [(1, 1, true), (2, 2, true), (1, 1, false)] {
            block_on(
                manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(row, col, pressed))),
            );
            assert_eq!(block_on(events.next_event()), KeyboardEvent::key(row, col + 6, pressed));
        }
        block_on(super::release_peripheral_keys(0));
        assert_eq!(block_on(events.next_event()), KeyboardEvent::key(2, 8, false));
        block_on(super::release_peripheral_keys(0));
        block_on(async {
            assert!(futures::poll!(core::pin::pin!(events.next_event())).is_pending());
        });
        block_on(manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(2, 2, true))));
        assert_eq!(block_on(events.next_event()), KeyboardEvent::key(2, 8, true));
        block_on(super::release_peripheral_keys(0));
        assert_eq!(block_on(events.next_event()), KeyboardEvent::key(2, 8, false));
        block_on(async {
            assert!(futures::poll!(core::pin::pin!(events.next_event())).is_pending());
        });
    }

    #[test]
    fn disconnect_cleanup_waits_for_backpressure_without_losing_releases() {
        use embassy_futures::{block_on, join::join};

        use crate::event::{EventSubscriber, KeyboardEvent, SubscribableEvent, publish_event_async};
        let mut events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        for col in [1, 2] {
            block_on(manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(1, col, true))));
            let _ = block_on(events.next_event());
        }
        for _ in 0..crate::KEYBOARD_EVENT_CHANNEL_SIZE {
            block_on(publish_event_async(KeyboardEvent::key(0, 0, false)));
        }
        let mut cleanup = core::pin::pin!(super::release_peripheral_keys(0));
        block_on(async {
            assert!(futures::poll!(cleanup.as_mut()).is_pending());
        });
        block_on(join(cleanup, async {
            for _ in 0..crate::KEYBOARD_EVENT_CHANNEL_SIZE {
                assert_eq!(events.next_event().await, KeyboardEvent::key(0, 0, false));
            }
            let a = events.next_event().await;
            let b = events.next_event().await;
            assert!(a != b);
            for release in [a, b] {
                assert!(release == KeyboardEvent::key(1, 7, false) || release == KeyboardEvent::key(1, 8, false));
            }
        }));
        assert!(super::PRESSED_KEYS.lock(|keys| keys.borrow_mut()[0].pop()).is_none());
    }

    #[test]
    fn invalid_peripheral_coordinates_never_reach_disconnect_cleanup() {
        use embassy_futures::block_on;

        use crate::event::{EventSubscriber, KeyboardEvent, SubscribableEvent};
        let mut events = KeyboardEvent::subscriber();
        let manager = super::PeripheralManager::new(
            TestDriver,
            0,
            super::PeripheralMatrixConfig {
                rows: 4,
                cols: 6,
                row_offset: 0,
                col_offset: 6,
            },
        );
        for (row, col) in [(4, 0), (0, 6), (255, 255)] {
            block_on(manager.process_peripheral_message(super::SplitMessage::Key(KeyboardEvent::key(row, col, true))));
        }
        block_on(super::release_peripheral_keys(0));
        block_on(async {
            assert!(futures::poll!(core::pin::pin!(events.next_event())).is_pending());
        });
    }

    #[test]
    fn caches_latest_peripheral_battery_status() {
        let status = rmk_types::battery::BatteryStatus::Available {
            charge_state: ChargeState::Discharging,
            level: Some(73),
        };

        set_peripheral_battery(0, status);

        assert_eq!(current_peripheral_battery_status(0), Some(status));
        assert_eq!(current_peripheral_battery_status(crate::SPLIT_PERIPHERALS_NUM), None);
    }
}

/// PeripheralManager runs on the central and manages communication with one split peripheral.
/// It publishes input events from that peripheral and sends the central's state changes back.
pub(crate) struct PeripheralManager<T: SplitReader + SplitWriter> {
    /// Receiver
    pub(crate) transceiver: T,
    /// Peripheral id
    pub(crate) id: usize,
    /// This peripheral's matrix size and placement in the central's keymap
    matrix_config: PeripheralMatrixConfig,
    #[cfg(feature = "dfu_split")]
    pub(crate) passthrough_crc: crate::crc32::Crc32,
    /// Whether to skip hash comparison and always flash firmware.
    #[cfg(feature = "dfu_split")]
    pub(crate) policy: crate::split::dfu::UpdatePolicy,
    /// Set after a chunk fails all retries — aborts subsequent writes.
    #[cfg(feature = "dfu_split")]
    pub(crate) dfu_aborted: bool,
}

impl<T: SplitReader + SplitWriter> PeripheralManager<T> {
    pub(crate) fn new(
        transceiver: T,
        id: usize,
        matrix_config: PeripheralMatrixConfig,
        #[cfg(feature = "dfu_split")] policy: crate::split::dfu::UpdatePolicy,
    ) -> Self {
        Self {
            transceiver,
            matrix_config,
            id,
            #[cfg(feature = "dfu_split")]
            passthrough_crc: crate::crc32::Crc32::new(),
            #[cfg(feature = "dfu_split")]
            policy,
            #[cfg(feature = "dfu_split")]
            dfu_aborted: false,
        }
    }

    /// Send a message to the peripheral, returning Err on disconnect.
    pub(crate) async fn send(&mut self, msg: &SplitMessage) -> Result<(), ()> {
        debug!("Sending message to peripheral {}: {:?}", self.id, msg);
        match self.transceiver.write(msg).await {
            Ok(_) => Ok(()),
            Err(SplitDriverError::Disconnected) => Err(()),
            Err(e) => {
                error!("SplitDriver write error: {:?}", e);
                Err(())
            }
        }
    }

    /// Run the manager.
    ///
    /// The manager receives from the peripheral and publishes input events.
    /// It also syncs the central's `ConnectionStatus` to the peripheral on every
    /// change as an informational signal
    pub(crate) async fn run(mut self) {
        use crate::event::EventSubscriber;

        let mut indicator_sub = crate::event::LedIndicatorEvent::subscriber();
        let mut layer_sub = crate::event::LayerChangeEvent::subscriber();
        // Subscribe before the initial send so any change racing past the
        // snapshot is still delivered to us.
        let mut connection_sub = crate::event::ConnectionStatusChangeEvent::subscriber();
        #[cfg(feature = "_ble")]
        let mut clear_peer_sub = crate::event::ClearPeerEvent::subscriber();
        #[cfg(feature = "display")]
        let mut wpm_sub = crate::event::WpmUpdateEvent::subscriber();
        #[cfg(feature = "display")]
        let mut modifier_sub = crate::event::ModifierEvent::subscriber();
        let mut sleep_sub = crate::event::SleepStateEvent::subscriber();
        #[cfg(feature = "dfu_split")]
        let mut dfu_sub = DfuCmdEvent::subscriber();

        // Send the current state when the session starts.
        for message in [
            SplitMessage::ConnectionStatus(crate::state::current_connection_status()),
            SplitMessage::KeyboardIndicator(crate::keyboard::current_led_indicator().into_bits()),
            SplitMessage::SleepState(crate::state::current_sleep_state()),
        ] {
            if self.send(&message).await.is_err() {
                return;
            }
        }

        #[cfg(feature = "dfu_split")]
        self.check_firmware_update().await;

        loop {
            // Use select_biased_with_feature to handle feature-gated subscriber arms
            let next_event_to_peri = async {
                crate::select_biased_with_feature! {
                    e = indicator_sub.next_event().fuse() => SplitMessage::KeyboardIndicator(e.0.into_bits()),
                    e = layer_sub.next_event().fuse() => SplitMessage::Layer(e.0),
                    e = connection_sub.next_event().fuse() => SplitMessage::ConnectionStatus(e.0),
                    with_feature("_ble"): _ = clear_peer_sub.next_event().fuse() => {
                        #[cfg(feature = "storage")]
                        {
                            use {crate::split::ble::PeerAddress, crate::storage::{StorageItem, store_unchecked}};
                            store_unchecked(StorageItem::PeerAddress(PeerAddress::new(self.id as u8, false, [0; 6]))).await;
                        }
                        SplitMessage::ClearPeer
                    },
                    e = sleep_sub.next_event().fuse() => SplitMessage::SleepState(e.0),
                    with_feature("display"): e = wpm_sub.next_event().fuse() => SplitMessage::Wpm(e.0),
                    with_feature("display"): e = modifier_sub.next_event().fuse() => SplitMessage::Modifier(e.modifier.into_bits()),
                }
            };

            #[cfg(feature = "dfu_split")]
            let event_or_signal = select(next_event_to_peri, dfu_sub.next_message_pure()).fuse();
            #[cfg(not(feature = "dfu_split"))]
            let event_or_signal = next_event_to_peri;

            match select(self.transceiver.read(), event_or_signal).await {
                Either::First(read_result) => match read_result {
                    #[cfg(feature = "dfu_split")]
                    Ok(SplitMessage::FirmwareHashResponse(hash)) => {
                        self.handle_proactive_hash(hash).await;
                    }
                    Ok(split_message) => self.process_peripheral_message(split_message).await,
                    Err(e) => error!("Peripheral message read error: {:?}", e),
                },
                #[cfg(feature = "dfu_split")]
                Either::Second(Either::First(msg)) => {
                    if self.send(&msg).await.is_err() {
                        return;
                    }
                }
                #[cfg(feature = "dfu_split")]
                Either::Second(Either::Second(cmd_event)) => {
                    self.handle_dfu_event(cmd_event).await;
                }
                #[cfg(not(feature = "dfu_split"))]
                Either::Second(msg) => {
                    if self.send(&msg).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// Process a single message from the peripheral.
    async fn process_peripheral_message(&self, split_message: SplitMessage) {
        trace!("Got message from peripheral: {:?}", split_message);
        match split_message {
            SplitMessage::Key(e) => match e.source {
                KeyboardEventSource::Key(key_pos) => {
                    // Verify the row/col
                    if key_pos.row >= self.matrix_config.rows || key_pos.col >= self.matrix_config.cols {
                        error!("Invalid peripheral row/col: {} {}", key_pos.row, key_pos.col);
                        return;
                    }
                    #[cfg(feature = "_ble")]
                    if e.pressed
                        && !PRESSED_KEYS.lock(|keys| {
                            keys.borrow()[self.id].can_press(
                                key_pos.row + self.matrix_config.row_offset,
                                key_pos.col + self.matrix_config.col_offset,
                            )
                        })
                    {
                        error!("Peripheral pressed-key tracking full");
                        return;
                    }
                    publish_event_async(KeyboardEvent::key(
                        key_pos.row + self.matrix_config.row_offset,
                        key_pos.col + self.matrix_config.col_offset,
                        e.pressed,
                    ))
                    .await;
                    #[cfg(feature = "_ble")]
                    {
                        // Commit after delivery: cancellation must not invent a press or lose a release.
                        PRESSED_KEYS.lock(|keys| {
                            keys.borrow_mut()[self.id].update(
                                key_pos.row + self.matrix_config.row_offset,
                                key_pos.col + self.matrix_config.col_offset,
                                e.pressed,
                            );
                        });
                    }
                }
                _ => publish_event_async(e).await,
            },
            // Non-key events are drop-on-full to keep the split read loop responsive.
            SplitMessage::Pointing(e) => publish_event(e),
            #[cfg(feature = "_ble")]
            SplitMessage::BatteryStatus(state) => set_peripheral_battery(self.id, state.0),
            #[cfg(feature = "dfu_split")]
            SplitMessage::FirmwareHashResponse(hash) => {
                info!("dfu_split: stale hash response ({:#x}) in event loop", hash);
            }
            #[cfg(feature = "dfu_split")]
            SplitMessage::FirmwareChunkAck { offset, crc: _ } => {
                info!("dfu_split: stale chunk ack (offset {}) in event loop, ignoring", offset);
            }
            #[cfg(feature = "dfu_split")]
            SplitMessage::FirmwareUpdateConfirm => {
                info!("dfu_split: stale update confirm in event loop, ignoring");
            }
            _ => warn!("{:?} should not come from peripheral", split_message),
        }
    }
}
