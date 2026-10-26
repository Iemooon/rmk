// Applications with persistent DFU markers opt in before USB starts.
#[cfg(feature = "storage")]
static PREPARE_BOOTLOADER: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "storage")]
static BOOTLOADER_REQUEST: embassy_sync::signal::Signal<crate::RawMutex, ()> = embassy_sync::signal::Signal::new();

/// Route software bootloader requests to the application's storage-aware worker.
#[cfg(feature = "storage")]
pub fn install_bootloader_prepare() {
    PREPARE_BOOTLOADER.store(true, core::sync::atomic::Ordering::Release);
}

/// Wait for a software request. Requests remain latched until the worker starts.
#[cfg(feature = "storage")]
pub async fn wait_for_bootloader_request() {
    BOOTLOADER_REQUEST.wait().await;
}

/// Request bootloader entry; opted-in applications prepare persistent state first.
pub fn jump_to_bootloader() {
    #[cfg(feature = "storage")]
    if PREPARE_BOOTLOADER.load(core::sync::atomic::Ordering::Acquire) {
        BOOTLOADER_REQUEST.signal(());
        return;
    }
    jump_to_bootloader_after_prepare();
}

/// Final hardware jump. The marker worker calls this only after verified cleanup.
/// Do not use this to bypass an installed preparation worker from host commands.
pub fn jump_to_bootloader_after_prepare() {
    #[cfg(feature = "adafruit_bl")]
    // Reference: https://github.com/adafruit/Adafruit_nRF52_Bootloader/blob/d6b28e66053eea467166f44875e3c7ec741cb471/src/main.c#L107
    embassy_nrf::pac::POWER
        .gpregret()
        .write_value(embassy_nrf::pac::power::regs::Gpregret(0x57));

    #[cfg(feature = "rp2040")]
    // Jump to RP2040 bootloader
    embassy_rp::rom_data::reset_to_usb_boot(0, 0);

    #[cfg(all(
        feature = "zsa_voyager_bl",
        target_arch = "arm",
        target_os = "none",
        any(target_abi = "eabi", target_abi = "eabihf")
    ))]
    unsafe {
        const GPIOA_MODER: *mut u32 = 0x4800_0000 as *mut u32;
        const GPIOA_ODR: *mut u32 = 0x4800_0014 as *mut u32;
        // PA8 + PA9: push-pull output, drive high.
        let m = core::ptr::read_volatile(GPIOA_MODER);
        core::ptr::write_volatile(GPIOA_MODER, (m & !(0b1111 << 16)) | (0b0101 << 16));
        let d = core::ptr::read_volatile(GPIOA_ODR);
        core::ptr::write_volatile(GPIOA_ODR, d | (1 << 8) | (1 << 9));
        // 500 ms at 72 MHz SYSCLK charges the RC network past the bootloader's threshold.
        cortex_m::asm::delay(36_000_000);
        // PA9 low before reset discharges the cap.
        let d = core::ptr::read_volatile(GPIOA_ODR);
        core::ptr::write_volatile(GPIOA_ODR, d & !(1 << 9));
    }

    #[cfg(not(any(feature = "adafruit_bl", feature = "rp2040", feature = "zsa_voyager_bl")))]
    warn!("Please specify a bootloader to jump to!");

    reboot_keyboard();
}

pub(crate) fn reboot_keyboard() {
    warn!("Rebooting keyboard!");
    // For cortex-m:
    #[cfg(all(
        target_arch = "arm",
        target_os = "none",
        any(target_abi = "eabi", target_abi = "eabihf")
    ))]
    cortex_m::peripheral::SCB::sys_reset();

    #[cfg(feature = "_esp_ble")]
    esp_hal::system::software_reset();
}


#[cfg(all(test, feature = "storage"))]
mod tests {
    #[test]
    fn software_bootloader_request_waits_for_application_preparation() {
        super::install_bootloader_prepare();
        super::jump_to_bootloader();
        // The request must stay latched until the application worker is ready.
        crate::test_support::test_block_on(super::wait_for_bootloader_request());
    }
}
