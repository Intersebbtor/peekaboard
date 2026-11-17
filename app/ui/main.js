// The main window: the Swift app's menu bar popover.

const ICONS = {
  folder: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"><path d="M1.5 4.5a1 1 0 0 1 1-1h3.2l1.5 1.7h6.3a1 1 0 0 1 1 1v6.3a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z"/></svg>',
  clock: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.3 1.4"/></svg>',
  gear: '<svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor"><path d="M8 5.3a2.7 2.7 0 1 0 0 5.4 2.7 2.7 0 0 0 0-5.4zM6.9 1h2.2l.4 1.9 1.1.5 1.6-1.1 1.6 1.6-1.1 1.6.5 1.1 1.9.4v2.2l-1.9.4-.5 1.1 1.1 1.6-1.6 1.6-1.6-1.1-1.1.5-.4 1.9H6.9l-.4-1.9-1.1-.5-1.6 1.1-1.6-1.6 1.1-1.6-.5-1.1L1 9.1V6.9l1.9-.4.5-1.1-1.1-1.6 1.6-1.6 1.6 1.1 1.1-.5z" fill-rule="evenodd"/></svg>',
  pencil: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"><path d="M11 2.5l2.5 2.5L5.5 13H3v-2.5z"/></svg>',
  hud: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="2" y="3" width="12" height="9" rx="2"/><path d="M2 6h12"/></svg>',
  quit: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"><circle cx="8" cy="8" r="6"/><path d="M5.8 5.8l4.4 4.4M10.2 5.8l-4.4 4.4"/></svg>',
  check: '<svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 8.5l3.2 3L13 4.5"/></svg>',
  trash: '<svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5"/></svg>',
  keyboard: '<svg viewBox="0 0 72 44" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><g opacity="0.9"><rect x="3" y="6" width="26" height="32" rx="6"/><rect x="43" y="6" width="26" height="32" rx="6"/><path d="M9 14h4M16 14h4M9 22h4M16 22h4M9 30h4M16 30h4M49 14h4M56 14h4M49 22h4M56 22h4M49 30h4M56 30h4"/></g></svg>',
};

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const ui = {
  settingsOpen: false,
  recentOpen: false,
  recording: false,
  shortcutError: '',
};

// ---------- Helpers ----------

function fileName(path) {
  return path.split(/[\\/]/).pop();
}

/** Long paths shortened in the middle, like the Swift app's truncation */
function shortenPath(path) {
  const home = Store.keymap && Store.keymap.homeDir;
  if (home && (path === home || path.startsWith(home + '/') || path.startsWith(home + '\\'))) path = '~' + path.slice(home.length);
  return path.length > 72 ? path.slice(0, 30) + '…' + path.slice(-40) : path;
}

/** True when `value` differs from what was last drawn under `key`. Parts of
 *  the window are only redrawn when their content changes: a redraw under the
 *  mouse would swallow a click. */
const drawn = {};
function changed(key, value) {
  const signature = JSON.stringify(value);
  if (drawn[key] === signature) return false;
  drawn[key] = signature;
  return true;
}

/** Empties the content area; whatever was drawn there is drawn again */
function clearContent() {
  $('content').innerHTML = '';
  delete drawn.placeholder;
  delete drawn.tabs;
  delete drawn.board;
}

// ---------- Header ----------

