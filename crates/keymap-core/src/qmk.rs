//! QMK keymaps: `keymap.json` (QMK Configurator, `qmk c2json`) and `keymap.c`.
//!
//! QMK keycodes get the same labels as their ZMK counterparts (`KC_SCLN` and
//! ZMK's `SEMI` both show `;`), so a QMK keymap looks like a ZMK one.
//!
//! The physical layout (rows, split, switch matrix) comes from the keyboard's
//! `keyboard_info.json` / `keyboard.json` / `info.json` when one sits next to
//! the keymap or in a folder above it, as in a `qmk_firmware` or userspace
//! checkout. Without it,
//! `keymap.c` rows follow the line breaks of the `LAYOUT(...)` call, and
//! `keymap.json` rows follow the key count.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::layout::KeyboardLayout;
use crate::parser::{
    calculate_keys_per_row, format_key, format_modifier, remove_comments, Firmware, KeyBinding,
    Keymap, KeymapLayer,
};

/// One key of a layout macro in the keyboard's info.json
#[derive(Debug, Clone, PartialEq)]
struct PhysicalKey {
    x: f64,
    matrix: Option<[u8; 2]>,
}

/// A layer as read from the file: the codes in LAYOUT order, each with the
/// line it is on (for the rows of `keymap.c`)
struct RawLayer {
    /// `_NAV` in `[_NAV] = LAYOUT(...)`; None in keymap.json
    designator: Option<String>,
    codes: Vec<(String, usize)>,
}

static KEYMAPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"keymaps\s*\[").unwrap());
static LAYER_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\[\s*([^\[\]]+?)\s*\]\s*=\s*)?(LAYOUT\w*)\s*\(").unwrap());

// MARK: - Files

/// Parses a QMK `keymap.json`. `info` is the keyboard's info.json, if found.
pub fn parse_json(text: &str, path: Option<&str>, info: Option<&str>) -> Result<Keymap, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Not a QMK keymap.json: {e}"))?;
    let layers = v
        .get("layers")
        .and_then(Value::as_array)
        .ok_or("No \"layers\" in this JSON file. Is it a QMK keymap.json?")?;

    let raw: Vec<RawLayer> = layers
        .iter()
        .map(|layer| RawLayer {
            designator: None,
            codes: layer
                .as_array()
                .map(|keys| {
                    keys.iter()
                        .map(|k| (k.as_str().unwrap_or("KC_NO").to_string(), 0))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();

    let keyboard = v.get("keyboard").and_then(Value::as_str).unwrap_or("");
    let layout_name = v.get("layout").and_then(Value::as_str).unwrap_or("");
    build(raw, keyboard, layout_name, path, info)
}

/// Parses the `keymaps[]` array of a QMK `keymap.c`
pub fn parse_c(text: &str, path: Option<&str>, info: Option<&str>) -> Result<Keymap, String> {
    let cleaned = remove_comments(&text.replace("\r\n", "\n"));
    let not_found = "No keymaps[] array in this file. Is it a QMK keymap.c?";
    let start = KEYMAPS.find(&cleaned).ok_or(not_found)?;
    // keymaps[][MATRIX_ROWS][MATRIX_COLS] = { ... }: the body starts after the '='
    let equals = cleaned[start.end()..].find('=').ok_or(not_found)? + start.end();
    let open = cleaned[equals..].find('{').ok_or(not_found)? + equals;
    let close = matching(&cleaned, open, b'{', b'}').ok_or(not_found)?;
    let body = &cleaned[open + 1..close];

    let mut raw = Vec::new();
    let mut layout_name = String::new();
    let mut at = 0;
    while let Some(caps) = LAYER_START.captures_at(body, at) {
        let whole = caps.get(0).unwrap();
        let paren = whole.end() - 1;
        let Some(end) = matching(body, paren, b'(', b')') else {
            break;
        };
        if layout_name.is_empty() {
            layout_name = caps[2].to_string();
        }
        raw.push(RawLayer {
            designator: caps.get(1).map(|d| d.as_str().trim().to_string()),
            codes: split_args(&body[paren + 1..end]),
        });
        at = end + 1;
    }
    if raw.is_empty() {
        return Err("No LAYOUT(...) layers in the keymaps[] array".to_string());
    }

    // The keyboard's folder name says which board it is (keyboards/ferris/sweep/...)
    build(raw, "", &layout_name, path, info)
}

/// The keyboard's `keyboard_info.json` (from `qmk info -m -f json`),
/// `keyboard.json` or `info.json` with a `layouts` section,
/// next to the keymap or up to five folders above it
pub fn find_keyboard_info(keymap_path: &str) -> Option<String> {
    let mut dir = Path::new(keymap_path).parent();
    for _ in 0..6 {
        let d = dir?;
        for name in ["keyboard_info.json", "keyboard.json", "info.json"] {
            let candidate = d.join(name);
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                if text.contains("\"layouts\"") {
                    return Some(text);
                }
            }
        }
        dir = d.parent();
    }
    None
}

// MARK: - Building the keymap

fn build(
    raw: Vec<RawLayer>,
    keyboard: &str,
    layout_name: &str,
    path: Option<&str>,
    info: Option<&str>,
) -> Result<Keymap, String> {
    let key_count = raw.first().map_or(0, |l| l.codes.len());
    if key_count == 0 {
        return Err("The keymap has no keys".to_string());
    }

    let physical = info.and_then(|i| physical_layout(i, layout_name, key_count));
    let rows = match &physical {
        Some(keys) => rows_from_positions(keys),
        None => rows_from_lines(&raw[0].codes).unwrap_or_else(|| rows_from_count(key_count)),
    };

    let designators: Vec<Option<String>> = raw.iter().map(|l| l.designator.clone()).collect();
    let layers: Vec<KeymapLayer> = raw
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let mut bindings = Vec::with_capacity(layer.codes.len());
            let mut column = 0;
            for (i, (code, _)) in layer.codes.iter().enumerate() {
                let row = rows.get(i).or(rows.last()).copied().unwrap_or(0);
                if i > 0 && rows.get(i - 1) != Some(&row) {
                    column = 0;
                }
                bindings.push(KeyBinding {
                    display_text: label(code, &designators),
                    raw_code: code.clone(),
                    name: None,
                    row,
                    column,
                });
                column += 1;
            }
            let keys_per_row = calculate_keys_per_row(&bindings);
            KeymapLayer {
                name: layer_name(layer.designator.as_deref(), index),
                row_count: keys_per_row.len(),
                column_count: keys_per_row.iter().copied().max().unwrap_or(0),
                bindings,
            }
        })
        .collect();

    // Board name and split: the known boards first, then the layout's name and shape
    let hint = format!("{keyboard} {layout_name}");
    let known = KeyboardLayout::detect_keyboard_info(&hint, path);
    let is_split = known.map(|k| k.1).unwrap_or_else(|| {
        layout_name.to_lowercase().contains("split")
            || physical.as_ref().is_some_and(|keys| has_middle_gap(keys))
    });
    let name = known
        .map(|k| k.0)
        .or((!keyboard.is_empty()).then_some(keyboard));

    let keys_per_row = calculate_keys_per_row(&layers[0].bindings);
    let layout = KeyboardLayout::create(key_count, keys_per_row, name, is_split);
    let matrix = physical
        .and_then(|keys| keys.iter().map(|k| k.matrix).collect::<Option<Vec<_>>>())
        .unwrap_or_default();

    Ok(Keymap {
        layers,
        layout,
        behaviors: Default::default(),
        macros: Default::default(),
        firmware: Firmware::Qmk,
        matrix,
    })
}

fn layer_name(designator: Option<&str>, index: usize) -> String {
    match designator.map(|d| d.trim_start_matches('_')) {
        Some(d) if !d.is_empty() && d.parse::<usize>().is_err() => d.to_string(),
        _ => format!("Layer {index}"),
    }
}

/// The keys of the layout macro the keymap uses, from the keyboard's info.json
fn physical_layout(info: &str, layout_name: &str, key_count: usize) -> Option<Vec<PhysicalKey>> {
    let v: Value = serde_json::from_str(info).ok()?;
    let layouts = v.get("layouts")?.as_object()?;
    let key_list = |l: &Value| l.get("layout").and_then(Value::as_array).cloned();

    let resolved = v
        .get("layout_aliases")
        .and_then(|a| a.get(layout_name))
        .and_then(Value::as_str)
        .unwrap_or(layout_name);
    let keys = layouts
        .get(resolved)
        .and_then(key_list)
        .filter(|k| k.len() == key_count)
        .or_else(|| {
            layouts
                .values()
                .filter_map(key_list)
                .find(|k| k.len() == key_count)
        })?;

    Some(
        keys.iter()
            .map(|k| PhysicalKey {
                x: k.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                matrix: k.get("matrix").and_then(|m| {
                    let m = m.as_array()?;
                    Some([m.first()?.as_u64()? as u8, m.get(1)?.as_u64()? as u8])
                }),
            })
            .collect(),
    )
}

/// QMK lists the keys row by row, left to right: a key further left than the
/// one before starts a new row
fn rows_from_positions(keys: &[PhysicalKey]) -> Vec<usize> {
    let mut row = 0;
    let mut rows = Vec::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        if i > 0 && key.x <= keys[i - 1].x {
            row += 1;
        }
        rows.push(row);
    }
    rows
}

/// keymap.c: one row per line of the LAYOUT call. None if that says nothing
/// (all keys on one line, or every key on its own line).
fn rows_from_lines(codes: &[(String, usize)]) -> Option<Vec<usize>> {
    let mut rows = Vec::with_capacity(codes.len());
    let mut row = 0;
    for (i, (_, line)) in codes.iter().enumerate() {
        if i > 0 && *line != codes[i - 1].1 {
            row += 1;
        }
        rows.push(row);
    }
    let row_count = row + 1;
    (row_count > 1 && row_count < codes.len()).then_some(rows)
}

/// No layout information: rows of 10 (up to 40 keys) or 12, the rest is the
/// thumb row. Right for the common splits (34, 36, 42 keys) and the Planck.
fn rows_from_count(key_count: usize) -> Vec<usize> {
    let width = if key_count >= 42 { 12 } else { 10 };
    (0..key_count).map(|i| i / width).collect()
}

/// A split keyboard has a gap of well over one key in the middle of a row
fn has_middle_gap(keys: &[PhysicalKey]) -> bool {
    keys.windows(2).any(|w| w[1].x - w[0].x >= 2.0)
}

