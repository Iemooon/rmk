# Nordic Gazell build dependency

This independent crate supplies the existing, unmodified Nordic static archives.
It is opt-in by adding a build dependency; ordinary RMK USB/BLE applications do
not depend on it. It adds no RMK features, runtime state or storage records.

```toml
[build-dependencies]
rmk-gazell = { git = "https://github.com/Iemooon/rmk.git", branch = "lemon-nmk" }
```

In the application's `build.rs`, call `rmk_gazell::link("nrf52833")` once.
Supported chips are nrf52833, nrf52840 and nrf52820. The application still supplies
Gazell FFI, interrupts/glue, clocks and board-specific radio settings. This crate
does not supply a ready-to-run receiver or transmitter. nRF51 SDK 11 transmitter
dependencies remain in the transmitter project.

| Archive | Source | Selection |
| --- | --- | --- |
| `vendor/gzll/gzll_nrf52840_gcc.a` | nRF5 SDK 17.1.1 | existing nRF52840/nRF52833 receivers, hard-float |
| `vendor/gzll/nrfxlib-52820/libgzll.a` | sdk-nrfxlib v2.9.0 | nRF52820, soft-float, application glue required |

See the adjacent provenance documents and original licenses. These binaries
are Nordic-only and are not covered by RMK's MIT/Apache license. Preserve the
license notices when redistributing; do not modify or reverse engineer them.
The nRF52833 selection preserves the existing application's archive selection;
it does not assert a new Nordic compatibility certification.

`GZLL_DIR` overrides the archive directory (relative paths resolve against the
application manifest). `GZLL_LIB` overrides the filename. Missing files or
unsupported chip names fail the build. No SDK checkout or network download
is performed by the build helper itself.
