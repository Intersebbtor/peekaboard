//! ZMK `.keymap` parser. Port of `KeymapParser` from the Swift viewer.
//!
//! Same approach, same labels: strip comments, find the `keymap { }` block,
//! cut the layers out with a regex and tokenize the bindings line by line.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::key_chars::KeyCharacters;
use crate::layout::KeyboardLayout;

/// A single key binding with both display text and raw code
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBinding {
    /// Human-readable display (e.g. "Q", "⌃+Q")
    pub display_text: String,
    /// Original keymap code (e.g. "&long_MT RIGHT_CONTROL Q")
    pub raw_code: String,
    /// Name given with a right-click (e.g. "Screenshot" for ⇧⌘4)
    pub name: Option<String>,
    pub row: usize,
    pub column: usize,
}

impl KeyBinding {
    /// The name if present, otherwise the display text
    pub fn effective_display_text(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.display_text)
    }
}

/// A layer in the keymap. A key's ZMK position is its index in `bindings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeymapLayer {
    pub name: String,
    pub bindings: Vec<KeyBinding>,
    pub row_count: usize,
    pub column_count: usize,
}

/// Complete parsed keymap with all layers and layout info
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keymap {
    pub layers: Vec<KeymapLayer>,
    pub layout: KeyboardLayout,
    /// behavior name -> label
    pub behaviors: BTreeMap<String, String>,
    /// macro name -> label
    pub macros: BTreeMap<String, String>,
}

// MARK: - Regexes

static KEYMAP_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"keymap\s*\{").unwrap());
static BEHAVIORS_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"behaviors\s*\{").unwrap());
static MACROS_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"macros\s*\{").unwrap());
static LABELLED_NODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)(\w+):\s*\w+\s*\{[^}]*label\s*=\s*"([^"]+)""#).unwrap());
static LAYER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)(\w+)\s*\{([^}]*?)bindings\s*=\s*<(.*?)>\s*;").unwrap());
static LAYER_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:label|display-name)\s*=\s*"([^"]*)""#).unwrap());
static FIRST_BINDINGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)bindings\s*=\s*<(.*?)>").unwrap());
static MODIFIED_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(L[SCAG]|R[SCAG]|LC|RC|LA|RA|LG|RG|LS|RS)\((.+)\)").unwrap()
});

/// Spaces and tabs, like Foundation's `.whitespaces`
fn trim_ws(s: &str) -> &str {
    s.trim_matches(|c: char| c == '\t' || (c.is_whitespace() && c != '\n' && c != '\r'))
}

/// Parses a ZMK keymap file into a complete `Keymap`
pub fn parse(data: &str, file_path: Option<&str>) -> Option<Keymap> {
    // Windows files: \r\n would stick to the last token of every line
    let data = data.replace("\r\n", "\n");

    // Detect the keyboard from the raw content BEFORE removing comments
    let keyboard_info = KeyboardLayout::detect_keyboard_info(&data, file_path);
    let detected_name = keyboard_info.map(|i| i.0);

    let cleaned = remove_comments(&data);
    let behaviors = parse_behaviors(&cleaned);
    let macros = parse_macros(&cleaned);

    let range = find_keymap_section(&cleaned)?;
    let mut keymap_content = &cleaned[range];

    // Strip the outer "keymap { ... }" so "keymap" isn't matched as a layer
    if let (Some(first), Some(last)) = (keymap_content.find('{'), keymap_content.rfind('}')) {
        if first + 1 < last {
            keymap_content = &keymap_content[first + 1..last];
        }
    }

    let layers = parse_layers(keymap_content);
    if layers.is_empty() {
        return None;
    }

    // Known boards say whether they are split; otherwise look at the spacing
    let is_split = match keyboard_info {
        Some((_, split)) => split,
        None => detect_split_layout(keymap_content),
    };

    let first_layer = &layers[0];
    let keys_per_row = calculate_keys_per_row(&first_layer.bindings);
    let layout = KeyboardLayout::create(
        first_layer.bindings.len(),
        keys_per_row,
        detected_name,
        is_split,
    );

    Some(Keymap {
        layers,
        layout,
        behaviors,
        macros,
    })
}

/// Reads and parses a keymap file
pub fn load_keymap(path: &str) -> Result<Keymap, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Error reading file: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    // A UTF-8 byte order mark would sit in front of the first token
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    parse(text, Some(path)).ok_or_else(|| "Failed to parse keymap file".to_string())
}