/// Byte index of the bracket that closes the one at `open`
fn matching(s: &str, open: usize, open_ch: u8, close_ch: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if b == open_ch {
            depth += 1;
        } else if b == close_ch {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Splits macro arguments at the top-level commas; each with its line number
fn split_args(args: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    let mut line = 0;
    let mut token_line = None;

    let mut push = |current: &mut String, token_line: &mut Option<usize>| {
        let t = current.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() {
            out.push((t, token_line.unwrap_or(0)));
        }
        current.clear();
        *token_line = None;
    };

    for c in args.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                push(&mut current, &mut token_line);
                continue;
            }
            '\n' => line += 1,
            _ => {}
        }
        if !c.is_whitespace() && token_line.is_none() {
            token_line = Some(line);
        }
        current.push(c);
    }
    push(&mut current, &mut token_line);
    out
}

// MARK: - Labels

/// The label of a QMK keycode, in the style of the ZMK labels.
/// `layers` are the layer designators, for `MO(_NAV)` and friends.
fn label(code: &str, layers: &[Option<String>]) -> String {
    let code = code.trim();
    match code {
        "KC_TRNS" | "KC_TRANSPARENT" | "_______" | "TRNS" => return "▽".to_string(),
        "KC_NO" | "XXXXXXX" | "NO" => return "✕".to_string(),
        "QK_BOOT" | "QK_BOOTLOADER" | "RESET" => return "BOOT".to_string(),
        "QK_RBT" | "QK_REBOOT" => return "RESET".to_string(),
        "EE_CLR" | "QK_CLEAR_EEPROM" => return "EE CLR".to_string(),
        "CW_TOGG" | "QK_CAPS_WORD_TOGGLE" => return "CAPS W".to_string(),
        "QK_REP" | "QK_REPEAT_KEY" => return "REP".to_string(),
        "QK_LEAD" | "QK_LEADER" => return "LEAD".to_string(),
        "QK_GESC" | "QK_GRAVE_ESCAPE" => return "ESC".to_string(),
        _ => {}
    }
    if let Some(expr) = key_expr(code) {
        return format_key(&expr);
    }
    if let Some(mouse) = mouse_label(code) {
        return mouse.to_string();
    }
    if let Some((func, args)) = call(code) {
        let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
        let layer = || layer_ref(arg(0), layers);
        match func {
            "MO" => return format!("MO{}", layer()),
            "TG" => return format!("TG{}", layer()),
            "TO" => return format!("TO{}", layer()),
            "TT" => return format!("TT{}", layer()),
            "OSL" => return format!("OSL{}", layer()),
            "DF" | "PDF" => return format!("DF{}", layer()),
            "LT" if args.len() == 2 => {
                return format!("L{}\n{}", layer(), label(arg(1), layers));
            }
            "LM" if args.len() == 2 => return format!("L{}\n{}", layer(), mods_label(arg(1))),
            "MT" if args.len() == 2 => {
                return format!("{}\n{}", mods_label(arg(0)), label(arg(1), layers));
            }
            "OSM" => return format!("OS\n{}", mods_label(arg(0))),
            "TD" => return format!("TD\n{}", clean_name(arg(0))),
            _ => {}
        }
        if let Some(mods) = mod_tap(func) {
            let mods: Vec<String> = mods.iter().map(|m| format_modifier(m)).collect();
            return format!("{}\n{}", mods.join("+"), label(arg(0), layers));
        }
    }
    clean_name(code)
}

/// Unknown codes (custom keycodes, features): the name without its prefix
fn clean_name(code: &str) -> String {
    let name = code
        .strip_prefix("KC_")
        .or_else(|| code.strip_prefix("QK_"))
        .unwrap_or(code);
    name.trim_start_matches('_').to_uppercase()
}

/// `MO(_NAV)` -> the index of the `[_NAV]` layer; numbers stay numbers
fn layer_ref(arg: &str, layers: &[Option<String>]) -> String {
    let arg = arg.trim();
    if arg.parse::<usize>().is_ok() {
        return arg.to_string();
    }
    match layers.iter().position(|d| d.as_deref() == Some(arg)) {
        Some(index) => index.to_string(),
        None => arg.trim_start_matches('_').to_string(),
    }
}

/// `FUNC(a, b)` -> ("FUNC", ["a", "b"])
fn call(code: &str) -> Option<(&str, Vec<String>)> {
    let open = code.find('(')?;
    if !code.ends_with(')') {
        return None;
    }
    let func = code[..open].trim();
    if func.is_empty() || !func.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let args = split_args(&code[open + 1..code.len() - 1])
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    Some((func, args))
}

/// A QMK key as a ZMK key expression: `KC_SCLN` -> `SEMI`, `LSFT(KC_1)` -> `LS(N1)`
fn key_expr(code: &str) -> Option<String> {
    if let Some((func, args)) = call(code) {
        let (_, mods) = MOD_WRAPPERS.iter().find(|(name, _)| *name == func)?;
        let inner = key_expr(args.first()?)?;
        return Some(
            mods.iter()
                .rev()
                .fold(inner, |acc, m| format!("{m}({acc})")),
        );
    }

    let name = code.strip_prefix("KC_")?;
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => return Some(name.to_string()),
        (Some(c), None) if c.is_ascii_digit() => return Some(format!("N{c}")),
        _ => {}
    }
    if name.len() <= 3 && name.starts_with('F') && name[1..].parse::<u8>().is_ok() {
        return Some(name.to_string());
    }
    if let Some(digit) = name
        .strip_prefix("KP_")
        .or_else(|| name.strip_prefix('P'))
        .filter(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()))
    {
        return Some(format!("KP_N{digit}"));
    }
    KEYS.iter()
        .find(|(names, _)| names.contains(&name))
        .map(|(_, zmk)| zmk.to_string())
}