function renderHeader() {
  const k = Store.keymap;
  const header = $('header');
  const divider = $('header-divider');
  const hasKeymap = k && k.keymap;
  const hidden = !hasKeymap && !ui.settingsOpen;
  const live = Store.live;
  const device = live && live.device;
  if (!changed('header', [hidden, Store.keymapVersion, ui.recentOpen, device, live && live.liveLayerName])) return;
  header.style.display = divider.style.display = hidden ? 'none' : '';
  if (hidden) return;

  const recent = (k && k.recentKeymaps) || [];

  let menu = '';
  if (ui.recentOpen && recent.length) {
    menu =
      '<div class="menu">' +
      recent
        .map(
          (p) =>
            `<button class="item" data-recent="${esc(p)}" title="${esc(p)}">${p === k.filePath ? ICONS.check : '<span style="width:12px"></span>'}${esc(fileName(p))}</button>`
        )
        .join('') +
      `<div class="sep"></div><button class="item danger" data-clear>${ICONS.trash} Clear History</button></div>`;
  }

  header.innerHTML = `
    <div class="header">
      <div class="row">
        <span class="title">Keymap</span>
        <span class="spacer"></span>
        <button class="btn small" data-choose>${ICONS.folder} Choose File…</button>
        ${recent.length ? `<div class="menu-wrap"><button class="btn small" data-recent-toggle>${ICONS.clock} Recent</button>${menu}</div>` : ''}
      </div>
      ${k && k.filePath ? `<div class="filepath" title="${esc(k.filePath)}">${esc(shortenPath(k.filePath))}</div>` : ''}
      <div class="status">
        ${k && k.filePath ? `<button class="link" data-editor>${ICONS.pencil} Open in Editor</button>` : ''}
        <span class="dot ${device ? 'on' : ''}"></span>
        <span>${device ? `${esc(device)}${live.liveLayerName ? ' · ' + esc(live.liveLayerName) : ''}` : 'No keyboard connected'}</span>
        <span class="spacer"></span>
        ${hasKeymap ? `<span class="chip">${esc(k.keymap.layout.name)}</span>` : ''}
      </div>
      ${k && k.error ? `<div class="err">${esc(k.error)}</div>` : ''}
    </div>`;

  header.querySelector('[data-choose]').onclick = () => Bridge.invoke('choose_file');
  const toggle = header.querySelector('[data-recent-toggle]');
  if (toggle) {
    toggle.onclick = (e) => {
      e.stopPropagation();
      ui.recentOpen = !ui.recentOpen;
      renderHeader();
    };
  }
  header.querySelectorAll('[data-recent]').forEach((b) => {
    b.onclick = () => {
      ui.recentOpen = false;
      Bridge.invoke('load_path', { path: b.dataset.recent });
    };
  });
  const clear = header.querySelector('[data-clear]');
  if (clear) {
    clear.onclick = () => {
      ui.recentOpen = false;
      Bridge.invoke('clear_recent');
    };
  }
  const editor = header.querySelector('[data-editor]');
  if (editor) editor.onclick = () => Bridge.invoke('open_in_editor');
}

// ---------- Body ----------

function renderBody() {
  const content = $('content');
  if (ui.settingsOpen) {
    if (!content.querySelector('.settings')) buildSettings(content);
    updateSettings();
    return;
  }

  const k = Store.keymap;
  const live = Store.live;
  if (!k || !k.keymap || !live) {
    if (!changed('placeholder', [Store.keymapVersion, ui.settingsOpen])) return;
    delete drawn.tabs;
    delete drawn.board;
    content.innerHTML = `
      <div class="placeholder">
        ${ICONS.keyboard}
        <h2>No keymap yet</h2>
        <p>Choose the <b>.keymap</b> file of your ZMK config (usually <code>config/&lt;keyboard&gt;.keymap</code>), or drop it on this window.</p>
        ${k && k.error ? `<p class="error">${esc(k.error)}</p>` : ''}
        <button class="btn prominent" data-choose>Choose File…</button>
      </div>`;
    content.querySelector('[data-choose]').onclick = () => Bridge.invoke('choose_file');
    return;
  }

  const { keymap } = k;
  const selected = live.selection.displayedIndex;

  // Tabs and board are two parts that change on their own
  if (!content.querySelector('#tabs-area')) {
    delete drawn.placeholder;
    delete drawn.tabs;
    delete drawn.board;
    content.innerHTML = '<div class="tabs" id="tabs-area"></div><div class="board-area" id="board"></div>';
  }

  if (changed('tabs', [Store.keymapVersion, selected])) {
    const tabRows = [];
    for (let i = 0; i < keymap.layers.length; i += 6) {
      tabRows.push(keymap.layers.slice(i, i + 6).map((layer, j) => ({ layer, index: i + j })));
    }
    $('tabs-area').innerHTML = tabRows
      .map(
        (row) =>
          `<div class="tabrow">${row
            .map(({ layer, index }) => `<button class="tab ${index === selected ? 'selected' : ''}" data-layer="${index}">${esc(layer.name)}</button>`)
            .join('')}</div>`
      )
      .join('');
    $('tabs-area').querySelectorAll('[data-layer]').forEach((b) => {
      b.onclick = () => Bridge.invoke('select_layer', { index: Number(b.dataset.layer) });
    });
  }

  if (changed('board', [Store.keymapVersion, selected, live.pressed])) {
    const board = $('board');
    board.innerHTML = '';
    const layer = keymap.layers[selected];
    if (layer) {
      board.appendChild(
        KeyboardView.render(layer, keymap.layout, {
          pressed: new Set(live.pressed),
          onContext: showKeyMenu,
        })
      );
    }
  }
}

