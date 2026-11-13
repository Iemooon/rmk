Base: rmk-rs/rmk 708c9ee32f96ac345295819c0925aff2b6a80105

Maintained feature index (Chinese): [LEMON-NMK-FEATURES.md](LEMON-NMK-FEATURES.md).

2026-11-13: Merged upstream main `708c9ee32` (61 commits since `6c89c4ba1`).
This branch carries no nRF54 code of its own: the nRF54LM20/L15 features, watchdog
gating, the two `use_rust/nrf54*` examples and the CI skiplist entry all come from
upstream, and `git diff upstream/main..HEAD` contains no nRF54 lines. The earlier
nRF54 experiment was reverted in `a7723b2c7`; both nRF54 example directories are now
byte-identical to upstream.
Upstream moved the sleep manager from `rmk/src/ble/sleep.rs` to `rmk/src/sleep.rs` and
starts it from `Keyboard::run`, so the BLE task is back to `join3`; the local pinned-future
stack fix is kept, but its 14272-byte measurement predates this merge. The local runtime
sleep timeout, its `TIMEOUT_CHANGED` signal and the stored SleepTimeout still work; a zero
timeout keeps disabling idle expiry only, so explicit sleep requests still apply — that is
why upstream's two zero-timeout tests were adjusted instead of adopting its early
`pending()` return. Upstream also moved the embassy-usb HID classes to `class::hid::device`,
made `Render` async and added 21 keycodes; local call sites were updated accordingly.
`SleepTimeout` stays at the end of the storage enums, so STORAGE-BASE-COMMIT is retained.
Host `cargo check` passed for all 8 featuresets; `split,vial,storage,async_matrix` ran 629
nextest tests green; the nRF52840 BLE release firmware built and linked. BLE host tests on
Windows still fail at link time on the pre-existing embassy-crypto backend symbols. Nothing
was flashed and no hardware acceptance was performed; details in LEMON-NMK-FEATURES.md.

2026-11-07: Without BLE, Vial channel 0xF0 get/set requests now return 0xFF
instead of echoing a successful-looking response. Other channels are unchanged.
Corrected sleep protocol documentation and tests to use CustomGetValue (0x08)
and CustomSetValue (0x07); tests now reference the enum. All four Vial handler
tests passed on Windows with vial,storage and no BLE. No hardware validation.

2026-11-06: Vial channel 0xF0 adds runtime BLE idle timeout configuration
(0 disables idle expiry; 30–3600 seconds accepted). Requires Vial and BLE;
storage is optional. Explicit sleep requests still apply at zero. Changes while
asleep are consumed after activity wakes the manager. Failed storage writes
restore the previous value and report 0xFF. See README.md for the wire format.
SleepTimeout is appended to StorageKey/StorageValue, preserving existing tags.
STORAGE-BASE-COMMIT is retained; version, feature and capacity changes still affect
schema identity. Downgrade reading of the new item has not been verified.
ARM release library check passed on Rust 1.99.0; command, source state, log and
remaining test gaps are recorded in LEMON-NMK-FEATURES.md. No hardware acceptance.

2026-11-01: Merge 4317ebb53 integrated upstream main bba0f24bf. This updates the
upstream baseline independently of the retained storage baseline.

2026-11-01: Added independent opt-in `rmk-gazell` build dependency with unchanged
Nordic SDK 17.1.1 and nrfxlib 2.9.0 archives, provenance and original licenses.
The helper emits application link directives, preserving GZLL_DIR/GZLL_LIB.
No RMK feature, storage schema, DFU policy or runtime driver changes.
Host helper check and Ergo52 nRF52833 application link passed; no hardware test.

2026-11-01: Extracted opt-in Gazell v4 A/B switching, generic per-source battery
storage, EBAT/1 and independent battery USB HID modules under `rmk/src/receiver/`.
Keyboard handover APIs clear transient LT, layers, macros, reports and matrix state;
external User routing defaults off. RF/board configuration remains application-owned.
No storage schema change. See LEMON-NMK-RECEIVER.md for integration constraints.
Current verification: 619 no-BLE host tests passed and Ergo52 nRF52833 firmware
linked. BLE host tests did not execute because upstream embassy-crypto backend
symbols failed to link. Hardware switching/power acceptance remains outstanding.

NMK receiver branch synchronized with dynamic-layers and upstream main.
Local patches: dynamic layer APIs, BLE split disconnect/offline peer fixes, and optional asynchronous DFU preparation handshake.

STORAGE-BASE-COMMIT tracks the storage schema baseline independently of local implementation commits.
Advanced on 2026-10-27 because upstream replaced MacroData with MacroChunk storage and changed stored action types.
Firmware built from this update resets incompatible persisted settings and pairing data on first boot.
Advance the schema identity whenever stored data formats change.

Hardware behavior remains application/board specific; no device was flashed.

BLE keyboard task joining uses pinned references to avoid large temporary
future moves in `MaybeDone`. CCK MO probe build: largest frame 14272 bytes
(previously 17792, unchanged 16384-byte gate); Vial/BLE host tests: 636 passed.
This does not change the storage format or RF protocol.

## Unified branch policy