/// `MOD_LCTL | MOD_LSFT` -> "⌃+⇧"
fn mods_label(arg: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for part in arg.split('|').map(str::trim) {
        let mods: &[&str] = match part {
            "MOD_LCTL" | "MOD_RCTL" | "MOD_MASK_CTRL" => &["LCTRL"],
            "MOD_LSFT" | "MOD_RSFT" | "MOD_MASK_SHIFT" => &["LSHFT"],
            "MOD_LALT" | "MOD_RALT" | "MOD_MASK_ALT" => &["LALT"],
            "MOD_LGUI" | "MOD_RGUI" | "MOD_MASK_GUI" => &["LGUI"],
            "MOD_MASK_CS" => &["LCTRL", "LSHFT"],
            "MOD_MASK_CA" => &["LCTRL", "LALT"],
            "MOD_MASK_CG" => &["LCTRL", "LGUI"],
            "MOD_MASK_SA" => &["LSHFT", "LALT"],
            "MOD_MASK_SG" => &["LSHFT", "LGUI"],
            "MOD_MASK_AG" => &["LALT", "LGUI"],
            "MOD_MEH" | "MOD_MASK_CSA" => &["LCTRL", "LSHFT", "LALT"],
            "MOD_HYPR" | "MOD_MASK_CSAG" => &["LCTRL", "LSHFT", "LALT", "LGUI"],
            other => {
                out.push(other.trim_start_matches("MOD_").to_string());
                continue;
            }
        };
        out.extend(mods.iter().map(|m| format_modifier(m)));
    }
    out.join("+")
}

/// The modifiers of a mod-tap shortcut like `LSFT_T(KC_A)`
fn mod_tap(func: &str) -> Option<&'static [&'static str]> {
    Some(match func {
        "LCTL_T" | "CTL_T" | "RCTL_T" => &["LCTRL"],
        "LSFT_T" | "SFT_T" | "RSFT_T" => &["LSHFT"],
        "LALT_T" | "ALT_T" | "LOPT_T" | "OPT_T" | "RALT_T" | "ROPT_T" | "ALGR_T" => &["LALT"],
        "LGUI_T" | "GUI_T" | "LCMD_T" | "CMD_T" | "LWIN_T" | "WIN_T" | "RGUI_T" | "RCMD_T"
        | "RWIN_T" => &["LGUI"],
        "LCS_T" | "C_S_T" | "RCS_T" => &["LCTRL", "LSHFT"],
        "LCA_T" => &["LCTRL", "LALT"],
        "LCG_T" => &["LCTRL", "LGUI"],
        "LSA_T" => &["LSHFT", "LALT"],
        "LSG_T" | "SGUI_T" | "SCMD_T" | "SWIN_T" => &["LSHFT", "LGUI"],
        "LAG_T" => &["LALT", "LGUI"],
        "LCAG_T" => &["LCTRL", "LALT", "LGUI"],
        "MEH_T" => &["LCTRL", "LSHFT", "LALT"],
        "HYPR_T" | "ALL_T" => &["LCTRL", "LSHFT", "LALT", "LGUI"],
        _ => return None,
    })
}

fn mouse_label(code: &str) -> Option<&'static str> {
    let name = code
        .strip_prefix("KC_MS_")
        .or_else(|| code.strip_prefix("MS_"))
        .or_else(|| code.strip_prefix("KC_"))?;
    Some(match name {
        "UP" | "U" => "M↑",
        "DOWN" | "D" => "M↓",
        "LEFT" | "L" => "M←",
        "RIGHT" | "RGHT" | "R" => "M→",
        "BTN1" => "MB1",
        "BTN2" => "MB2",
        "BTN3" => "MB3",
        "BTN4" => "MB4",
        "BTN5" => "MB5",
        "WH_UP" | "WH_U" | "WHLU" => "W↑",
        "WH_DOWN" | "WH_D" | "WHLD" => "W↓",
        "WH_LEFT" | "WH_L" | "WHLL" => "W←",
        "WH_RIGHT" | "WH_R" | "WHLR" => "W→",
        "ACL0" | "ACCEL0" => "ACC0",
        "ACL1" | "ACCEL1" => "ACC1",
        "ACL2" | "ACCEL2" => "ACC2",
        _ => return None,
    })
}