// ---------- Footer ----------

function renderFooter() {
  const live = Store.live;
  const hudOn = live && live.settings.hudEnabled;
  const shortcut = live ? live.settings.shortcut : '';
  if (!changed('footer', [ui.settingsOpen, hudOn, shortcut, live && live.version])) return;
  $('footer').innerHTML = `
    <span class="version">v${live ? esc(live.version) : ''}</span>
    <span class="spacer"></span>
    <button class="link ${hudOn ? 'active' : ''}" data-hud title="Toggle HUD Mode (${esc(shortcut)})">${ICONS.hud} HUD Mode</button>
    <span class="vdivider"></span>
    <button class="link ${ui.settingsOpen ? 'active' : ''}" data-settings>${ICONS.gear} Settings</button>
    <span class="vdivider"></span>
    <button class="link" data-quit>${ICONS.quit} Quit</button>`;
  const f = $('footer');
  f.querySelector('[data-hud]').onclick = () => Bridge.invoke('toggle_hud');
  f.querySelector('[data-settings]').onclick = () => {
    ui.settingsOpen = !ui.settingsOpen;
    clearContent();
    renderAll();
  };
  // A fresh footer (e.g. the HUD button's colour) is drawn without losing the click handlers above
  f.querySelector('[data-quit]').onclick = () => Bridge.invoke('quit');
}

// ---------- Settings ----------

function buildSettings(content) {
  content.innerHTML = `
    <div class="settings">
      <div class="head"><h1>Settings</h1><button class="close" data-close title="Close">×</button></div>

      <section>
        <h3>Global Shortcut</h3>
        <div class="line">
          <span class="shortcut" id="shortcut"></span>
          <button class="btn small" id="record">Record</button>
        </div>
        <p class="note">This shortcut toggles the HUD visibility from anywhere.</p>
        <div class="err" id="shortcut-error"></div>
      </section>
      <div class="divider"></div>

      <section>
        <h3>Live Layer</h3>
        <div class="status"><span class="dot" id="live-dot"></span><span id="live-text"></span></div>
        <label class="toggle"><input type="checkbox" data-setting="followLiveLayer"> Follow the keyboard's active layer</label>
        <label class="toggle"><input type="checkbox" data-setting="showLayerInTray"> Show layer number in the tray icon</label>
        <p class="note">Needs the zmk-raw-hid and zmk-keypeek-layer-notifier firmware modules. Without a keyboard the app shows the keymap file.</p>
      </section>
      <div class="divider"></div>

      <section>
        <h3>HUD Appearance</h3>
        <div class="sliders">
          <div class="sl">
            <div class="top"><span>Opacity</span><span id="opacity-value"></span></div>
            <input type="range" min="0.1" max="1" step="0.01" data-slider="hudOpacity">
          </div>
          <div class="sl">
            <div class="top"><span>Auto-hide</span><span id="timeout-value"></span></div>
            <input type="range" min="0" max="10" step="0.5" data-slider="hudTimeout">
          </div>
        </div>
        ${navigator.userAgent.includes('Mac') ? '<label class="toggle"><input type="checkbox" data-setting="hudUseMaterial"> Use Background Blur (Material)</label>' : ''}
        <label class="toggle"><input type="checkbox" data-setting="showWpmInHud"> Show typing speed (WPM) in the HUD</label>
      </section>
    </div>`;

  content.querySelector('[data-close]').onclick = () => {
    ui.settingsOpen = false;
    stopRecording();
    clearContent();
    renderAll();
  };

  content.querySelectorAll('[data-setting]').forEach((box) => {
    box.onchange = () => Bridge.invoke('set_setting', { key: box.dataset.setting, value: box.checked });
  });
  content.querySelectorAll('[data-slider]').forEach((slider) => {
    slider.oninput = () => {
      const value = Number(slider.value);
      Bridge.invoke('set_setting', { key: slider.dataset.slider, value });
      updateSliderLabels(slider.dataset.slider, value);
    };
  });
  $('record').onclick = () => (ui.recording ? stopRecording() : startRecording());
}

