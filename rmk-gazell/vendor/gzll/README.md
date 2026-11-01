# Original Gazell archives

`gzll_nrf52840_gcc.a` is copied without modification from nRF5 SDK 17.1.1,
`components/proprietary_rf/gzll/gcc/`. It is the same binary previously used
by Ergo52 and Planck Duos, including their existing nRF52833 profiles.

SHA-256: `50d3e01a47aaaf0298f840c8e87817f90d1cfca460fd7d6ddb0d2165d91ae59c`

The adjacent `license.txt` is the original Nordic 5-Clause notice. The library
is closed source and must only be used with Nordic Semiconductor ICs.
Keep its notice when redistributing and do not modify/reverse engineer it.

The application supplies `memcpy`, `memset`, `nrf_gzll_host_rx_data_ready`,
`nrf_gzll_device_tx_success`, `nrf_gzll_device_tx_failed` and `nrf_gzll_disabled`.
This archive owns RADIO, TIMER2 and SWI0_EGU0 handlers. The nRF52820 archive
has a different glue/IRQ contract; see [its provenance](nrfxlib-52820/README.md).

Migration changes distribution and linking paths only. No new RF, timing or
chip-compatibility claims follow from moving these bytes into this repository.