/// Modifier wrappers like `LSFT(kc)` and the ZMK modifier functions they become
const MOD_WRAPPERS: &[(&str, &[&str])] = &[
    ("LCTL", &["LC"]),
    ("C", &["LC"]),
    ("LSFT", &["LS"]),
    ("S", &["LS"]),
    ("LALT", &["LA"]),
    ("A", &["LA"]),
    ("LOPT", &["LA"]),
    ("LGUI", &["LG"]),
    ("G", &["LG"]),
    ("LCMD", &["LG"]),
    ("LWIN", &["LG"]),
    ("RCTL", &["RC"]),
    ("RSFT", &["RS"]),
    ("RALT", &["RA"]),
    ("ROPT", &["RA"]),
    ("ALGR", &["RA"]),
    ("RGUI", &["RG"]),
    ("RCMD", &["RG"]),
    ("RWIN", &["RG"]),
    ("LCS", &["LC", "LS"]),
    ("C_S", &["LC", "LS"]),
    ("RCS", &["RC", "RS"]),
    ("LCA", &["LC", "LA"]),
    ("LCG", &["LC", "LG"]),
    ("LSA", &["LS", "LA"]),
    ("SAGR", &["RS", "RA"]),
    ("LSG", &["LS", "LG"]),
    ("SGUI", &["LS", "LG"]),
    ("SCMD", &["LS", "LG"]),
    ("SWIN", &["LS", "LG"]),
    ("LAG", &["LA", "LG"]),
    ("LCAG", &["LC", "LA", "LG"]),
    ("MEH", &["LC", "LS", "LA"]),
    ("HYPR", &["LC", "LS", "LA", "LG"]),
];