function updateSliderLabels(key, value) {
  if (key === 'hudOpacity') $('opacity-value').textContent = Math.round(value * 100) + '%';
  if (key === 'hudTimeout') $('timeout-value').textContent = value === 0 ? 'Off' : value.toFixed(1) + 's';
}

/** Refreshes the values without rebuilding, so dragging a slider keeps working */
function updateSettings() {
  const live = Store.live;
  if (!live || !$('shortcut')) return;
  const s = live.settings;
  $('shortcut').textContent = ui.recording ? 'Press keys…' : s.shortcut;
  $('shortcut').classList.toggle('recording', ui.recording);
  $('record').textContent = ui.recording ? 'Cancel' : 'Record';
  $('shortcut-error').textContent = ui.shortcutError;

  $('live-dot').classList.toggle('on', !!live.device);
  $('live-text').textContent = live.device
    ? `${live.device} connected${live.liveLayerName ? ' · ' + live.liveLayerName : ''}`
    : 'No keyboard with layer notifier found';

  document.querySelectorAll('[data-setting]').forEach((box) => {
    box.checked = !!s[box.dataset.setting];
  });
  document.querySelectorAll('[data-slider]').forEach((slider) => {
    if (document.activeElement !== slider) slider.value = s[slider.dataset.slider];
    updateSliderLabels(slider.dataset.slider, Number(s[slider.dataset.slider]));
  });
}

// ---------- Shortcut recording ----------

const KEY_NAMES = {
  Space: 'Space', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Delete: 'Delete',
  ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
  Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Insert: 'Insert',
  Minus: '-', Equal: '=', Comma: ',', Period: '.', Slash: '/', Backslash: '\\',
  Semicolon: ';', Quote: "'", Backquote: '`', BracketLeft: '[', BracketRight: ']',
};

function keyNameOf(code) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return KEY_NAMES[code] || null;
}

function onRecordKey(e) {
  e.preventDefault();
  e.stopPropagation();
  if (e.code === 'Escape') return stopRecording();
  const key = keyNameOf(e.code);
  if (!key) return; // a modifier alone, or a key we don't know
  const mods = [];
  if (e.ctrlKey) mods.push('Ctrl');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');
  if (e.metaKey) mods.push('Super');
  if (!mods.length) {
    ui.shortcutError = 'Use at least one of Ctrl, Alt, Shift.';
    return updateSettings();
  }
  const accelerator = [...mods, key].join('+');
  stopRecording();
  Bridge.invoke('set_shortcut', { accelerator }).then(
    () => {
      ui.shortcutError = '';
      updateSettings();
    },
    (err) => {
      ui.shortcutError = String(err);
      updateSettings();
    }
  );
}

function startRecording() {
  ui.recording = true;
  ui.shortcutError = '';
  window.addEventListener('keydown', onRecordKey, true);
  updateSettings();
}

function stopRecording() {
  if (!ui.recording) return;
  ui.recording = false;
  window.removeEventListener('keydown', onRecordKey, true);
  updateSettings();
}

// ---------- Naming keys (right-click) ----------

function closeContextMenu() {
  document.querySelectorAll('.ctx').forEach((n) => n.remove());
}

