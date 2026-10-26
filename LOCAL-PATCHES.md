Base: rmk-rs/rmk 5ed4fd6fc9059107612004a3416b7d2569fbb045

NMK receiver branch synchronized with dynamic-layers and upstream main.
Local patches: dynamic layer APIs, BLE split disconnect/offline peer fixes, and optional asynchronous DFU preparation handshake.

STORAGE-BASE-COMMIT tracks the storage schema baseline independently of local implementation commits.
Advanced on 2026-10-27 because upstream replaced MacroData with MacroChunk storage and changed stored action types.
Firmware built from this update resets incompatible persisted settings and pairing data on first boot.
Advance the schema identity whenever stored data formats change.

Hardware behavior remains application/board specific; no device was flashed.

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