/// QMK key names (without `KC_`, long and short form) and their ZMK name
const KEYS: &[(&[&str], &str)] = &[
    (&["ENTER", "ENT"], "RET"),
    (&["ESCAPE", "ESC"], "ESC"),
    (&["BACKSPACE", "BSPC"], "BSPC"),
    (&["TAB"], "TAB"),
    (&["SPACE", "SPC"], "SPC"),
    (&["MINUS", "MINS"], "MINUS"),
    (&["EQUAL", "EQL"], "EQUAL"),
    (&["LEFT_BRACKET", "LBRC"], "LBKT"),
    (&["RIGHT_BRACKET", "RBRC"], "RBKT"),
    (&["BACKSLASH", "BSLS"], "BSLH"),
    (&["NONUS_HASH", "NUHS"], "NON_US_HASH"),
    (&["SEMICOLON", "SCLN"], "SEMI"),
    (&["QUOTE", "QUOT"], "SQT"),
    (&["GRAVE", "GRV"], "GRAVE"),
    (&["COMMA", "COMM"], "COMMA"),
    (&["DOT"], "DOT"),
    (&["SLASH", "SLSH"], "FSLH"),
    (&["NONUS_BACKSLASH", "NUBS"], "NON_US_BSLH"),
    (&["CAPS_LOCK", "CAPS"], "CAPS"),
    (&["PRINT_SCREEN", "PSCR"], "PSCRN"),
    (&["SCROLL_LOCK", "SCRL", "SLCK"], "SLCK"),
    (&["PAUSE", "PAUS", "BRK"], "PAUSE_BREAK"),
    (&["INSERT", "INS"], "INS"),
    (&["HOME"], "HOME"),
    (&["PAGE_UP", "PGUP"], "PG_UP"),
    (&["DELETE", "DEL"], "DEL"),
    (&["END"], "END"),
    (&["PAGE_DOWN", "PGDN"], "PG_DN"),
    (&["RIGHT", "RGHT"], "RIGHT"),
    (&["LEFT"], "LEFT"),
    (&["DOWN"], "DOWN"),
    (&["UP"], "UP"),
    (&["APPLICATION", "APP"], "K_APP"),
    (&["LEFT_CTRL", "LCTL"], "LCTRL"),
    (&["LEFT_SHIFT", "LSFT"], "LSHFT"),
    (&["LEFT_ALT", "LALT", "LOPT"], "LALT"),
    (&["LEFT_GUI", "LGUI", "LCMD", "LWIN"], "LGUI"),
    (&["RIGHT_CTRL", "RCTL"], "RCTRL"),
    (&["RIGHT_SHIFT", "RSFT"], "RSHFT"),
    (&["RIGHT_ALT", "RALT", "ROPT", "ALGR"], "RALT"),
    (&["RIGHT_GUI", "RGUI", "RCMD", "RWIN"], "RGUI"),
    (&["TILDE", "TILD"], "TILDE"),
    (&["EXCLAIM", "EXLM"], "EXCL"),
    (&["AT"], "AT"),
    (&["HASH"], "HASH"),
    (&["DOLLAR", "DLR"], "DLLR"),
    (&["PERCENT", "PERC"], "PRCNT"),
    (&["CIRCUMFLEX", "CIRC"], "CARET"),
    (&["AMPERSAND", "AMPR"], "AMPS"),
    (&["ASTERISK", "ASTR"], "STAR"),
    (&["LEFT_PAREN", "LPRN"], "LPAR"),
    (&["RIGHT_PAREN", "RPRN"], "RPAR"),
    (&["UNDERSCORE", "UNDS"], "UNDER"),
    (&["PLUS"], "PLUS"),
    (&["LEFT_CURLY_BRACE", "LCBR"], "LBRC"),
    (&["RIGHT_CURLY_BRACE", "RCBR"], "RBRC"),
    (&["PIPE"], "PIPE"),
    (&["COLON", "COLN"], "COLON"),
    (&["DOUBLE_QUOTE", "DQUO", "DQT"], "DQT"),
    (&["LEFT_ANGLE_BRACKET", "LABK", "LT"], "LT"),
    (&["RIGHT_ANGLE_BRACKET", "RABK", "GT"], "GT"),
    (&["QUESTION", "QUES"], "QMARK"),
    (&["AUDIO_MUTE", "MUTE"], "C_MUTE"),
    (&["AUDIO_VOL_UP", "VOLU"], "C_VOL_UP"),
    (&["AUDIO_VOL_DOWN", "VOLD"], "C_VOL_DN"),
    (&["MEDIA_NEXT_TRACK", "MNXT"], "C_NEXT"),
    (&["MEDIA_PREV_TRACK", "MPRV"], "C_PREV"),
    (&["MEDIA_PLAY_PAUSE", "MPLY"], "C_PP"),
    (&["BRIGHTNESS_UP", "BRIU"], "C_BRI_UP"),
    (&["BRIGHTNESS_DOWN", "BRID"], "C_BRI_DN"),
    (&["KP_SLASH", "PSLS"], "KP_SLASH"),
    (&["KP_ASTERISK", "PAST"], "KP_MULTIPLY"),
    (&["KP_MINUS", "PMNS"], "KP_MINUS"),
    (&["KP_PLUS", "PPLS"], "KP_PLUS"),
    (&["KP_ENTER", "PENT"], "KP_ENTER"),
    (&["KP_DOT", "PDOT"], "KP_DOT"),
    (&["KP_EQUAL", "PEQL"], "KP_EQUAL"),
    (&["NUM_LOCK", "NUM"], "KP_NUMLOCK"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_chars::test_support::TABLE_LOCK;
    use crate::parser::{set_modifier_style, ModifierStyle};

    /// Labels without the OS keyboard layout and with the Mac modifiers
    fn plain() -> std::sync::MutexGuard<'static, ()> {
        let guard = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::KeyCharacters::clear();
        set_modifier_style(ModifierStyle::Mac);
        guard
    }

    fn l(code: &str) -> String {
        label(code, &[])
    }

    /// A 34-key Sweep keymap.json as the QMK Configurator exports it
    fn sweep_json() -> String {
        let base = [
            "KC_Q",
            "KC_W",
            "KC_E",
            "KC_R",
            "KC_T",
            "KC_Y",
            "KC_U",
            "KC_I",
            "KC_O",
            "KC_P",
            "LGUI_T(KC_A)",
            "LALT_T(KC_S)",
            "LCTL_T(KC_D)",
            "LSFT_T(KC_F)",
            "KC_G",
            "KC_H",
            "RSFT_T(KC_J)",
            "RCTL_T(KC_K)",
            "LALT_T(KC_L)",
            "RGUI_T(KC_SCLN)",
            "KC_Z",
            "KC_X",
            "KC_C",
            "KC_V",
            "KC_B",
            "KC_N",
            "KC_M",
            "KC_COMM",
            "KC_DOT",
            "KC_SLSH",
            "MO(1)",
            "KC_SPC",
            "LT(2, KC_ENT)",
            "KC_BSPC",
        ];
        let nav: Vec<&str> = (0..34)
            .map(|i| if i == 15 { "KC_LEFT" } else { "KC_TRNS" })
            .collect();
        let fun: Vec<&str> = (0..34)
            .map(|i| if i == 0 { "QK_BOOT" } else { "_______" })
            .collect();
        serde_json::json!({
            "keyboard": "ferris/sweep",
            "keymap": "default",
            "layout": "LAYOUT_split_3x5_2",
            "layers": [base.to_vec(), nav, fun],
        })
        .to_string()
    }

    #[test]
    fn labels_of_basic_keys() {
        let _g = plain();
        assert_eq!(l("KC_A"), "A");
        assert_eq!(l("KC_1"), "1");
        assert_eq!(l("KC_F12"), "F12");
        assert_eq!(l("KC_SCLN"), ";");
        assert_eq!(l("KC_SEMICOLON"), ";");
        assert_eq!(l("KC_QUOT"), "'");
        assert_eq!(l("KC_SPC"), "␣");
        assert_eq!(l("KC_BSPC"), "⌫");
        assert_eq!(l("KC_ENT"), "⏎");
        assert_eq!(l("KC_EXLM"), "!");
        assert_eq!(l("KC_LPRN"), "(");
        assert_eq!(l("KC_LEFT"), "←");
        assert_eq!(l("KC_VOLU"), "VOL+");
        assert_eq!(l("KC_MPLY"), "⏯");
        assert_eq!(l("KC_P7"), "7");
    }

    #[test]
    fn labels_of_special_keys() {
        let _g = plain();
        assert_eq!(l("KC_TRNS"), "▽");
        assert_eq!(l("_______"), "▽");
        assert_eq!(l("XXXXXXX"), "✕");
        assert_eq!(l("KC_NO"), "✕");
        assert_eq!(l("QK_BOOT"), "BOOT");
        assert_eq!(l("MS_BTN1"), "MB1");
        assert_eq!(l("KC_WH_U"), "W↑");
        assert_eq!(l("CW_TOGG"), "CAPS W");
    }

    #[test]
    fn labels_of_layer_keys() {
        let _g = plain();
        assert_eq!(l("MO(1)"), "MO1");
        assert_eq!(l("TG(3)"), "TG3");
        assert_eq!(l("TO(0)"), "TO0");
        assert_eq!(l("OSL(2)"), "OSL2");
        assert_eq!(l("LT(2, KC_ENT)"), "L2\n⏎");
        assert_eq!(l("LT(1,KC_SPC)"), "L1\n␣");
    }

    #[test]
    fn layer_names_resolve_to_indices() {
        let _g = plain();
        let layers = vec![Some("_BASE".to_string()), Some("_NAV".to_string())];
        assert_eq!(label("MO(_NAV)", &layers), "MO1");
        assert_eq!(label("LT(_NAV, KC_TAB)", &layers), "L1\n⇥");
        // An unknown name stays a name
        assert_eq!(label("MO(_SYM)", &layers), "MOSYM");
    }

    #[test]
    fn labels_of_mod_taps_and_modified_keys() {
        let _g = plain();
        assert_eq!(l("LSFT_T(KC_A)"), "⇧\nA");
        assert_eq!(l("LCTL_T(KC_D)"), "⌃\nD");
        assert_eq!(l("MT(MOD_LGUI, KC_A)"), "⌘\nA");
        assert_eq!(l("MT(MOD_LCTL | MOD_LSFT, KC_Z)"), "⌃+⇧\nZ");
        assert_eq!(l("MEH_T(KC_ESC)"), "⌃+⇧+⌥\nESC");
        assert_eq!(l("OSM(MOD_LSFT)"), "OS\n⇧");
        assert_eq!(l("LGUI(KC_C)"), "⌘+C");
        assert_eq!(l("LCTL(LSFT(KC_T))"), "⌃+⇧+T");
        assert_eq!(l("C_S(KC_T)"), "⌃+⇧+T");
        assert_eq!(l("S(KC_1)"), "⇧+1");
    }

    #[test]
    fn modifier_names_follow_the_style() {
        let _g = plain();
        set_modifier_style(ModifierStyle::Windows);
        assert_eq!(l("LGUI_T(KC_A)"), "Win\nA");
        assert_eq!(l("LCTL(KC_C)"), "Ctrl+C");
        set_modifier_style(ModifierStyle::Mac);
    }

    #[test]
    fn unknown_codes_show_their_name() {
        let _g = plain();
        assert_eq!(l("KC_KB_MUTE"), "KB_MUTE");
        assert_eq!(l("MY_MACRO"), "MY_MACRO");
        assert_eq!(l("TD(TD_ESC_CAPS)"), "TD\nTD_ESC_CAPS");
        assert_eq!(l("RGB_TOG"), "RGB_TOG");
    }

    #[test]
    fn parses_a_keymap_json() {
        let _g = plain();
        let km = parse_json(&sweep_json(), Some("/x/sweep.json"), None).unwrap();
        assert_eq!(km.firmware, Firmware::Qmk);
        assert_eq!(km.layers.len(), 3);
        assert_eq!(km.layers[0].name, "Layer 0");
        assert_eq!(km.layers[0].bindings.len(), 34);
        assert_eq!(km.layout.name, "Sweep/Cradio");
        assert!(km.layout.is_split);
        assert_eq!(km.layout.keys_per_row, vec![10, 10, 10, 4]);
        assert!(km.layout.has_thumb_cluster);
        let b = &km.layers[0].bindings;
        assert_eq!(b[10].display_text, "⌘\nA");
        assert_eq!(b[10].raw_code, "LGUI_T(KC_A)");
        assert_eq!((b[10].row, b[10].column), (1, 0));
        assert_eq!(b[30].display_text, "MO1");
        assert_eq!((b[33].row, b[33].column), (3, 3));
        assert_eq!(km.layers[1].bindings[15].display_text, "←");
        assert_eq!(km.layers[2].bindings[0].display_text, "BOOT");
        assert!(km.matrix.is_empty());
    }

    #[test]
    fn rejects_other_json() {
        assert!(parse_json("{\"foo\": 1}", None, None).is_err());
        assert!(parse_json("not json", None, None).is_err());
        assert!(parse_json("{\"layers\": []}", None, None).is_err());
    }

    /// info.json of a made-up 2x3 split board, right half mirrored in the matrix
    const INFO: &str = r#"{
        "keyboard_name": "Tiny Split",
        "layout_aliases": { "LAYOUT": "LAYOUT_split_2x3" },
        "layouts": {
            "LAYOUT_split_2x3": {
                "layout": [
                    {"matrix": [0, 0], "x": 0, "y": 0},
                    {"matrix": [0, 1], "x": 1, "y": 0},
                    {"matrix": [0, 2], "x": 2, "y": 0},
                    {"matrix": [2, 2], "x": 5, "y": 0},
                    {"matrix": [2, 1], "x": 6, "y": 0},
                    {"matrix": [2, 0], "x": 7, "y": 0},
                    {"matrix": [1, 1], "x": 1.5, "y": 1.2},
                    {"matrix": [1, 2], "x": 2.5, "y": 1.2},
                    {"matrix": [3, 2], "x": 4.5, "y": 1.2},
                    {"matrix": [3, 1], "x": 5.5, "y": 1.2}
                ]
            }
        }
    }"#;

    fn tiny_json() -> String {
        let keys: Vec<String> = (0..10)
            .map(|i| format!("KC_{}", (b'A' + i) as char))
            .collect();
        serde_json::json!({ "keyboard": "tiny", "layout": "LAYOUT", "layers": [keys] }).to_string()
    }

    #[test]
    fn info_json_gives_rows_split_and_matrix() {
        let _g = plain();
        let km = parse_json(&tiny_json(), None, Some(INFO)).unwrap();
        assert_eq!(km.layout.keys_per_row, vec![6, 4]);
        assert!(km.layout.is_split);
        assert_eq!(km.layout.name, "tiny");
        assert_eq!(km.matrix.len(), 10);
        assert_eq!(km.matrix[3], [2, 2]);
        // Key presses arrive by matrix position
        assert_eq!(km.position_of(2, 2), Some(3));
        assert_eq!(km.position_of(3, 1), Some(9));
        assert_eq!(km.position_of(9, 9), None);
    }

    #[test]
    fn info_json_with_other_key_count_is_ignored() {
        let _g = plain();
        let keys: Vec<String> = (0..12).map(|_| "KC_A".to_string()).collect();
        let json = serde_json::json!({ "layout": "LAYOUT", "layers": [keys] }).to_string();
        let km = parse_json(&json, None, Some(INFO)).unwrap();
        assert!(km.matrix.is_empty());
        assert_eq!(km.layout.keys_per_row, vec![10, 2]);
    }

    #[test]
    fn without_matrix_qmk_key_presses_are_unknown() {
        let _g = plain();
        let km = parse_json(&sweep_json(), None, None).unwrap();
        assert_eq!(km.position_of(0, 5), None);
    }

    const KEYMAP_C: &str = r#"