/// Removes C-style comments (// and /* */). Newlines inside block comments stay.
fn remove_comments(data: &str) -> String {
    let chars: Vec<char> = data.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(data.len());
    let mut i = 0;

    while i < n {
        if i + 1 < n && chars[i] == '/' && chars[i + 1] == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            if i < n {
                out.push('\n');
                i += 1;
            }
            continue;
        }

        if i + 1 < n && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < n {
                if chars[i] == '*' && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Byte range of the `keymap { ... }` block, braces included
fn find_keymap_section(data: &str) -> Option<std::ops::Range<usize>> {
    let start = KEYMAP_START.find(data)?;
    let mut depth = 0i32;
    let mut found_first = false;
    let mut end = start.end();

    for (offset, c) in data[start.start()..].char_indices() {
        if c == '{' {
            depth += 1;
            found_first = true;
        } else if c == '}' {
            depth -= 1;
            if found_first && depth == 0 {
                end = start.start() + offset + 1;
                break;
            }
        }
    }
    Some(start.start()..end)
}

fn labelled_nodes(data: &str, from: usize) -> BTreeMap<String, String> {
    LABELLED_NODE
        .captures_iter(&data[from..])
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect()
}

fn parse_behaviors(data: &str) -> BTreeMap<String, String> {
    // Swift looks for the section header, then reads labels from the whole file
    if BEHAVIORS_START.find(data).is_none() {
        return BTreeMap::new();
    }
    labelled_nodes(data, 0)
}

fn parse_macros(data: &str) -> BTreeMap<String, String> {
    match MACROS_START.find(data) {
        Some(m) => labelled_nodes(data, m.start()),
        None => BTreeMap::new(),
    }
}

fn parse_layers(keymap_content: &str) -> Vec<KeymapLayer> {
    LAYER
        .captures_iter(keymap_content)
        .map(|caps| {
            let layer_id = &caps[1];
            let pre_bindings = &caps[2];
            let bindings_raw = &caps[3];

            let name = LAYER_LABEL
                .captures(pre_bindings)
                .map(|l| l[1].to_string())
                .unwrap_or_else(|| layer_id.to_string());

            let bindings = parse_bindings(bindings_raw);
            let keys_per_row = calculate_keys_per_row(&bindings);

            KeymapLayer {
                name,
                row_count: keys_per_row.len(),
                column_count: keys_per_row.iter().copied().max().unwrap_or(0),
                bindings,
            }
        })
        .collect()
}

/// Strips a C-style line comment (// ...) from a line
fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => trim_ws(&line[..i]),
        None => line,
    }
}

/// Parses the raw bindings content of a layer. Every line with bindings is one row.
pub fn parse_bindings(bindings_raw: &str) -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    let mut current_row = 0usize;

    for original_line in bindings_raw.split('\n').map(trim_ws) {
        let stripped = strip_line_comment(original_line);
        if stripped.is_empty() {
            continue;
        }

        let tokens = tokenize_bindings(stripped);
        if tokens.is_empty() {
            continue;
        }

        for (column, raw_code) in tokens.into_iter().enumerate() {
            bindings.push(KeyBinding {
                display_text: parse_display_text(&raw_code),
                raw_code,
                name: None,
                row: current_row,
                column,
            });
        }
        current_row += 1;
    }
    bindings
}

/// Tokenizes bindings; block comments between them are dropped
fn tokenize_bindings(input: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0i32;
    let mut in_block_comment = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    let push_trimmed = |tokens: &mut Vec<String>, current: &str| {
        let t = trim_ws(current);
        if !t.is_empty() {
            tokens.push(t.to_string());
        }
    };

    while i < chars.len() {
        let c = chars[i];

        if !in_block_comment && i + 1 < chars.len() && c == '/' && chars[i + 1] == '*' {
            in_block_comment = true;
            i += 2;
            continue;
        }
        if in_block_comment && i + 1 < chars.len() && c == '*' && chars[i + 1] == '/' {
            in_block_comment = false;
            i += 2;
            continue;
        }
        if in_block_comment {
            i += 1;
            continue;
        }

        if c == '&' {
            push_trimmed(&mut tokens, &current);
            current = "&".to_string();
        } else if c == '(' {
            paren_depth += 1;
            current.push(c);
        } else if c == ')' {
            paren_depth -= 1;
            current.push(c);
        } else if c == ' ' && paren_depth == 0 && !current.is_empty() {
            // A space outside parentheses ends the token if the next
            // non-space is the start of a new binding
            let mut next = i + 1;
            while next < chars.len() && chars[next] == ' ' {
                next += 1;
            }
            if next < chars.len() && chars[next] == '&' {
                push_trimmed(&mut tokens, &current);
                current.clear();
            } else {
                current.push(c);
            }
        } else {
            current.push(c);
        }
        i += 1;
    }

    push_trimmed(&mut tokens, &current);
    tokens
}

