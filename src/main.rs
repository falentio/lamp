use anyhow::{Error, Result};
use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{AnyIOPin, InterruptType, PinDriver, Pull},
    io::Write,
    peripherals::Peripherals,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::EspHttpServer,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use url::Url;

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

    log::debug!("Initializing relays");
    let relays = {
        let mut relays = Vec::new();
        let pin: AnyIOPin = peripherals.pins.gpio13.into();
        relays.push(("lampu kamar", PinDriver::input_output(pin)?));
        let pin: AnyIOPin = peripherals.pins.gpio12.into();
        relays.push(("lampu keluarga", PinDriver::input_output(pin)?));
        let pin: AnyIOPin = peripherals.pins.gpio14.into();
        relays.push(("lampu ruang tamu", PinDriver::input_output(pin)?));
        let pin: AnyIOPin = peripherals.pins.gpio27.into();
        relays.push(("lampu dapur", PinDriver::input_output(pin)?));
        Arc::new(Mutex::new(relays))
    };

    log::debug!("Initializing server");
    let mut server = create_server()?;
    server.fn_handler::<Error, _>("/", esp_idf_svc::http::Method::Get, {
        let relays = relays.clone();

        move |req| {
            let relay_data = {
                let relay_guard = relays.lock().unwrap();
                let j = json!([
                    {
                        "name": "lampu kamar",
                        "isActive": relay_guard.get(0).unwrap().1.is_high(),
                        "id": 0,
                    },
                    {
                        "name": "lampu keluarga",
                        "isActive": relay_guard.get(1).unwrap().1.is_high(),
                        "id": 1,
                    },
                    {
                        "name": "lampu ruang tamu",
                        "isActive": relay_guard.get(2).unwrap().1.is_high(),
                        "id": 2,
                    },
                    {
                        "name": "lampu dapur",
                        "isActive": relay_guard.get(3).unwrap().1.is_high(),
                        "id": 3,
                    }
                ]);
                j.to_string()
            };
            let html = include_str!("../static/index.html").replace("$RELAYS", &relay_data);
            req.into_ok_response()?.write_all(html.as_bytes())?;
            Ok(())
        }
    })?;

    server.fn_handler::<Error, _>("/relay/toggle", esp_idf_svc::http::Method::Post, {
        let relays = relays.clone();
        move |req| {
            log::info!("Relay parse req uri: {}", req.uri());
            let u = Url::parse(format!("http:///{}", req.uri()).as_str())?;
            let relay_id = {
                let mut r = 0_u8;
                if let Some((_, relay_id_str)) = u.query_pairs().find(|(k, _v)| k == "relayId") {
                    r = relay_id_str.parse::<u8>()?;
                }
                r
            };
            if relay_id > 3 {
                req.into_response(400, Some("Relay ID is out of range"), &[])?;
                return Ok(());
            }
            log::info!("Relay parse req uri: {}", relay_id);
            let is_active = {
                let mut r = false;
                if let Some((_, is_active_str)) = u.query_pairs().find(|(k, _v)| k == "isActive") {
                    r = is_active_str.parse::<bool>()?;
                }
                r
            };
            log::info!("Relay parse req uri: {}", is_active);

            if let Ok(mut relay_guard) = relays.lock() {
                log::info!("Relay toggled via web");

                if is_active {
                    relay_guard
                        .get_mut(relay_id as usize)
                        .unwrap()
                        .1
                        .set_high()?;
                } else {
                    relay_guard
                        .get_mut(relay_id as usize)
                        .unwrap()
                        .1
                        .set_low()?;
                }
            }
            req.into_ok_response()?;
            Ok(())
        }
    })?;

    log::debug!("Initializing button");
    // TODO: make proper data structure for buttons
    let mut btn1 = PinDriver::input(peripherals.pins.gpio15)?;
    btn1.set_pull(Pull::Up)?;
    btn1.set_interrupt_type(InterruptType::LowLevel)?;
    let is_low1 = AtomicBool::new(false);

    let mut btn2 = PinDriver::input(peripherals.pins.gpio18)?;
    btn2.set_pull(Pull::Up)?;
    btn2.set_interrupt_type(InterruptType::LowLevel)?;
    let is_low2 = AtomicBool::new(false);

    let mut btn3 = PinDriver::input(peripherals.pins.gpio19)?;
    btn3.set_pull(Pull::Up)?;
    btn3.set_interrupt_type(InterruptType::LowLevel)?;
    let is_low3 = AtomicBool::new(false);

    let mut btn4 = PinDriver::input(peripherals.pins.gpio21)?;
    btn4.set_pull(Pull::Up)?;
    btn4.set_interrupt_type(InterruptType::LowLevel)?;
    let is_low4 = AtomicBool::new(false);

    log::info!("Starting main loop");
    loop {
        if btn1.is_low() && !is_low1.load(Ordering::Relaxed) {
            if let Ok(mut relay_guard) = relays.lock() {
                relay_guard.get_mut(0).unwrap().1.toggle()?;
                log::info!("Relay toggled via button");
            }
        }
        if btn2.is_low() && !is_low2.load(Ordering::Relaxed) {
            if let Ok(mut relay_guard) = relays.lock() {
                relay_guard.get_mut(1).unwrap().1.toggle()?;
                log::info!("Relay toggled via button");
            }
        }
        if btn3.is_low() && !is_low3.load(Ordering::Relaxed) {
            if let Ok(mut relay_guard) = relays.lock() {
                relay_guard.get_mut(2).unwrap().1.toggle()?;
                log::info!("Relay toggled via button");
            }
        }
        if btn4.is_low() && !is_low4.load(Ordering::Relaxed) {
            if let Ok(mut relay_guard) = relays.lock() {
                relay_guard.get_mut(3).unwrap().1.toggle()?;
                log::info!("Relay toggled via button");
            }
        }

        if !wifi.is_connected()? {
            let mut i = 1;
            loop {
                i += 1;
                if let Err(e) = connect_wifi(&mut wifi) {
                    log::error!("Failed to connect to wifi: {}", e);
                } else {
                    break;
                }
                let mut delay = 300 * i * i;
                if delay > 3000 {
                    delay = 3000;
                }
                FreeRtos::delay_ms(delay);
            }
        }
        is_low1.store(btn1.is_low(), Ordering::Relaxed);
        is_low2.store(btn2.is_low(), Ordering::Relaxed);
        is_low3.store(btn3.is_low(), Ordering::Relaxed);
        is_low4.store(btn4.is_low(), Ordering::Relaxed);
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