#include QMK_KEYBOARD_H

enum layers { _BASE, _NAV, _NUM };

// Home row mods
#define HOME_A LGUI_T(KC_A)

const uint16_t PROGMEM keymaps[][MATRIX_ROWS][MATRIX_COLS] = {
    [_BASE] = LAYOUT_split_3x5_2(
        KC_Q,    KC_W,    KC_E,    KC_R,    KC_T,         KC_Y,    KC_U,    KC_I,    KC_O,    KC_P,
        HOME_A,  KC_S,    KC_D,    KC_F,    KC_G,         KC_H,    KC_J,    KC_K,    KC_L,    KC_SCLN,
        KC_Z,    KC_X,    KC_C,    KC_V,    KC_B,         KC_N,    KC_M,    KC_COMM, KC_DOT,  KC_SLSH,
                          MO(_NAV), KC_SPC,               LT(_NUM, KC_ENT), KC_BSPC
    ),
    /* Navigation */
    [_NAV] = LAYOUT_split_3x5_2(
        _______, _______, _______, _______, _______,      _______, _______, _______, _______, _______,
        _______, _______, _______, _______, _______,      KC_LEFT, KC_DOWN, KC_UP,   KC_RGHT, _______,
        _______, _______, _______, _______, _______,      _______, _______, _______, _______, _______,
                          _______, _______,               _______, _______
    ),
    [_NUM] = LAYOUT_split_3x5_2(
        KC_1,    KC_2,    KC_3,    KC_4,    KC_5,         KC_6,    KC_7,    KC_8,    KC_9,    KC_0,
        _______, _______, _______, _______, _______,      _______, _______, _______, _______, _______,
        _______, _______, _______, _______, _______,      _______, _______, _______, _______, QK_BOOT,
                          _______, _______,               _______, _______
    )
};
"#;

    #[test]
    fn parses_a_keymap_c() {
        let _g = plain();
        let km = parse_c(
            KEYMAP_C,
            Some("/q/keyboards/ferris/sweep/keymaps/me/keymap.c"),
            None,
        )
        .unwrap();
        assert_eq!(km.firmware, Firmware::Qmk);
        let names: Vec<&str> = km.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["BASE", "NAV", "NUM"]);
        assert_eq!(km.layout.name, "Sweep/Cradio");
        assert!(km.layout.is_split);
        assert_eq!(km.layout.keys_per_row, vec![10, 10, 10, 4]);
        let b = &km.layers[0].bindings;
        assert_eq!(b[30].display_text, "MO1");
        assert_eq!(b[32].display_text, "L2\n⏎");
        // A #define stays a name: the preprocessor is not run
        assert_eq!(b[10].display_text, "HOME_A");
        assert_eq!(km.layers[1].bindings[15].display_text, "←");
        assert_eq!(km.layers[2].bindings[29].display_text, "BOOT");
    }

    #[test]
    fn keymap_c_without_designators_and_on_one_line() {
        let _g = plain();
        let c = "const uint16_t keymaps[][2][2] = { LAYOUT(KC_A, KC_B, KC_C, KC_D), \
                 LAYOUT(MO(1), KC_TRNS, KC_TRNS, KC_TRNS) };";
        let km = parse_c(c, None, None).unwrap();
        assert_eq!(km.layers.len(), 2);
        assert_eq!(km.layers[1].name, "Layer 1");
        assert_eq!(km.layers[1].bindings[0].display_text, "MO1");
        assert!(!km.layout.is_split);
    }

    #[test]
    fn keymap_c_without_keymaps_array_fails() {
        assert!(parse_c("int main(void) { return 0; }", None, None).is_err());
    }

    #[test]
    fn finds_the_keyboard_info_above_the_keymap() {
        let root = std::env::temp_dir().join(format!("peekaboard-qmk-{}", std::process::id()));
        let keymap_dir = root.join("keymaps").join("me");
        std::fs::create_dir_all(&keymap_dir).unwrap();
        std::fs::write(root.join("keyboard.json"), INFO).unwrap();
        let keymap = keymap_dir.join("keymap.json");
        std::fs::write(&keymap, tiny_json()).unwrap();

        let _g = plain();
        let km = crate::load_keymap(keymap.to_str().unwrap()).unwrap();
        assert_eq!(km.matrix.len(), 10);
        assert_eq!(km.layout.keys_per_row, vec![6, 4]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