`lemon-nmk` is the common RMK source for NMK/Gazell applications and BLE
applications. The name does not select a radio transport. Each application's
Cargo features and board configuration select the transport and hardware.

The local `vendor/rmk`, `vendor/rmk-cck`, and `vendor/rmk-lily58` snapshots
were audited against commit `bd07c85354fc3911b03dc66c2f14740fc1fc6c62`.
Their shared runtime patches are already covered by this branch:

| Feature | Implementation |
| --- | --- |
| Trackball/mouse temporary layers | `rmk/src/keymap.rs`: `activate_layer_if_inactive`, `deactivate_layer_if_active` |
| Release a disconnected half's outstanding keys | `rmk/src/split/pressed.rs`, `rmk/src/split/driver.rs`; tracks delivered events and global coordinates |
| Clear an offline BLE peer and restart discovery | `rmk/src/split/ble/central.rs`, `rmk/src/keyboard.rs` |
| Prepare persistent state before software DFU | `rmk/src/boot.rs`: opt-in request/worker handshake |
| Stable storage identity across implementation commits | `STORAGE-BASE-COMMIT`, `rmk/build.rs` |

Dynamic layer ownership is independent of BLE/Gazell transport. It is useful
for pointing devices and need not be called by a keyboard without a pointer.
The newer split implementation also covers cancellation while delivering
press/release events; do not replace it with the older vendor patch.

The old snapshot's proposed `RMK_STORAGE_COMMIT` override is not carried over:
this branch has a newer storage format. Using an old identity would not make
that format compatible. Advance `STORAGE-BASE-COMMIT` when the format changes.

## Application responsibilities

The optional `nrf52833_dfu_window` feature contains the 15–50 second timer and
the software-entry cleanup worker, migrated from the firmware repository's
`common/dfu_policy.rs` and `common/dfu_window.rs`. It is off by default and
selects the nRF52833 HAL chip; selecting another HAL chip at the same time is
invalid. Board-specific flash layout remains the application's responsibility.
See [the integration guide](LEMON-NMK-DFU.md).

Gazell RF addresses/channels, pointing processors, wiring, keymaps, Vial
unlock policy, USB identities, and bootloader partitions remain application
configuration. Updating an RMK dependency does not migrate those settings.

Consumers should select `lemon-nmk` and commit `Cargo.lock`, or pin a tested
revision. Moving the dependency from an older snapshot to this branch needs
an application build and hardware acceptance; expect the storage reset noted
above. No consumer is migrated merely by consolidating this library branch.

## Validation (2026-10-27)

`cargo nextest run --manifest-path rmk/Cargo.toml --config-file
.config/nextest.toml --no-default-features --features <features>` passed:

| Features | Result |
| --- | --- |
| `split,vial,storage,async_matrix,_ble` | 628 passed |
| `rynk,_ble,split,async_matrix,storage` | 675 passed |
| `split,vial,storage,async_matrix` | 574 passed |

These are host regression tests. They do not establish target flash/RAM fit,
Gazell RF compatibility, BLE reconnect behavior, or physical DFU acceptance.

## Receiver and USB reliability migration (2026-10-27)

Migrated the TFT receiver project's USB pending-report/session invalidation
patch into `rmk/src/usb/mod.rs` and `rmk/src/state.rs`, preserving newer upstream
USB reset/pre-suspend handling. Added the production-writer regression harness
in `scripts/check_usb_wakeup.py`.

Added opt-in-by-use `rmk::receiver::SnapshotQueue` for decoded full-state
receivers, generalized from the TFT project's Gazell queue to configurable
source counts. No Gazell radio implementation, board mapping or RF parameters
are imported. Consumer projects have not been migrated by this library change.
See `LEMON-NMK-RECEIVER.md` for integration and overflow limitations.
No storage format change; the existing schema baseline is retained.

Validation for this migration: Vial/BLE 648, Vial without BLE 594, and
Rynk/BLE 695 host tests passed; the extracted production USB writer passed
7 simulations. Restoring the old writer fails 4 simulations; disabling queue
expiry/overflow recovery fails 5/2 queue tests. Release ARM library checks
passed for nRF52840 BLE and nRF52833 without BLE. These are not complete
firmware links or hardware acceptance. The writer test is wired into local
and CI test entry points; the full CI scripts were not run.

## Upstream sync (2026-10-30)

Merged upstream main `71556128fa72881fc35f0bf6c292d5e28ad9568d` into `lemon-nmk`. Preserved local dynamic layers, split disconnect/offline peer cleanup, pinned BLE futures, USB session protection, receiver queues, battery snapshots and opt-in nRF52833 DFU window. The split connector now uses upstream legacy/extended connection dispatch.

No storage record layout change was identified: upstream StenoKey enum retains the same one-byte chart-index encoding. STORAGE-BASE-COMMIT is retained. Dependency/features and BLE profile addressing changed upstream; application builds and pairing/reconnect acceptance remain required.

Validation for this merge is recorded in LEMON-NMK-FEATURES.md; older results above are historical. No firmware was flashed.

2026-11-01：opt-in `battery_voltage`、WBAT v2 与共享电压客户端，见 LEMON-NMK-RECEIVER.md。标准蓝牙电池服务仍用百分比；不接入 A/B 切换，不改变存储。
