Base: rmk-rs/rmk 5ed4fd6fc9059107612004a3416b7d2569fbb045

NMK receiver branch synchronized with dynamic-layers and upstream main.
Local patches: dynamic layer APIs, BLE split disconnect/offline peer fixes, and optional asynchronous DFU preparation handshake.

STORAGE-BASE-COMMIT tracks the storage schema baseline independently of local implementation commits.
Advanced on 2026-10-27 because upstream replaced MacroData with MacroChunk storage and changed stored action types.
Firmware built from this update resets incompatible persisted settings and pairing data on first boot.
Advance the schema identity whenever stored data formats change.

Hardware behavior remains application/board specific; no device was flashed.
