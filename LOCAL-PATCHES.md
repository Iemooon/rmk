Base: rmk-rs/rmk f12257e89e9d07c461f4c774626b04af5b10600b

RMK fork branch for NMK receivers; baseline history and storage schema identity retained.
Local patch: optional asynchronous bootloader preparation handshake for DFU marker cleanup.
No RF, matrix, Vial wire format or storage layout changes.

STORAGE-BASE-COMMIT intentionally retains the original schema identity because this patch changes no serialized fields. Advance the schema identity if stored data formats change.

Validation: 555 tests passed with split,vial,storage,async_matrix,_ble, including the new latched software-bootloader-request regression. Hardware behavior remains application/board specific.