fn calculate_keys_per_row(bindings: &[KeyBinding]) -> Vec<usize> {
    let Some(max_row) = bindings.iter().map(|b| b.row).max() else {
        return vec![0];
    };
    let mut counts = vec![0usize; max_row + 1];
    for b in bindings {
        counts[b.row] += 1;
    }
    counts
}

/// Split keyboards have a large gap in the MIDDLE of each row (symmetric
/// split); non-split ones have gaps spread out for alignment.
fn detect_split_layout(keymap_content: &str) -> bool {
    let Some(caps) = FIRST_BINDINGS.captures(keymap_content) else {
        return true; // most ZMK keyboards are split
    };
    let bindings_raw = &caps[1];

    let lines: Vec<Vec<char>> = bindings_raw
        .split('\n')
        .filter(|l| {
            let t = trim_ws(l);
            !t.is_empty() && t.contains('&')
        })
        .map(|l| l.chars().collect())
        .collect();

    if lines.is_empty() {
        return true;
    }

    let mut lines_with_middle_gap = 0usize;
    let mut total_binding_lines = 0usize;

    for line in &lines {
        let positions: Vec<usize> = line
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == '&')
            .map(|(i, _)| i)
            .collect();
        // Too few keys to see a pattern
        if positions.len() < 6 {
            continue;
        }
        total_binding_lines += 1;

        let middle = positions.len() / 2;
        let left = positions[middle - 1];
        let right = positions[middle];

        // The segment between the two middle bindings has to contain a space
        if !line[left..right].contains(&' ') {
            continue;
        }

        let other_gaps: Vec<usize> = (1..positions.len())
            .filter(|&i| i != middle)
            .map(|i| positions[i] - positions[i - 1])
            .collect();
        let middle_gap = right - left;
        let avg_other = if other_gaps.is_empty() {
            0
        } else {
            other_gaps.iter().sum::<usize>() / other_gaps.len()
        };

        // Split: the middle gap is clearly larger than the average (1.5x)
        if avg_other > 0 && middle_gap > avg_other + avg_other / 2 {
            lines_with_middle_gap += 1;
        }
    }

    if total_binding_lines > 0 {
        return lines_with_middle_gap as f64 / total_binding_lines as f64 > 0.5;
    }
    true
}

// MARK: - Display text

fn parse_display_text(raw_code: &str) -> String {
    let code = raw_code.strip_prefix('&').unwrap_or(raw_code);
    let parts: Vec<&str> = code.split(' ').filter(|p| !p.is_empty()).collect();
    let Some(&behavior) = parts.first() else {
        return raw_code.to_string();
    };

    match behavior {
        "kp" => {
            if parts.len() > 1 {
                return format_key(parts[1]);
            }
        }
        "mt" => {
            if parts.len() > 2 {
                return format!("{}\n{}", format_modifier(parts[1]), format_key(parts[2]));
            }
        }
        "lt" => {
            if parts.len() > 2 {
                return format!("L{}\n{}", parts[1], format_key(parts[2]));
            }
        }
        "mo" => {
            if parts.len() > 1 {
                return format!("MO{}", parts[1]);
            }
        }
        "tog" => {
            if parts.len() > 1 {
                return format!("TG{}", parts[1]);
            }
        }
        "trans" => return "▽".to_string(),
        "none" => return "✕".to_string(),
        "bt" => {
            if parts.len() > 1 {
                let action = parts[1];
                if action == "BT_SEL" && parts.len() > 2 {
                    return format!("BT{}", parts[2]);
                } else if action == "BT_CLR" {
                    return "BT CLR".to_string();
                }
                return action.replace("BT_", "");
            }
        }
        "sys_reset" => return "RESET".to_string(),
        "bootloader" => return "BOOT".to_string(),
        "studio_unlock" => return "STUDIO".to_string(),
        _ => {
            // Custom behaviors like long_MT, short_LT
            if behavior.contains("MT") || behavior.contains("mt") {
                if parts.len() > 2 {
                    return format!("{}\n{}", format_modifier(parts[1]), format_key(parts[2]));
                }
            }
            if behavior.contains("LT") || behavior.contains("lt") {
                if parts.len() > 2 {
                    return format!("L{}\n{}", parts[1], format_key(parts[2]));
                }
            }
            // Macros and other custom behaviors
            if parts.len() == 1 {
                return behavior.to_uppercase();
            }
            if let Some(last) = parts.last() {
                return format_key(last);
            }
        }
    }
    raw_code.to_string()
}

