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

const SSID: &str = "iQOO Z7x 5G";
const PASSWORD: &str = "123456789";

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::debug!("Initializing peripherals");
    let peripherals = Peripherals::take().unwrap();

    log::debug!("Initializing button");
    let mut btn = PinDriver::input(peripherals.pins.gpio15)?;
    btn.set_pull(Pull::Up)?;
    btn.set_interrupt_type(InterruptType::LowLevel)?;

    log::debug!("Initializing notification for button");
    let notification = Notification::new();
    let notifier = notification.notifier();

    log::debug!("Subscribing to button");
    unsafe {
        btn.subscribe(move || {
            notifier.notify_and_yield(NonZeroU32::new(1).unwrap());
        })?;
    }

    log::debug!("Initializing relay");
    let mut relay = PinDriver::output(peripherals.pins.gpio13)?;

    log::debug!("Starting main loop");
    let is_low = AtomicBool::new(false);
    loop {
        if btn.is_low() && !is_low.load(Ordering::Relaxed) {
            relay.toggle()?;
        }
        is_low.store(btn.is_low(), Ordering::Relaxed);
        FreeRtos::delay_ms(10);
    }
}
