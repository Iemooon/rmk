# nRF52820 Gazell soft-float library

Unmodified Nordic `sdk-nrfxlib` v2.9.0 archive:
https://github.com/nrfconnect/sdk-nrfxlib/blob/v2.9.0/gzll/lib/nrf52820/soft-float/libgzll.a

License: accompanying `LICENSE` (Nordic 5-Clause).
Glue interface: `gzll/include/nrf_gzll_glue.h` at the same tag.
Integration reference: https://github.com/nrfconnect/sdk-nrf/blob/v2.9.0/subsys/gazell/gzll_glue.c

`src/gazell_glue.rs` implements the exported timer/IRQ/PPI/clock/delay contract.
Unlike the nRF5 SDK archive, handlers are `nrf_gzll_*_irq_handler` functions.
Requires `thumbv7em-none-eabi`. Copied without modification from the locally
verified Split89 reference at `D:/Projects/weekin-nmk/split89-nmk` (2026-10-14).
The reference project records a successful no-bootloader SWD flash; this Ergo52
image still requires its own hardware test.

Archive SHA-256: `ddec68abe86fac0f445a1bf577eae9edefce575d508bd3952eee0844240de972`.
The copied archive hash was compared to the local reference. All accompanying
Nordic licence terms apply; include LICENSE when distributing the binary.
