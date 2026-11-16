//! Live layer from the keyboard: Raw HID packets of the zmk-keypeek-layer-
//! notifier firmware module (usage page 0xFF60, usage 0x61), over USB or
//! Bluetooth. The vendor usage page means no special permission is needed.
//!
//! `--simulate` (or ZMK_SIMULATE=1) fakes a keyboard instead, to try the app
//! without hardware.

use std::time::Duration;

use hidapi::HidApi;
use zmk_core::live::{USAGE, USAGE_PAGE};
use zmk_core::LayerPacket;

use crate::hub::Hub;

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

        let candidate = api
            .device_list()
            .find(|d| d.usage_page() == USAGE_PAGE && d.usage() == USAGE)
            .map(|d| {
                let name = d.product_string().unwrap_or("Keyboard").to_string();
                (name, d.open_device(&api))
            });

        match candidate {
            Some((name, Ok(device))) => {
                eprintln!("[LiveLayer] Connected: {name}");
                hub.keyboard_connected(name);

                let mut buffer = [0u8; 64];
                loop {
                    match device.read_timeout(&mut buffer, 250) {
                        Ok(0) => {}
                        Ok(length) => {
                            if let Some(packet) = LayerPacket::decode(&buffer[..length]) {
                                hub.packet(packet);
                            }
                        }
                        Err(e) => {
                            eprintln!("[LiveLayer] Read failed: {e}");
                            break;
                        }
                    }
                }

                eprintln!("[LiveLayer] Disconnected");
                hub.keyboard_disconnected();
            }
            Some((name, Err(e))) => eprintln!("[LiveLayer] Could not open {name}: {e}"),
            None => {}
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

/// A keyboard that changes layers and types a little
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
                km.and_then(|k| k.layers.first()).map_or(34, |l| l.bindings.len()),
            )
        };

        // Bursts of ~2 s typing, then ~6 s pause (so the HUD fades in between),
        // a new layer with every burst
        let typing = tick % 36 < 9;
        if tick % 36 == 0 {
            let layer = (tick / 36) as usize % layer_count.max(1);
            hub.packet(LayerPacket::LayerState {
                default_layers: 1,
                active_layers: 1 | (1u32 << layer),
            });
        }

        // Press and release keys
        match held.take() {
            Some(position) => hub.packet(LayerPacket::KeyEvent { position, pressed: false }),
            None if typing => {
                let position = (tick.wrapping_mul(2654435761) >> 7) as usize % key_count.max(1);
                held = Some(position);
                hub.packet(LayerPacket::KeyEvent { position, pressed: true });
            }
            None => {}
        }
    }
}
