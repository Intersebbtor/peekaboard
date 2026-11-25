# peekaboard

Peek at your keyboard: peekaboard shows your ZMK keymap as a 2D grid, with a floating
**HUD** and the **live layer** of your keyboard. For Windows and Linux, QMK support is planned.

peekaboard is the cross-platform port of the
[ZMK Keymap Viewer](https://github.com/Intersebbtor/ZMK-keymap-viewer), a macOS menu bar app.
**On a Mac with ZMK, use the ZMK Keymap Viewer**: it is feature complete and has a few
extras that are not ported yet (see below).

> **Status: MVP / preview.** Built to be tried on Windows (ARM64 in a VM, x64 on a PC).
> Linux builds the same code but is untested.

## What works

Ported from the Swift app (v1.6.1), same behaviour and look:

- Parse a `.keymap` file: layers, bindings, labels (⇧ ⌃ ⌥ ⌘, ␣, ⌫ ...), layout detection
  (Sweep/Cradio, Corne, Sofle, Lily58, ... or "Custom"), split or not, thumb cluster
- Layer tabs, keyboard grid with the same key colours and sizes
- **Live reload** when the file changes on disk
- **HUD mode**: transparent, always on top, never takes the focus; draggable,
  resizable; fades after a timeout, wakes on layer changes, key presses and hover;
  opacity and timeout in the settings; global shortcut (default `Ctrl+Shift+K`)
- **Live layer** over Raw HID (usage page `0xFF60`, usage `0x61`, USB or Bluetooth):
  follows the active layer, highlights held keys, typing speed (WPM) in the HUD
- Names for keys (right-click), recent files, drag and drop of `.keymap` files
- Tray icon with the live layer number; closing the window keeps the app in the tray
- Labels that show what a key types in the active keyboard layout (Windows only)

## What is not in yet

Compared to the macOS app:

- Reading the keymap from the keyboard over ZMK Studio (the file is always used)
- Setup window and firmware setup guide
- PDF export
- Auto-update (download the new exe by hand)
- Background blur of the HUD (a plain translucent panel instead)
- QMK / Vial (planned: the keyboard part is kept separate for that)

## Try it without a keyboard

```
zmk-keymap-viewer --simulate --hud path\to\config\cradio.keymap
```

`--simulate` fakes a keyboard that changes layers and types; `--hud` opens the HUD.

## Build

Needs Rust (`rustup`). The UI is plain HTML/CSS/JS in `app/ui`, no Node build step.

```
cargo test --workspace            # core + app tests
cargo run -p zmk-keymap-viewer    # run on this machine
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
| `crates/zmk-core` | Platform-independent core: parser, layout detection, live-layer packets, typing speed. 73 tests, ported from the Swift test suite. |
| `app/src` | Tauri app: state, HID thread, file watcher, tray, windows, commands |
| `app/ui` | The two windows (`index.html`, `hud.html`); `dev/mock-state.json` is for browser previews |

To look at the UI in a browser: serve `app/ui` (e.g. `python3 -m http.server`) and
open `index.html?sim=1` or `hud.html?sim=1`.

## Settings

`settings.json` in the app's config folder (on Windows `%APPDATA%\com.intersebbtor.zmk-keymap-viewer`).

## Live layer needs the firmware modules

Same as the macOS app: [zmk-raw-hid](https://github.com/zzeneg/zmk-raw-hid) and
[zmk-keypeek-layer-notifier](https://github.com/srwi/zmk-keypeek-layer-notifier) in your firmware.

MIT license, same as the macOS app.
