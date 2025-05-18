use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Result;
use esp_idf_hal::{
    delay::{self, FreeRtos},
    gpio::{InterruptType, PinDriver, Pull},
    peripherals::Peripherals,
    task::notification::Notification,
};

fn main() -> Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let mut btn = PinDriver::input(peripherals.pins.gpio15)?;
    btn.set_pull(Pull::Up)?;
    btn.set_interrupt_type(InterruptType::LowLevel)?;

    let notification = Notification::new();
    let notifier = notification.notifier();

    unsafe {
        btn.subscribe(move || {
            notifier.notify_and_yield(NonZeroU32::new(1).unwrap());
        })?;
    }

    let mut relay = PinDriver::output(peripherals.pins.gpio13)?;

    log::info!("Starting main loop");
    let is_low = AtomicBool::new(false);
    loop {
        if btn.is_low() && !is_low.load(Ordering::Relaxed) {
            relay.toggle()?;
        }
        log::info!("Button pressed {}", btn.is_low());
        is_low.store(btn.is_low(), Ordering::Relaxed);
        FreeRtos::delay_ms(10);
    }
}
