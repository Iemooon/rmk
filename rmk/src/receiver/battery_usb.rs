//! Independent vendor HID, leaving Vial and keyboard traffic on their own endpoints.
use embassy_usb::{
    Builder,
    class::hid::{Config, HidBootProtocol, HidReaderWriter, HidSubclass, State},
    driver::Driver,
};
use static_cell::StaticCell;

/// Runnable wrapper for applications using `run_all!`.
pub struct BatteryUsb<D: Driver<'static>> {
    hid: Option<HidReaderWriter<'static, D, 32, 32>>,
    snapshot: fn() -> [Option<super::battery::Reading>; 2],
    status: fn() -> (u8, bool),
}
impl<D: Driver<'static>> BatteryUsb<D> {
    pub fn new(
        builder: &mut Builder<'static, D>,
        snapshot: fn() -> [Option<super::battery::Reading>; 2],
        status: fn() -> (u8, bool),
    ) -> Self {
        Self {
            hid: Some(build(builder)),
            snapshot,
            status,
        }
    }
    pub async fn run(&mut self) -> ! {
        run(
            self.hid.take().expect("battery USB can only run once"),
            self.snapshot,
            self.status,
        )
        .await
    }
}

// Usage page 0xFF62, usage 0x52; no Report ID; 32-byte IN and OUT reports.
const DESCRIPTOR: &[u8] = &[
    0x06, 0x62, 0xff, 0x09, 0x52, 0xa1, 0x01, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95, 0x20, 0x09, 0x01, 0x81,
    0x02, 0x09, 0x02, 0x91, 0x02, 0xc0,
];

pub fn build<D: Driver<'static>>(builder: &mut Builder<'static, D>) -> HidReaderWriter<'static, D, 32, 32> {
    static STATE: StaticCell<State<'static>> = StaticCell::new();
    HidReaderWriter::new(
        builder,
        STATE.init(State::new()),
        Config {
            report_descriptor: DESCRIPTOR,
            request_handler: None,
            poll_ms: 10,
            max_packet_size: 32,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        },
    )
}

pub async fn run<D: Driver<'static>>(
    hid: HidReaderWriter<'static, D, 32, 32>,
    snapshot: fn() -> [Option<super::battery::Reading>; 2],
    status: fn() -> (u8, bool),
) -> ! {
    let (mut reader, mut writer) = hid.split();
    loop {
        reader.ready().await;
        let mut request = [0; 32];
        match reader.read(&mut request).await {
            Ok(n) => {
                let (receiver, active) = status();
                if let Some(reply) = super::battery_protocol::reply(
                    &request[..n],
                    snapshot(),
                    embassy_time::Instant::now().as_millis(),
                    receiver,
                    active,
                ) {
                    // A host that stops reading cannot hold up the keyboard or radio tasks.
                    let _ = embassy_time::with_timeout(embassy_time::Duration::from_millis(100), writer.write(&reply))
                        .await;
                }
            }
            Err(_) => embassy_time::Timer::after_millis(10).await,
        }
    }
}
