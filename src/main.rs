use anyhow::{Error, Result};
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{InterruptType, PinDriver, Pull},
    io::Write,
    peripherals::Peripherals,
    task::notification::Notification,
    timer::Timer,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::EspHttpServer,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

const SSID: &str = "iQOO Z7x 5G";
const PASSWORD: &str = "123456789";

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::debug!("Initializing peripherals");
    let peripherals = Peripherals::take().unwrap();

    log::debug!("Initializing system event loop");
    let sys_loop = EspSystemEventLoop::take()?;

    log::debug!("Initializing NVS");
    let nvs = EspDefaultNvsPartition::take()?;

    log::debug!("Initializing WiFi");
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

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
    let relay = Arc::new(Mutex::new(PinDriver::input_output(
        peripherals.pins.gpio13,
    )?));

    log::debug!("Initializing server");
    let mut server = create_server()?;
    {
        let relay = relay.clone();
        server.fn_handler::<Error, _>("/toggle", esp_idf_svc::http::Method::Post, move |req| {
            if let Ok(mut relay_guard) = relay.lock() {
                relay_guard.toggle()?;
                let is_high = relay_guard.is_high();
                if is_high {
                    req.into_ok_response()?.write_all("ON".as_bytes())?;
                } else {
                    req.into_ok_response()?.write_all("OFF".as_bytes())?;
                }
            } else {
                log::error!("Failed to lock relay");
                return Err(Error::msg("Failed to lock relay"));
            }
            Ok(())
        })?;
    }
    log::info!("Starting main loop");
    let is_low = AtomicBool::new(false);
    loop {
        if btn.is_low() && !is_low.load(Ordering::Relaxed) {
            if let Ok(mut relay_guard) = relay.lock() {
                relay_guard.toggle()?;
                log::info!("Relay toggled via button");
            }
        }
        is_low.store(btn.is_low(), Ordering::Relaxed);
        // TODO: auto reconnect to wifi
        FreeRtos::delay_ms(10);
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> Result<()> {
    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: PASSWORD.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;

    wifi.start()?;
    log::debug!("Wifi started");

    wifi.connect()?;
    log::debug!("Wifi connected");

    wifi.wait_netif_up()?;
    log::debug!("Wifi netif up");

    Ok(())
}

fn create_server() -> Result<EspHttpServer<'static>> {
    let server_configuration = esp_idf_svc::http::server::Configuration {
        stack_size: 1028 * 5,
        http_port: 8080,
        ..Default::default()
    };

    Ok(EspHttpServer::new(&server_configuration)?)
}