const SPECIAL_KEYS: &[(&str, &str)] = &[
    ("BACKSPACE", "⌫"), ("BSPC", "⌫"),
    ("SPACE", "␣"), ("SPC", "␣"),
    ("TAB", "⇥"),
    ("RETURN", "⏎"), ("RET", "⏎"), ("ENTER", "⏎"),
    ("ESCAPE", "ESC"), ("ESC", "ESC"),
    ("DELETE", "DEL"), ("DEL", "DEL"),
    ("LEFT", "←"), ("RIGHT", "→"), ("UP", "↑"), ("DOWN", "↓"),
    ("SEMICOLON", ";"), ("SEMI", ";"),
    ("COMMA", ","),
    ("DOT", "."), ("PERIOD", "."),
    ("SLASH", "/"), ("FSLH", "/"),
    ("BACKSLASH", "\\"), ("BSLH", "\\"),
    ("MINUS", "-"), ("EQUAL", "="), ("PLUS", "+"),
    ("LBKT", "["), ("LEFT_BRACKET", "["),
    ("RBKT", "]"), ("RIGHT_BRACKET", "]"),
    ("LBRC", "{"), ("RBRC", "}"),
    ("LPAR", "("), ("RPAR", ")"),
    ("SQT", "'"), ("SINGLE_QUOTE", "'"),
    ("DQT", "\""), ("DOUBLE_QUOTES", "\""),
    ("GRAVE", "`"), ("TILDE", "~"),
    ("EXCLAMATION", "!"), ("EXCL", "!"),
    ("AT_SIGN", "@"), ("AT", "@"),
    ("HASH", "#"), ("POUND", "#"),
    ("DLLR", "$"), ("DOLLAR", "$"),
    ("PRCNT", "%"), ("PERCENT", "%"),
    ("CARET", "^"),
    ("AMPS", "&"), ("AMPERSAND", "&"),
    ("STAR", "*"), ("ASTRK", "*"), ("MULTIPLY", "*"),
    ("VOLUME_UP", "VOL+"), ("VOLUME_DOWN", "VOL-"), ("MUTE", "MUTE"),
    ("UNDER", "_"), ("UNDERSCORE", "_"),
    ("PIPE", "|"), ("COLON", ":"),
    ("LT", "<"), ("LESS_THAN", "<"),
    ("GT", ">"), ("GREATER_THAN", ">"),
    ("QMARK", "?"), ("QUESTION", "?"),
    ("C_VOL_UP", "VOL+"), ("C_VOL_DN", "VOL-"), ("C_MUTE", "MUTE"),
    ("C_PP", "⏯"), ("C_NEXT", "⏭"), ("C_PREV", "⏮"),
    ("C_BRI_UP", "☀+"), ("C_BRI_DN", "☀-"),
    ("N0", "0"), ("N1", "1"), ("N2", "2"), ("N3", "3"), ("N4", "4"),
    ("N5", "5"), ("N6", "6"), ("N7", "7"), ("N8", "8"), ("N9", "9"),
];

fn format_key(key: &str) -> String {
    // What the key types in the active layout: € for LS(LA(N2)), Ö for SEMI
    // in the German layout
    if let Some(typed) = KeyCharacters::typed_character(key) {
        return typed;
    }

    let formatted = key
        .replace("NUMBER_", "")
        .replace("KP_", "")
        .replace("K_", "");

    let upper = formatted.to_uppercase();
    if let Some((_, special)) = SPECIAL_KEYS.iter().find(|(name, _)| *name == upper) {
        return special.to_string();
    }

    // Modifier combos like LS(LG(NUMBER_4))
    if formatted.contains('(') {
        return parse_modified_key(&formatted);
    }

    upper
}

