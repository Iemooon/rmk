//! Keyboard-wide sleep management.
//!
//! One manager owns the keyboard's sleep state: it watches [`SLEEP_INPUT`],
//! latches each decision in [`SLEEPING_STATE`] for pollers like the battery
//! service, and publishes it as [`SleepStateEvent`] for everything else — the
//! display, and on split centrals the per-link connection-parameter followers
//! in `split::ble::central`.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use embassy_futures::select::{Either, select};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};

use crate::SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS;
use crate::event::{SleepStateEvent, publish_event};

pub(crate) const SLEEP_TIMEOUT_MIN_SECONDS: u32 = 30;
pub(crate) const SLEEP_TIMEOUT_MAX_SECONDS: u32 = 3600;

/// The latched sleep state.
/// - `true`: the keyboard is idle and sleeping
/// - `false`: the keyboard is awake
pub(crate) static SLEEPING_STATE: AtomicBool = AtomicBool::new(false);

/// Input to [`run_sleep_manager`], same encoding as [`SLEEPING_STATE`]:
/// - `true`: sleep now, without waiting out the idle timeout
/// - `false`: activity — wake up, or restart the idle timeout
static SLEEP_INPUT: Signal<crate::RawMutex, bool> = Signal::new();
static TIMEOUT_CHANGED: Signal<crate::RawMutex, ()> = Signal::new();
static SLEEP_TIMEOUT_SECONDS: AtomicU32 = AtomicU32::new(SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS);

pub(crate) fn sleep_timeout_seconds() -> u32 {
    SLEEP_TIMEOUT_SECONDS.load(Ordering::Acquire)
}

pub(crate) fn set_sleep_timeout_seconds(seconds: u32) -> bool {
    if seconds != 0 && !(SLEEP_TIMEOUT_MIN_SECONDS..=SLEEP_TIMEOUT_MAX_SECONDS).contains(&seconds) {
        return false;
    }
    SLEEP_TIMEOUT_SECONDS.store(seconds, Ordering::Release);
    TIMEOUT_CHANGED.signal(());
    true
}

/// Report keyboard activity: wake the keyboard up, or restart the idle timeout
/// when it's already awake.
pub fn report_activity() {
    SLEEP_INPUT.signal(false);
}

/// Ask the keyboard to sleep now instead of waiting out the idle timeout. Sent
/// when the host suspends us or when advertising times out.
pub fn request_sleep() {
    SLEEP_INPUT.signal(true);
}

/// The keyboard's one sleep manager.
///
/// Started by `Keyboard::run` on a unibody or split central. Split peripherals
/// follow the central's sleep events instead.
///
/// When `split_central_sleep_timeout_seconds` is 0 (the default), both automatic
/// sleep and explicit sleep requests are disabled.
pub(crate) async fn run_sleep_manager() {
    // Only stored when Vial can configure it, which needs BLE.
    #[cfg(all(feature = "storage", feature = "_ble"))]
    if let Ok(Some(crate::storage::StorageValue::SleepTimeout(seconds))) =
        crate::storage::read(crate::storage::StorageKey::SleepTimeout).await
    {
        if set_sleep_timeout_seconds(seconds) {
            info!("Loaded sleep timeout: {}s", seconds);
        }
    }

    info!("Sleep manager started with {}s timeout", sleep_timeout_seconds());
    manage_sleep_state(Duration::from_secs(sleep_timeout_seconds().into())).await
}

