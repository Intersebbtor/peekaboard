# peekaboard

Peek at your keyboard: peekaboard shows your ZMK or QMK keymap as a 2D grid, with a floating
**HUD** and the **live layer** of your keyboard. For Windows and Linux (macOS builds too).

peekaboard is the cross-platform port of the
[ZMK Keymap Viewer](https://github.com/Intersebbtor/ZMK-keymap-viewer), a macOS menu bar app.
**On a Mac with ZMK, use the ZMK Keymap Viewer**: it is feature complete and has a few
extras that are not ported yet (see below).

> **Status: MVP / preview.** Built to be tried on Windows (ARM64 in a VM, x64 on a PC).
> Linux builds the same code but is untested. QMK support is built from the file formats and
> the KeyPeek protocol and has not met a real QMK keyboard yet: reports welcome.

## What works

Ported from the Swift app (v1.6.1), same behaviour and look:

- Parse a ZMK `.keymap` file: layers, bindings, labels (⇧ ⌃ ⌥ ⌘, ␣, ⌫ ...), layout detection
  (Sweep/Cradio, Corne, Sofle, Lily58, ... or "Custom"), split or not, thumb cluster
- Parse a QMK `keymap.json` or `keymap.c`, see [QMK](#qmk)
- Layer tabs, keyboard grid with the same key colours and sizes
- **Live reload** when the file changes on disk
- **HUD mode**: transparent, always on top, never takes the focus; draggable,
  resizable; fades after a timeout, wakes on layer changes, key presses and hover;
  opacity and timeout in the settings; global shortcut (default `Ctrl+Shift+K`)
- **Live layer** over Raw HID (usage page `0xFF60`, usage `0x61`, USB or Bluetooth):
  follows the active layer, highlights held keys, typing speed (WPM) in the HUD
- Names for keys (right-click), recent files, drag and drop of keymap files
- Tray icon with the live layer number; closing the window keeps the app in the tray
- Labels that show what a key types in the active keyboard layout (Windows only)

## What is not in yet

Compared to the macOS app:

- Reading the keymap from the keyboard over ZMK Studio (the file is always used)
- Setup window and firmware setup guide
- PDF export
- Auto-update (download the new exe by hand)
- Background blur of the HUD (a plain translucent panel instead)
- Vial `.vil` files, and reading the keymap from a QMK/Vial keyboard over VIA

## Try it without a keyboard

```
peekaboard --simulate --hud path\to\config\cradio.keymap
peekaboard --simulate examples/qmk/sweep-keymap.json
```

`--simulate` fakes a keyboard that changes layers and types; `--hud` opens the HUD.
`examples/qmk/sweep-keymap.json` is a QMK keymap to try it with.

## Build

Needs Rust (`rustup`). The UI is plain HTML/CSS/JS in `app/ui`, no Node build step.

```
cargo test --workspace            # core + app tests
cargo run -p peekaboard           # run on this machine
```

CI runs these on every push and pull request (Windows, Linux, macOS). Run them before you open a PR:

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

**Windows exe from a Mac** (no Microsoft SDK needed, uses
[llvm-mingw](https://github.com/mstorsjo/llvm-mingw) in `~/toolchains/llvm-mingw`):

```
rustup target add aarch64-pc-windows-gnullvm x86_64-pc-windows-gnullvm
scripts/build-windows.sh arm64    # Windows on ARM (e.g. UTM on Apple Silicon)
scripts/build-windows.sh x64      # normal Windows PC
```

The exe and `WebView2Loader.dll` land in `dist/windows-<arch>/`; keep the two together.
It needs the WebView2 runtime, which Windows 11 has built in.

## Layout

| Path | What |
|---|---|
| `crates/keymap-core` | Platform-independent core: ZMK parser (`parser.rs`, tests ported from the Swift app), QMK parser (`qmk.rs`), layout detection, live-layer packets, typing speed |
| `app/src` | Tauri app: state, HID thread, file watcher, tray, windows, commands |
| `app/ui` | The two windows (`index.html`, `hud.html`); `dev/mock-state.json` is for browser previews |

To look at the UI in a browser: serve `app/ui` (e.g. `python3 -m http.server`) and
open `index.html?sim=1` or `hud.html?sim=1`.

## Settings

`settings.json` in the app's config folder (on Windows `%APPDATA%\com.intersebbtor.peekaboard`).
The first start takes over the settings of earlier builds (`com.intersebbtor.zmk-keymap-viewer`).

## QMK

Open the `keymap.json` (QMK Configurator export or `qmk c2json`) or the `keymap.c` of your keymap.

- **Labels:** QMK keycodes get the same labels as in ZMK: `KC_SCLN` shows `;`, `LSFT_T(KC_A)`
  shows `⇧` over `A`, `LT(_NAV, KC_SPC)` shows `L1` over `␣`. Layer names come from the
  `[_NAV] = LAYOUT(...)` designators in `keymap.c`; `keymap.json` has none, so its layers are
  numbered. Custom keycodes and `#define`s show their name (the C preprocessor is not run).
- **Rows and split:** from the keyboard's `keyboard_info.json`, `keyboard.json` or `info.json`
  when one sits next to the keymap or up to five folders above it (as in a `qmk_firmware` or
  userspace checkout). Without one, `keymap.c` rows follow the line breaks of `LAYOUT(...)`
  and `keymap.json` rows follow the key count (rows of 10 up to 40 keys, else 12).
- **Live layer:** the [KeyPeek](https://github.com/srwi/keypeek) module `srwi/keypeek_layer_notify`
  in your firmware (KeyPeek's README has the steps: add
  [qmk-modules](https://github.com/srwi/qmk-modules), list the module in `keymap.json`,
  `RAW_ENABLE = yes` and `VIA_ENABLE = yes`). The app keeps the subscription the module
  needs alive while a QMK keymap is open.
- **Held keys:** QMK reports keys by switch matrix position. To light them up, the app needs
  the matrix from the keyboard's layout file. Export it next to your keymap:
  `qmk info -kb <keyboard> -m -f json > keyboard_info.json`. Without it, the layer still
  follows and the typing speed still counts.

`--hid-check --qmk` lists the HID devices and listens for ten seconds with the subscription on.

## Live layer needs the firmware modules

- **ZMK:** same as the macOS app: [zmk-raw-hid](https://github.com/zzeneg/zmk-raw-hid) and
  [zmk-keypeek-layer-notifier](https://github.com/srwi/zmk-keypeek-layer-notifier) in your firmware.
- **QMK:** `srwi/keypeek_layer_notify`, see [QMK](#qmk).

The packet format is the same for both (`crates/keymap-core/src/live.rs`). No code is taken
from KeyPeek (GPL-3.0); peekaboard only speaks the same protocol.

MIT license, same as the macOS app.