fn parse_modified_key(key: &str) -> String {
    let mut modifiers: Vec<String> = Vec::new();
    let mut current = key.to_string();

    while let Some(caps) = MODIFIED_KEY.captures(&current) {
        modifiers.push(format_modifier(&caps[1]));
        let inner = caps[2].to_string();
        current = inner;
    }

    let base = format_key(&current);
    if modifiers.is_empty() {
        return base;
    }
    format!("{}+{}", modifiers.join("+"), base)
}

fn format_modifier(modifier: &str) -> String {
    let symbol = match modifier.to_uppercase().as_str() {
        "LEFT_SHIFT" | "LSHIFT" | "LSHFT" | "LS" | "RIGHT_SHIFT" | "RSHIFT" | "RSHFT" | "RS" => "⇧",
        "LEFT_CONTROL" | "LCTRL" | "LC" | "RIGHT_CONTROL" | "RCTRL" | "RC" => "⌃",
        "LEFT_ALT" | "LALT" | "LA" | "RIGHT_ALT" | "RALT" | "RA" => "⌥",
        "LEFT_GUI" | "LGUI" | "LG" | "RIGHT_GUI" | "RGUI" | "RG" => "⌘",
        "LEFT_META" | "LMETA" | "RIGHT_META" | "RMETA" => "◆",
        _ => return modifier.to_string(),
    };
    symbol.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_chars::test_support::TABLE_LOCK;

    fn keymap(layers: &str) -> String {
        format!(
            r#"
/ {{
    keymap {{
        compatible = "zmk,keymap";
        {layers}
    }};
}};
"#
        )
    }

    /// One layer without a label
    fn one_layer(bindings: &str) -> String {
        keymap(&format!("default_layer {{ bindings = < {bindings} >; }};"))
    }

    fn display(input: &str) -> Vec<String> {
        parse_bindings(input).into_iter().map(|b| b.display_text).collect()
    }

    fn first_layer_display(bindings: &str) -> Vec<String> {
        let km = parse(&one_layer(bindings), None).expect("parses");
        km.layers[0].bindings.iter().map(|b| b.display_text.clone()).collect()
    }

    // MARK: Basic keymap

    #[test]
    fn parse_simple_keymap() {
        let data = r#"
/ {
    keymap {
        compatible = "zmk,keymap";

        default_layer {
            label = "QWERTY";
            bindings = <
                &kp Q &kp W &kp E
                &kp A &kp S &kp D
            >;
        };
    };
};
"#;
        let km = parse(data, None).unwrap();
        assert_eq!(km.layers.len(), 1);
        assert_eq!(km.layers[0].name, "QWERTY");
        assert_eq!(km.layers[0].bindings.len(), 6);
    }

    #[test]
    fn parse_multiple_layers() {
        let data = keymap(
            r#"
            default_layer { label = "QWERTY"; bindings = < &kp Q &kp W &kp E >; };
            symbol_layer { label = "SYMBOLS"; bindings = < &kp EXCL &kp AT &kp HASH >; };
            "#,
        );
        let km = parse(&data, None).unwrap();
        assert_eq!(km.layers.len(), 2);
        assert_eq!(km.layers[0].name, "QWERTY");
        assert_eq!(km.layers[1].name, "SYMBOLS");
    }

    #[test]
    fn layer_without_label_uses_node_name() {
        let km = parse(&one_layer("&kp Q"), None).unwrap();
        assert_eq!(km.layers[0].name, "default_layer");
    }

    #[test]
    fn parse_key_bindings() {
        assert_eq!(first_layer_display("&kp Q &kp W &kp E\n&kp A &kp S &kp D")[..3], ["Q", "W", "E"]);
    }

    #[test]
    fn parse_mod_tap_bindings() {
        let d = first_layer_display("&mt LEFT_SHIFT A &mt LEFT_CONTROL S");
        assert_eq!(d.len(), 2);
        assert!(d[0].contains('⇧') && d[0].contains('A'));
        assert!(d[1].contains('⌃') && d[1].contains('S'));
    }

    #[test]
    fn parse_layer_tap_bindings() {
        let d = first_layer_display("&lt 1 SPACE");
        assert_eq!(d.len(), 1);
        assert!(d[0].contains("L1") && d[0].contains('␣'));
    }

    #[test]
    fn parse_momentary_and_toggle_layer_bindings() {
        assert_eq!(first_layer_display("&mo 1 &mo 2"), ["MO1", "MO2"]);
        assert_eq!(first_layer_display("&tog 1"), ["TG1"]);
    }

    #[test]
    fn parse_transparent_and_none_bindings() {
        assert_eq!(first_layer_display("&trans &none"), ["▽", "✕"]);
    }

    #[test]
    fn format_special_keys() {
        let d = first_layer_display("&kp SPACE &kp TAB &kp BACKSPACE &kp RETURN\n&kp LEFT &kp RIGHT &kp UP &kp DOWN");
        assert_eq!(d, ["␣", "⇥", "⌫", "⏎", "←", "→", "↑", "↓"]);
    }

    #[test]
    fn format_modifiers() {
        let d = first_layer_display("&mt LEFT_SHIFT A &mt LEFT_CONTROL B &mt LEFT_ALT C &mt LEFT_GUI D");
        assert!(d[0].contains('⇧'));
        assert!(d[1].contains('⌃'));
        assert!(d[2].contains('⌥'));
        assert!(d[3].contains('⌘'));
    }

    // MARK: Layout detection

    #[test]
    fn detect_sweep_layout() {
        let data = one_layer(
            "&kp Q &kp W &kp E &kp R &kp T &kp Y &kp U &kp I &kp O &kp P
             &kp A &kp S &kp D &kp F &kp G &kp H &kp J &kp K &kp L &kp SEMICOLON
             &kp Z &kp X &kp C &kp V &kp B &kp N &kp M &kp COMMA &kp DOT &kp SLASH
             &mo 1 &kp SPACE &kp SPACE &mo 1",
        );
        let km = parse(&data, Some("/config/cradio.keymap")).unwrap();
        assert_eq!(km.layout.total_keys, 34);
        assert_eq!(km.layout.name, "Sweep/Cradio");
        assert_eq!(km.layout.keys_per_row, vec![10, 10, 10, 4]);
    }

    #[test]
    fn detect_corne_layout() {
        let data = one_layer(
            "&kp Q &kp W &kp E &kp R &kp T &kp Y &kp U &kp I &kp O &kp P &kp LBKT &kp RBKT
             &kp A &kp S &kp D &kp F &kp G &kp H &kp J &kp K &kp L &kp SEMICOLON &kp APOSTROPHE &kp ENTER
             &kp Z &kp X &kp C &kp V &kp B &kp N &kp M &kp COMMA &kp DOT &kp SLASH &kp SLASH &kp RSHIFT
             &kp LCTRL &kp LALT &kp LGUI &kp SPACE &kp BSPC &kp DEL",
        );
        let km = parse(&data, Some("/config/corne.keymap")).unwrap();
        assert_eq!(km.layout.total_keys, 42);
        assert_eq!(km.layout.name, "Corne");
    }

    #[test]
    fn unknown_board_gets_custom_name() {
        let km = parse(&one_layer("&kp Q &kp W"), Some("my.keymap")).unwrap();
        assert_eq!(km.layout.name, "Custom (2 keys)");
    }

    #[test]
    fn spacing_decides_split_for_unknown_boards() {
        let split = one_layer(
            "&kp Q &kp W &kp E &kp R &kp T           &kp Y &kp U &kp I &kp O &kp P
             &kp A &kp S &kp D &kp F &kp G           &kp H &kp J &kp K &kp L &kp SEMI",
        );
        assert!(parse(&split, None).unwrap().layout.is_split);

        let flat = one_layer(
            "&kp Q &kp W &kp E &kp R &kp T &kp Y &kp U &kp I &kp O &kp P
             &kp A &kp S &kp D &kp F &kp G &kp H &kp J &kp K &kp L &kp SEMI",
        );
        assert!(!parse(&flat, None).unwrap().layout.is_split);
    }

    // MARK: Rows and columns

    #[test]
    fn calculate_rows_and_columns() {
        let km = parse(&one_layer("&kp Q &kp W &kp E\n&kp A &kp S &kp D"), None).unwrap();
        assert_eq!(km.layers[0].row_count, 2);
        assert_eq!(km.layers[0].column_count, 3);
    }

    #[test]
    fn uniform_rows_and_columns() {
        let km = parse(&one_layer("&kp Q &kp W &kp E &kp R\n&kp A &kp S &kp D &kp F\n&kp Z &kp X &kp C &kp V"), None).unwrap();
        assert_eq!(km.layers[0].row_count, 3);
        assert_eq!(km.layers[0].column_count, 4);
    }

    #[test]
    fn key_position_is_index_in_source_order() {
        let b = parse_bindings("&kp Q &kp W &kp E\n&kp A &kp S");
        assert_eq!(b[3].raw_code, "&kp A");
        assert_eq!(b[3].row, 1);
        assert_eq!(b[3].column, 0);
    }

    // MARK: Edge cases

    #[test]
    fn empty_keymap_fails() {
        let data = r#"/ { keymap { compatible = "zmk,keymap"; }; };"#;
        assert!(parse(data, None).is_none());
    }

    #[test]
    fn missing_keymap_section_fails() {
        let data = r#"/ { behaviors { compatible = "zmk,behaviors"; }; };"#;
        assert!(parse(data, None).is_none());
    }

    #[test]
    fn whitespace_and_comments() {
        let data = r#"
/ {
    keymap {
        compatible = "zmk,keymap";

        // This is a comment
        default_layer {
            label = "Default";
            /* Multi-line
               comment */
            bindings = <
                &kp Q &kp W
            >;
        };
    };
};
"#;
        assert_eq!(parse(data, None).unwrap().layers[0].bindings.len(), 2);
    }

    #[test]
    fn inline_block_comments() {
        let data = one_layer("&kp Q /* Q key */ &kp W /* W key */\n&kp A &kp S /* home row */");
        let km = parse(&data, None).unwrap();
        assert_eq!(km.layers[0].bindings.len(), 4);
        assert_eq!(km.layers[0].bindings[0].display_text, "Q");
        assert_eq!(km.layers[0].bindings[1].display_text, "W");
    }

    #[test]
    fn cpp_style_comments() {
        let data = r#"
/ {
    keymap {
        compatible = "zmk,keymap";
        default_layer {
            label = "Default"; // This is the default layer
            bindings = < // bindings start
                &kp Q &kp W // Q and W keys
                &kp A &kp S // A and S keys
            >;
        };
    };
};
"#;
        assert_eq!(parse(data, None).unwrap().layers[0].bindings.len(), 4);
    }

    #[test]
    fn nested_comment_syntax_still_parses() {
        let data = r#"
/ {
    keymap {
        compatible = "zmk,keymap";
        default_layer {
            label = "Default";
            /* This is a /* nested */ comment test */
            bindings = <
                &kp Q &kp W
            >;
        };
    };
};
"#;
        assert!(!parse(data, None).unwrap().layers[0].bindings.is_empty());
    }

    #[test]
    fn comments_between_bindings() {
        let data = one_layer("/* Row 1 */ &kp Q &kp W &kp E\n/* Row 2 */ &kp A &kp S &kp D");
        assert_eq!(parse(&data, None).unwrap().layers[0].bindings.len(), 6);
    }

    #[test]
    fn custom_mod_tap_bindings() {
        let d = first_layer_display("&long_MT LEFT_CONTROL A");
        assert_eq!(d.len(), 1);
        assert!(d[0].contains('⌃'));
    }

    #[test]
    fn bluetooth_bindings() {
        assert_eq!(first_layer_display("&bt BT_SEL 0 &bt BT_CLR"), ["BT0", "BT CLR"]);
    }

    #[test]
    fn system_bindings() {
        assert_eq!(first_layer_display("&sys_reset &bootloader"), ["RESET", "BOOT"]);
    }

    #[test]
    fn parse_behaviors() {
        let data = r#"
/ {
    behaviors {
        long_MT: behavior_mod_tap {
            label = "LONG_MOD_TAP";
        };
        short_LT: behavior_layer_tap {
            label = "SHORT_LAYER_TAP";
        };
    };

    keymap {
        compatible = "zmk,keymap";
        default_layer {
            bindings = <
                &kp Q
            >;
        };
    };
};
"#;
        let km = parse(data, None).unwrap();
        assert_eq!(km.behaviors.len(), 2);
        assert_eq!(km.behaviors["long_MT"], "LONG_MOD_TAP");
        assert_eq!(km.behaviors["short_LT"], "SHORT_LAYER_TAP");
    }

    /// Names come from a right-click only; old /* =name */ comments are plain comments
    #[test]
    fn name_comments_are_ignored() {
        let b = parse_bindings("&kp LS(LG(N4)) /* =Screenshot */ &kp Q");
        let raw: Vec<&str> = b.iter().map(|b| b.raw_code.as_str()).collect();
        assert_eq!(raw, ["&kp LS(LG(N4))", "&kp Q"]);
        assert!(b.iter().all(|b| b.name.is_none()));
    }

    #[test]
    fn name_comments_in_a_file_are_ignored() {
        let km = parse(&one_layer("&kp LC(C) /* =Copy */ &kp Q"), None).unwrap();
        let b = &km.layers[0].bindings;
        assert_eq!(b[0].raw_code, "&kp LC(C)");
        assert_eq!(b[1].raw_code, "&kp Q");
        assert_eq!(b[0].effective_display_text(), "⌃+C");
    }

    // MARK: Regression tests from the Swift suite

    #[test]
    fn code_layer_parses_all_36_keys() {
        let input = r#"
                &kp N1           &kp N2     &kp N3     &kp N4    &kp N5       &kp N6  &kp N7    &kp N8            &kp N9             &kp N0
                &hml LALT GRAVE  &kp LSHFT  &kp LCTRL  &kp LCMD  &none        &none   &kp RCMD  &hmr RCTRL SEMI   &hmr RSHFT MINUS   &hmr RALT EQUAL
                &none            &none      &none      &none     &kp CAPS     &none   &none     &kp LEFT_BRACKET  &kp RIGHT_BRACKET  &kp BACKSLASH
                                    &trans      &trans    &trans     &trans       &trans  &trans
        "#;
        let d = display(input);
        assert!(d.contains(&"1".to_string()));
        assert!(d.contains(&"RCMD".to_string()));
        assert!(d.contains(&"[".to_string()));
        assert!(d.contains(&"▽".to_string()));
        assert_eq!(d.len(), 36);
    }

    #[test]
    fn first_line_is_not_skipped() {
        assert_eq!(display("&kp N1 &kp N2 &kp N3\n&kp N4 &kp N5 &kp N6"), ["1", "2", "3", "4", "5", "6"]);
        assert_eq!(display("// comment\n&kp N1 &kp N2 &kp N3\n&kp N4 &kp N5 &kp N6"), ["1", "2", "3", "4", "5", "6"]);
    }

    #[test]
    fn comment_lines_are_stripped() {
        assert_eq!(display("&kp Q &kp W &kp E\n// col 0 // col 1 // col 2\n&kp A &kp S &kp D"), ["Q", "W", "E", "A", "S", "D"]);
        let real = "                // col 0       // col 1      // col 2        // col 3\n&kp Q &kp W &kp F";
        assert_eq!(display(real), ["Q", "W", "F"]);
    }

    // MARK: Typed characters (KeyCharacters)

    #[test]
    fn labels_use_the_layout_when_loaded() {
        use crate::key_chars::Combo;
        let _g = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        KeyCharacters::set_table(
            [
                (Combo { usage: 0x1F, shift: true, option: true }, "€".to_string()),
                (Combo { usage: 0x33, shift: false, option: false }, "ö".to_string()),
            ]
            .into_iter()
            .collect(),
        );
        let b = parse_bindings("&kp LS(LA(N2)) &kp LG(Q) &mt LSHFT SEMI");
        assert_eq!(b[0].display_text, "€");
        assert_eq!(b[1].display_text, "⌘+Q");
        assert!(b[2].display_text.ends_with('Ö'));
        KeyCharacters::clear();
    }

    #[test]
    fn without_layout_labels_stay_as_before() {
        let _g = TABLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        KeyCharacters::clear();
        assert_eq!(parse_bindings("&kp LS(LA(N2))")[0].display_text, "⇧+⌥+2");
    }

    // MARK: Windows files

    #[test]
    fn crlf_files_parse_like_lf_files() {
        let lf = one_layer("&kp Q &kp W\n&kp A &kp S");
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(parse(&lf, None), parse(&crlf, None));
        let b = &parse(&crlf, None).unwrap().layers[0].bindings;
        assert_eq!(b[1].raw_code, "&kp W");
    }
}
