//! Typing speed from key presses, like ZMK's WPM widget on the nice!view: a
//! word is five key presses. ZMK averages over up to five seconds and then
//! starts over; this slides the five seconds along, so the number doesn't
//! jump back every five seconds. Port of `WPMMeter.swift`.

pub const WINDOW: f64 = 5.0;
pub const CHARS_PER_WORD: f64 = 5.0;
/// One sample a second for the graph
pub const HISTORY_LENGTH: usize = 30;

#[derive(Debug, Clone, Default)]
pub struct WpmMeter {
    presses: Vec<f64>,
    burst_start: Option<f64>,
    history: Vec<u32>,
}

impl WpmMeter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn history(&self) -> &[u32] {
        &self.history
    }

    pub fn record_press(&mut self, time: f64) {
        if let Some(&last) = self.presses.last() {
            if time - last > WINDOW {
                self.burst_start = None;
            }
        }
        if self.burst_start.is_none() {
            self.burst_start = Some(time);
        }
        self.presses.push(time);
    }

    /// Words per minute at `time`. At the start of a burst it averages over
    /// the time typed so far (at least a second), like ZMK.
    pub fn wpm(&mut self, time: f64) -> u32 {
        self.presses.retain(|&p| time - p <= WINDOW);
        let Some(start) = self.burst_start.filter(|_| !self.presses.is_empty()) else {
            self.burst_start = None;
            return 0;
        };
        let elapsed = WINDOW.min(1f64.max(time - start));
        let count = self
            .presses
            .iter()
            .filter(|&&p| time - p <= elapsed)
            .count();
        (count as f64 / CHARS_PER_WORD * 60.0 / elapsed).round() as u32
    }

    /// Once a second: the speed now, kept for the graph
    pub fn sample(&mut self, time: f64) -> u32 {
        let value = self.wpm(time);
        self.history.push(value);
        if self.history.len() > HISTORY_LENGTH {
            let extra = self.history.len() - HISTORY_LENGTH;
            self.history.drain(..extra);
        }
        value
    }

    /// Nothing typed lately and the graph is flat: no need to sample
    pub fn is_idle(&self) -> bool {
        self.presses.is_empty() && self.history.iter().all(|&v| v == 0)
    }

    /// Bindings that only switch layers or control the keyboard type nothing
    /// (ZMK bindings and QMK keycodes)
    pub fn counts(raw_code: &str) -> bool {
        let code = raw_code.trim_matches(|c| c == ' ' || c == '\t');
        const NON_TYPING: [&str; 10] = [
            "&mo ",
            "&tog ",
            "&to ",
            "&sl ",
            "&bt ",
            "&out ",
            "&none",
            "&sys_reset",
            "&bootloader",
            "&studio_unlock",
        ];
        const QMK_NON_TYPING_PREFIXES: [&str; 10] = [
            "MO(", "TG(", "TO(", "TT(", "OSL(", "DF(", "PDF(", "OSM(", "MS_", "KC_MS_",
        ];
        const QMK_NON_TYPING: [&str; 9] = [
            "KC_NO",
            "XXXXXXX",
            "QK_BOOT",
            "QK_BOOTLOADER",
            "RESET",
            "QK_RBT",
            "QK_REBOOT",
            "EE_CLR",
            "QK_CLEAR_EEPROM",
        ];
        !(NON_TYPING
            .iter()
            .any(|p| code == p.trim() || code.starts_with(p))
            || QMK_NON_TYPING_PREFIXES.iter().any(|p| code.starts_with(p))
            || QMK_NON_TYPING.contains(&code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `per_second` presses a second from `start` for `seconds`
    fn type_for(meter: &mut WpmMeter, per_second: f64, seconds: f64, start: f64) {
        let mut time = start;
        while time < start + seconds {
            meter.record_press(time);
            time += 1.0 / per_second;
        }
    }

    #[test]
    fn steady_typing() {
        let mut m = WpmMeter::new();
        type_for(&mut m, 6.0, 10.0, 0.0);
        // 6 key presses a second are 72 words a minute
        assert!((m.wpm(10.0) as f64 - 72.0).abs() <= 3.0);
    }

    #[test]
    fn start_of_a_burst_uses_the_time_typed_so_far() {
        let mut m = WpmMeter::new();
        type_for(&mut m, 6.0, 2.0, 0.0);
        assert!((m.wpm(2.0) as f64 - 72.0).abs() <= 3.0);
    }

    #[test]
    fn drops_to_zero_after_the_pause() {
        let mut m = WpmMeter::new();
        type_for(&mut m, 6.0, 10.0, 0.0);
        assert!(m.wpm(12.0) > 0);
        assert_eq!(m.wpm(16.0), 0);
    }

    #[test]
    fn a_new_burst_starts_fresh() {
        let mut m = WpmMeter::new();
        type_for(&mut m, 6.0, 5.0, 0.0);
        type_for(&mut m, 3.0, 2.0, 20.0);
        assert!((m.wpm(22.0) as f64 - 36.0).abs() <= 6.0);
    }

    #[test]
    fn qmk_layer_and_boot_keys_type_nothing() {
        assert!(WpmMeter::counts("KC_A"));
        assert!(WpmMeter::counts("LSFT_T(KC_A)"));
        assert!(WpmMeter::counts("LT(1, KC_SPC)"));
        assert!(!WpmMeter::counts("MO(1)"));
        assert!(!WpmMeter::counts("TG(_NAV)"));
        assert!(!WpmMeter::counts("XXXXXXX"));
        assert!(!WpmMeter::counts("QK_BOOT"));
        assert!(!WpmMeter::counts("MS_BTN1"));
    }

    #[test]
    fn history_keeps_thirty_seconds() {
        let mut m = WpmMeter::new();
        type_for(&mut m, 6.0, 5.0, 0.0);
        for second in 1..=40 {
            m.sample(second as f64);
        }
        assert_eq!(m.history().len(), 30);
        assert!(m.is_idle(), "flat after the pause");
    }

    #[test]
    fn layer_and_keyboard_keys_dont_count() {
        assert!(WpmMeter::counts("&kp A"));
        assert!(WpmMeter::counts("&mt LSHFT Z"));
        assert!(WpmMeter::counts("&lt 1 BSPC"));
        assert!(WpmMeter::counts("&trans"));
        assert!(!WpmMeter::counts("&mo 2"));
        assert!(!WpmMeter::counts("&tog 4"));
        assert!(!WpmMeter::counts("&bt BT_SEL 0"));
        assert!(!WpmMeter::counts("&none"));
        assert!(!WpmMeter::counts("&studio_unlock"));
    }
}
