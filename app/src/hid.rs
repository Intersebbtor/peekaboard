//! Live layer from the keyboard: Raw HID packets of the KeyPeek firmware
//! modules for ZMK and QMK (usage page 0xFF60, usage 0x61), over USB or
//! Bluetooth. The vendor usage page means no special permission is needed.
//!
//! `--simulate` (or PEEKABOARD_SIMULATE=1) fakes a keyboard instead, to try the app
//! without hardware. It works with ZMK and QMK keymaps alike.

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use keymap_core::live::{QMK_SUBSCRIBE, QMK_UNSUBSCRIBE, USAGE, USAGE_PAGE};
use keymap_core::LayerPacket;

use crate::hub::Hub;

/// `--hid-check`: lists every HID device the system shows, then listens to the
/// live-layer device for ten seconds. Prints to stdout, for finding out why a
/// keyboard is not picked up (on Windows: pipe it, e.g. `| Out-String`).
/// `qmk`: keep the QMK subscription alive while listening (`--hid-check --qmk`).
pub fn check(qmk: bool) {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            println!("HID is not available: {e}");
            return;
        }
    };
    println!("HID devices (looking for usage page {USAGE_PAGE:#06x}, usage {USAGE:#04x}):");
    for d in api.device_list() {
        let marker = if d.usage_page() == USAGE_PAGE && d.usage() == USAGE {
            "  <-- live layer"
        } else {
            ""
        };
        println!(
            "  {:04x}:{:04x}  page {:#06x} usage {:#04x}  interface {:>2}  {:?} / {:?}{marker}",
            d.vendor_id(),
            d.product_id(),
            d.usage_page(),
            d.usage(),
            d.interface_number(),
            d.manufacturer_string().unwrap_or(""),
            d.product_string().unwrap_or(""),
        );
    }
    // Over USB and Bluetooth at once the keyboard shows up twice: listen to all
    let mut devices = Vec::new();
    for info in api
        .device_list()
        .filter(|d| d.usage_page() == USAGE_PAGE && d.usage() == USAGE)
    {
        let via = if info.interface_number() >= 0 {
            "USB"
        } else {
            "Bluetooth"
        };
        match info.open_device(&api) {
            Ok(device) => {
                println!("Opened the {via} one: {:?}", info.path());
                devices.push((via, device));
            }
            Err(e) => println!("Could not open the {via} one ({:?}): {e}", info.path()),
        }
    }
    if devices.is_empty() {
        println!("No live-layer device to listen to.");
        return;
    }
    println!("Listening for 10 s, press keys and change layers:");
    let start = std::time::Instant::now();
    let mut buffer = [0u8; 64];
    let mut packets = 0;
    let mut subscription = QmkSubscription::default();
    let handles: Vec<&HidDevice> = devices.iter().map(|(_, d)| d).collect();
    while start.elapsed() < Duration::from_secs(10) {
        subscription.keep_alive(&handles, qmk);
        for (via, device) in &devices {
            match device.read_timeout(&mut buffer, 50) {
                Ok(0) => {}
                Ok(length) => {
                    packets += 1;
                    let decoded = LayerPacket::decode(&buffer[..length]);
                    println!(
                        "  {via}: {length} bytes {:02x?} -> {decoded:?}",
                        &buffer[..length.min(8)]
                    );
                }
                Err(e) => println!("  {via}: read failed: {e}"),
            }
        }
    }
    println!("{packets} packets in 10 s.");
}

pub fn spawn(hub: Hub, simulate: bool) {
    std::thread::spawn(move || {
        if simulate {
            run_simulator(&hub);
        } else {
            run_hid(&hub);
        }
    });
}

