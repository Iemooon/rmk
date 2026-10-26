//! Optional nRF52833 power-cycle DFU window, migrated from Weekin's common worker.
//!
//! Construct one [`DfuWindow`] before USB starts, then run it alongside storage.
//! Do not also run an application-owned marker worker. Slot 240 is reserved.
//! This uses the storage queue for both BLE/MPSL and non-BLE applications.
mod policy;
use crate::storage::{read_user_data, store_user_data};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use policy::{ARMED, CLEAR, SLOT};

async fn write_and_confirm(value: &[u8]) -> bool {
    if store_user_data(SLOT, value).await.is_err() {
        return false;
    }
    // store_user_data only enqueues: the ordered read confirms persistence.
    read_user_data(SLOT).await.as_deref() == Some(value)
}

pub(crate) async fn clear_marker() -> bool {
    if !write_and_confirm(CLEAR).await {
        warn!("DFU marker clear failed; stay in application");
        return false;
    }
    if !policy::clear_confirmed(read_user_data(SLOT).await.as_deref()) {
        warn!("DFU marker verification failed; stay in application");
        return false;
    }
    true
}

async fn clear_and_jump() {
    if !clear_marker().await {
        return;
    }
    Timer::after_millis(200).await;
    // Bypass the request hook only here, after the marker is durably consumed.
    crate::boot::jump_to_bootloader_after_prepare();
}

async fn run_window() {
    let base = Instant::now();
    let arm_at = base + Duration::from_millis(policy::ARM_MS);
    let clear_at = base + Duration::from_millis(policy::CLEAR_MS);
    if policy::is_armed(read_user_data(SLOT).await.as_deref()) {
        clear_and_jump().await;
        return;
    }
    Timer::at(arm_at).await;
    if !policy::may_arm(base.elapsed().as_millis()) {
        return;
    }
    if !write_and_confirm(ARMED).await {
        return;
    }
    Timer::at(clear_at).await;
    if !write_and_confirm(CLEAR).await {
        warn!("DFU marker still armed; next restart may enter DFU");
    }
}

async fn run() {
    // A software request cancels the sole ARMED producer before clearing.
    // Already-enqueued writes finish before our CLEAR because storage is FIFO.
    // Storage discards replies of cancelled reads by request ID.
    if let Either::First(()) = select(run_window(), crate::boot::wait_for_bootloader_request()).await {
        crate::boot::wait_for_bootloader_request().await;
    }
    loop {
        clear_and_jump().await;
        // On failure remain in the app, with arming stopped. Allow an explicit retry.
        crate::boot::wait_for_bootloader_request().await;
    }
}

/// Adapter for the RMK peripheral processor registration.
pub struct DfuWindow {
    _registered: (),
}
impl DfuWindow {
    pub fn new() -> Self {
        crate::boot::install_bootloader_prepare();
        Self { _registered: () }
    }
}
impl crate::core_traits::Runnable for DfuWindow {
    async fn run(&mut self) -> ! {
        run().await;
        core::future::pending().await
    }
}