function showKeyMenu(binding, event) {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'ctx';
  menu.style.left = Math.min(event.clientX, window.innerWidth - 190) + 'px';
  menu.style.top = Math.min(event.clientY, window.innerHeight - 70) + 'px';
  menu.innerHTML =
    '<button class="item" data-name>Name This Key…</button>' + (binding.name ? '<button class="item" data-remove>Remove Name</button>' : '');
  menu.querySelector('[data-name]').onclick = () => {
    closeContextMenu();
    askForKeyName(binding);
  };
  const remove = menu.querySelector('[data-remove]');
  if (remove) {
    remove.onclick = () => {
      closeContextMenu();
      Bridge.invoke('set_key_name', { displayText: binding.displayText, name: null });
    };
  }
  document.body.appendChild(menu);
}

function askForKeyName(binding) {
  const current = binding.displayText.replace(/\n/g, ' ');
  const root = $('overlay-root');
  root.innerHTML = `
    <div class="overlay" data-backdrop>
      <div class="dialog">
        <h2>Name this key</h2>
        <p>Shown instead of ${esc(current)} wherever that binding sits.</p>
        <input type="text" id="key-name" placeholder="e.g. Screenshot" value="${esc(binding.name || '')}">
        <div class="actions">
          ${binding.name ? '<button class="btn" data-remove>Remove Name</button>' : ''}
          <span class="spacer"></span>
          <button class="btn" data-cancel>Cancel</button>
          <button class="btn prominent" data-save>Save</button>
        </div>
      </div>
    </div>`;
  const input = $('key-name');
  const close = () => (root.innerHTML = '');
  const save = () => {
    Bridge.invoke('set_key_name', { displayText: binding.displayText, name: input.value });
    close();
  };
  root.querySelector('[data-save]').onclick = save;
  root.querySelector('[data-cancel]').onclick = close;
  const remove = root.querySelector('[data-remove]');
  if (remove) {
    remove.onclick = () => {
      Bridge.invoke('set_key_name', { displayText: binding.displayText, name: null });
      close();
    };
  }
  root.querySelector('[data-backdrop]').addEventListener('mousedown', (e) => {
    if (e.target.dataset.backdrop !== undefined) close();
  });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') save();
    if (e.key === 'Escape') close();
  });
  input.focus();
  input.select();
}

// ---------- Wiring ----------

function renderAll() {
  renderHeader();
  renderBody();
  renderFooter();
}

document.addEventListener('click', () => {
  closeContextMenu();
  if (ui.recentOpen) {
    ui.recentOpen = false;
    renderHeader();
  }
});

// The window never gets smaller than the keyboard, like the Swift window's
// minWidth/minHeight (ContentView.calculateOptimalWidth/Height)
let minSize = null;
function fitWindowToKeyboard() {
  const keymap = Store.keymap && Store.keymap.keymap;
  if (!keymap || Bridge.mock) return;
  const layout = keymap.layout;
  const maxKeys = Math.max(0, ...layout.keysPerRow) || 10;
  const first = layout.keysPerRow[0];
  const keyWidth = first !== undefined && first > 10 ? 48 : 54;
  const width = Math.max(700, maxKeys * keyWidth + (maxKeys - 1) * 3 + (layout.isSplit ? 30 : 0) + 40);
  const tabRows = Math.ceil(Math.max(1, keymap.layers.length) / 6);
  const tabs = 16 + tabRows * 26 + (tabRows - 1) * 4;
  const rows = layout.rowCount;
  const height = Math.max(320, 100 + tabs + rows * 44 + (rows - 1) * 3 + (layout.hasThumbCluster ? 20 : 0) + 40 + 50);
  const key = `${width}x${height}`;
  if (key === minSize) return;
  minSize = key;
  Bridge.invoke('set_main_min_size', { width, height });
}

Store.onChange(() => {
  if (!Store.keymap || !Store.live) return;
  renderAll();
  fitWindowToKeyboard();
});

// A .keymap file dropped on the window loads
Bridge.listen('tauri://drag-enter', () => document.body.classList.add('dropzone'));
Bridge.listen('tauri://drag-leave', () => document.body.classList.remove('dropzone'));
Bridge.listen('tauri://drag-drop', (payload) => {
  document.body.classList.remove('dropzone');
  const path = (payload.paths || []).find((p) => p.toLowerCase().endsWith('.keymap'));
  if (path) Bridge.invoke('load_path', { path });
});

Store.init();