fn run_hid(hub: &Hub) {
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("[LiveLayer] HID is not available: {e}");
            return;
        }
    };
    eprintln!("[LiveLayer] Monitoring started");

    loop {
        if let Err(e) = api.refresh_devices() {
            eprintln!("[LiveLayer] Could not list devices: {e}");
        }

        // With a cable in while Bluetooth is connected, the keyboard shows up
        // twice and talks over only one of them: listen to every match
        let mut name = None;
        let mut devices = Vec::new();
        for info in api
            .device_list()
            .filter(|d| d.usage_page() == USAGE_PAGE && d.usage() == USAGE)
        {
            let product = info.product_string().unwrap_or("Keyboard").to_string();
            match info.open_device(&api) {
                Ok(device) => {
                    name.get_or_insert(product);
                    devices.push(device);
                }
                Err(e) => eprintln!("[LiveLayer] Could not open {product}: {e}"),
            }
        }

        if let Some(name) = name {
            eprintln!(
                "[LiveLayer] Connected: {name} ({} interface(s))",
                devices.len()
            );
            hub.keyboard_connected(name);

            let mut buffer = [0u8; 64];
            let timeout = (250 / devices.len()).max(20) as i32;
            let mut subscription = QmkSubscription::default();
            'read: loop {
                subscription.keep_alive(&devices, hub.lock().is_qmk());
                for device in &devices {
                    match device.read_timeout(&mut buffer, timeout) {
                        Ok(0) => {}
                        Ok(length) => {
                            if let Some(packet) = LayerPacket::decode(&buffer[..length]) {
                                hub.packet(packet);
                            }
                        }
                        Err(e) => {
                            // One of them gone (cable out, Bluetooth off):
                            // look again, the other may still be there
                            eprintln!("[LiveLayer] Read failed: {e}");
                            break 'read;
                        }
                    }
                }
            }

            eprintln!("[LiveLayer] Disconnected");
            hub.keyboard_disconnected();
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

/// QMK's keypeek module only sends while the host renews a subscription about
/// once a second. ZMK sends without one, so nothing is written to a ZMK keyboard.
#[derive(Default)]
struct QmkSubscription {
    last_sent: Option<Instant>,
}

impl QmkSubscription {
    fn keep_alive<D: std::borrow::Borrow<HidDevice>>(&mut self, devices: &[D], is_qmk: bool) {
        match (is_qmk, self.last_sent) {
            (true, Some(sent)) if sent.elapsed() < Duration::from_millis(900) => {}
            (true, _) => {
                Self::send(devices, QMK_SUBSCRIBE);
                self.last_sent = Some(Instant::now());
            }
            // A ZMK keymap was loaded after a QMK one
            (false, Some(_)) => {
                Self::send(devices, QMK_UNSUBSCRIBE);
                self.last_sent = None;
            }
            (false, None) => {}
        }
    }

    fn send<D: std::borrow::Borrow<HidDevice>>(devices: &[D], message: [u8; 2]) {
        // Report ID 0, then the 32-byte raw HID report
        let mut report = [0u8; 33];
        report[1..3].copy_from_slice(&message);
        for device in devices {
            if let Err(e) = device.borrow().write(&report) {
                eprintln!("[LiveLayer] Could not send the QMK subscription: {e}");
            }
        }
    }
}

fn run_simulator(hub: &Hub) {
    eprintln!("[LiveLayer] Simulating a keyboard");
    std::thread::sleep(Duration::from_millis(800));
    hub.keyboard_connected("Simulated keyboard".to_string());

    let mut tick: u32 = 0;
    let mut held: Option<usize> = None;
    loop {
        std::thread::sleep(Duration::from_millis(220));
        tick = tick.wrapping_add(1);

        let (layer_count, key_count) = {
            let s = hub.lock();
            let km = s.keymap.as_ref();
            (
                km.map_or(1, |k| k.layers.len()),
                km.and_then(|k| k.layers.first())
                    .map_or(34, |l| l.bindings.len()),
            )
        };

        // Bursts of ~2 s typing, then ~6 s pause (so the HUD fades in between),
        // a new layer with every burst
        let typing = tick % 36 < 9;
        if tick.is_multiple_of(36) {
            let layer = (tick / 36) as usize % layer_count.max(1);
            hub.packet(LayerPacket::LayerState {
                default_layers: 1,
                active_layers: 1 | (1u32 << layer),
            });
        }

        // Press and release keys
        match held.take() {
            Some(position) => hub.packet(LayerPacket::KeyEvent {
                position,
                pressed: false,
            }),
            None if typing => {
                let position = (tick.wrapping_mul(2654435761) >> 7) as usize % key_count.max(1);
                held = Some(position);
                hub.packet(LayerPacket::KeyEvent {
                    position,
                    pressed: true,
                });
            }
            None => {}
        }
    }
}