/// The sleep state machine, separate from [`run_sleep_manager`] only so tests
/// can drive it with a short timeout (the configured one is 0 in test builds).
async fn manage_sleep_state(idle_timeout: Duration) -> ! {
    let mut idle_timeout = idle_timeout;
    let mut timeout_disabled = idle_timeout == Duration::from_secs(0);
    loop {
        // Awake: sleep once the keyboard has been idle for `idle_timeout`, or as
        // soon as something asks us to. The input is polled first so activity
        // racing the timeout wins the tie instead of causing a spurious sleep.
        loop {
            if timeout_disabled {
                match select(SLEEP_INPUT.wait(), TIMEOUT_CHANGED.wait()).await {
                    Either::First(true) => break,
                    Either::First(false) => debug!("Activity detected while sleep is disabled"),
                    Either::Second(_) => {
                        let seconds = sleep_timeout_seconds();
                        idle_timeout = Duration::from_secs(seconds.into());
                        timeout_disabled = seconds == 0;
                        if !timeout_disabled {
                            continue;
                        }
                    }
                }
            } else {
                match select(
                    SLEEP_INPUT.wait(),
                    select(TIMEOUT_CHANGED.wait(), Timer::after(idle_timeout)),
                )
                .await
                {
                    Either::First(true) | Either::Second(Either::Second(_)) => break,
                    Either::First(false) => debug!("Activity detected, resetting sleep timeout"),
                    Either::Second(Either::First(_)) => {
                        let seconds = sleep_timeout_seconds();
                        idle_timeout = Duration::from_secs(seconds.into());
                        timeout_disabled = seconds == 0;
                    }
                }
            }
        }
        info!("Entering sleep mode");
        SLEEPING_STATE.store(true, Ordering::Release);
        publish_event(SleepStateEvent::new(true));

        // Asleep: only activity wakes us; further sleep requests change nothing.
        while SLEEP_INPUT.wait().await {}

        info!("Waking up from sleep mode due to activity");
        SLEEPING_STATE.store(false, Ordering::Release);
        publish_event(SleepStateEvent::new(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_block_on as block_on;

    /// Run the state machine with a 1s idle timeout until `script` finishes.
    fn with_sleep_manager(script: impl core::future::Future<Output = ()>) {
        block_on(async {
            select(manage_sleep_state(Duration::from_secs(1)), script).await;
        });
    }

    fn sleeping() -> bool {
        crate::state::current_sleep_state()
    }

    #[test]
    fn keyboard_runs_configured_sleep_manager() {
        use rmk_types::action::KeyAction;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::core_traits::Runnable;
        use crate::keyboard::Keyboard;
        use crate::keymap::{KeyMap, KeymapData};

        block_on(async {
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[KeyAction::No]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut keyboard = Keyboard::new(&keymap);

            select(keyboard.run(), async {
                Timer::after_secs(u64::from(SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS) + 1).await;
                assert_eq!(sleeping(), SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS != 0);
                report_activity();
                Timer::after_millis(1).await;
                assert!(!sleeping(), "the keyboard's sleep manager must process activity");
            })
            .await;
        });
    }

    /// A zero timeout disables idle expiry only: an explicit request, such as the
    /// host suspending us or advertising timing out, still sleeps the keyboard.
    #[test]
    fn zero_timeout_keeps_explicit_sleep_requests() {
        assert_eq!(SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS, 0);
        block_on(async {
            select(run_sleep_manager(), async {
                Timer::after_secs(2).await;
                assert!(!sleeping(), "a zero timeout must not expire on its own");

                request_sleep();
                Timer::after_millis(1).await;
                assert!(sleeping(), "an explicit request still sleeps");

                report_activity();
                Timer::after_millis(1).await;
                assert!(!sleeping(), "activity wakes the keyboard");
            })
            .await;
        });
    }

    #[test]
    fn sleeps_when_idle_and_wakes_on_activity() {
        use crate::event::{EventSubscriber, SubscribableEvent};

        with_sleep_manager(async {
            let mut events = SleepStateEvent::subscriber();
            Timer::after_millis(900).await;
            assert!(!sleeping(), "still inside the idle timeout");

            Timer::after_millis(200).await;
            assert!(sleeping(), "idle timeout elapsed");
            assert_eq!(events.next_event().await, SleepStateEvent::new(true));

            report_activity();
            Timer::after_millis(10).await;
            assert!(!sleeping(), "activity wakes the keyboard");
            assert_eq!(events.next_event().await, SleepStateEvent::new(false));
        });
    }

    #[test]
    fn keyboard_activity_restarts_timeout_and_wakes_from_sleep() {
        use rmk_types::action::KeyAction;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::core_traits::Runnable;
        use crate::event::KeyboardEvent;
        use crate::keyboard::Keyboard;
        use crate::keymap::{KeyMap, KeymapData};

        block_on(async {
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[KeyAction::No]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut keyboard = Keyboard::new(&keymap);

            let script = async {
                for pressed in [true, false, true, false] {
                    Timer::after_millis(600).await;
                    assert!(!sleeping(), "key activity must restart the idle timeout");
                    publish_event(KeyboardEvent::key(0, 0, pressed));
                    Timer::after_millis(1).await;
                }
                Timer::after_millis(1100).await;
                assert!(sleeping());
                publish_event(KeyboardEvent::key(0, 0, true));
                Timer::after_millis(1).await;
                assert!(!sleeping(), "key activity must wake the keyboard");
            };

            // The manager's runtime floor is 30s, and only one manager may run
            // (a second one would take the activity signals), so set the atomic
            // directly and let `Keyboard::run` start the manager the real way.
            SLEEP_TIMEOUT_SECONDS.store(1, Ordering::Release);
            select(keyboard.run(), script).await;
        });
    }

    /// Ball motion is activity too. A trackball can go minutes without a key
    /// event, and treating only `KeyboardEvent` as activity put a board that is
    /// being actively moused to sleep mid-use.
    #[test]
    fn pointing_activity_restarts_the_idle_timeout() {
        use embassy_futures::select::select3;
        use rmk_types::action::KeyAction;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::core_traits::Runnable;
        use crate::event::{Axis, AxisEvent, AxisValType, PointingEvent, publish_event};
        use crate::input_device::pointing::{PointingProcessor, PointingProcessorConfig};
        use crate::keymap::{KeyMap, KeymapData};

        let roll = || PointingEvent {
            device_id: 0,
            axes: [
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::X,
                    value: 4,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Y,
                    value: -3,
                },
                AxisEvent {
                    typ: AxisValType::Rel,
                    axis: Axis::Z,
                    value: 0,
                },
            ],
        };

        block_on(async {
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[KeyAction::No]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut processor = PointingProcessor::new(&keymap, PointingProcessorConfig::default());

            let script = async {
                // Same shape as `activity_restarts_the_idle_timeout`, with the
                // ball as the only input: 1.8s of steady rolling, no key ever.
                for _ in 0..3 {
                    Timer::after_millis(600).await;
                    assert!(!sleeping(), "ball motion must restart the idle timeout");
                    publish_event(roll());
                    // Let the processor pick the event up before the next check.
                    Timer::after_millis(1).await;
                }
            };

            select3(manage_sleep_state(Duration::from_secs(1)), processor.run(), script).await;
        });
    }

    #[test]
    fn joystick_motion_keeps_awake_but_centered_samples_allow_sleep() {
        use embassy_futures::select::select3;
        use rmk_types::action::KeyAction;

        use crate::config::{BehaviorConfig, PositionalConfig};
        use crate::core_traits::Runnable;
        use crate::event::{Axis, AxisEvent, AxisValType, PointingEvent};
        use crate::input_device::joystick::JoystickProcessor;
        use crate::keymap::{KeyMap, KeymapData};

        block_on(async {
            let mut behavior = BehaviorConfig::default();
            let positional: PositionalConfig<1, 1> = PositionalConfig::default();
            let mut data: KeymapData<1, 1, 1, 0> = KeymapData::new([[[KeyAction::No]]]);
            let keymap = KeyMap::new(&mut data, &mut behavior, &positional).await;
            let mut processor = JoystickProcessor::new(7, [[1, 0], [0, 1]], [-100, 0], 10, &keymap);
            let sample = |device_id, x| PointingEvent {
                device_id,
                axes: [Axis::X, Axis::Y, Axis::Z].map(|axis| AxisEvent {
                    typ: AxisValType::Abs,
                    axis,
                    value: if matches!(axis, Axis::X) { x } else { 0 },
                }),
            };
            let script = async {
                for _ in 0..3 {
                    Timer::after_millis(600).await;
                    assert!(!sleeping(), "joystick motion must restart the idle timeout");
                    publish_event(sample(7, 200));
                    Timer::after_millis(1).await;
                }
                for _ in 0..3 {
                    Timer::after_millis(400).await;
                    // Bias and resolution turn this sample into zero output.
                    publish_event(sample(7, 105));
                    Timer::after_millis(1).await;
                }
                assert!(sleeping(), "centered samples must allow idle sleep");
                publish_event(sample(8, 200));
                Timer::after_millis(1).await;
                assert!(sleeping(), "another device must not wake this joystick");
                publish_event(sample(7, 200));
                Timer::after_millis(1).await;
                assert!(!sleeping(), "joystick motion must wake the keyboard");
            };
            select3(manage_sleep_state(Duration::from_secs(1)), processor.run(), script).await;
        });
    }

    #[test]
    fn sleep_request_skips_the_idle_timeout() {
        with_sleep_manager(async {
            request_sleep();
            Timer::after_millis(10).await;
            assert!(sleeping(), "a sleep request doesn't wait for the timeout");

            request_sleep();
            Timer::after_millis(10).await;
            assert!(sleeping(), "a second request while asleep changes nothing");

            report_activity();
            Timer::after_millis(10).await;
            assert!(!sleeping());
        });
    }
}
