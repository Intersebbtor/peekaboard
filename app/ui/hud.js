// The HUD: the Swift app's floating panel. A transparent always-on-top
// window with the layer tabs and the keyboard, scaled to fit. It fades after
// a timeout, wakes on layer changes, key presses and hover.

const hud = document.getElementById('hud');
const state = { inactive: false, hovering: false, timer: null, shownHud: false };

// ---------- Fade ----------

function applyFade() {
  const live = Store.live;
  const opacity = live ? live.settings.hudOpacity : 0.9;
  hud.style.opacity = state.inactive ? '' : String(opacity);
  hud.classList.toggle('inactive', state.inactive);
}

function setInactive(value) {
  state.inactive = value;
  applyFade();
}

/** Wakes a faded HUD; it fades again after the timeout */
function wake() {
  if (state.hovering) return;
  setInactive(false);
  clearTimeout(state.timer);
  const live = Store.live;
  if (!live) return;
  const timeout = live.settings.hudTimeout;
  if (timeout <= 0) return;
  // Don't fade while a key is held, e.g. a layer key: you are probably
  // looking for a key on that layer
  if (live.pressed.length > 0) return;
  state.timer = setTimeout(() => setInactive(true), timeout * 1000);
}

hud.addEventListener('mouseenter', () => {
  state.hovering = true;
  clearTimeout(state.timer);
  setInactive(false);
});
hud.addEventListener('mouseleave', () => {
  state.hovering = false;
  wake();
});

// ---------- Moving and resizing ----------

// The whole background drags the window
hud.addEventListener('mousedown', (e) => {
  if (e.button !== 0 || e.target.closest('button')) return;
  Bridge.window().startDragging();
});

document.querySelectorAll('.edge').forEach((edge) => {
  edge.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    Bridge.window().startResizeDragging(edge.dataset.dir);
  });
});

// ---------- Drawing ----------

function fitBoard() {
  const km = Store.keymap && Store.keymap.keymap;
  const board = document.getElementById('board');
  const kb = board.firstElementChild;
  if (!km || !kb) return;
  const { width, height } = KeyboardView.naturalSize(km.layout);
  const scale = Math.min(board.clientWidth / width, board.clientHeight / height, 2.0);
  kb.style.width = width + 'px';
  kb.style.height = height + 'px';
  kb.style.transform = `translate(-50%, -50%) scale(${scale > 0 ? scale : 1})`;
}

function renderAll() {
  const live = Store.live;
  const k = Store.keymap;
  if (!live || !k) return;
  const keymap = k.keymap;

  const tabs = document.getElementById('tabs');
  const board = document.getElementById('board');
  const empty = document.getElementById('empty');
  const selected = live.selection.displayedIndex;

  if (!keymap) {
    tabs.innerHTML = '';
    board.style.display = 'none';
    empty.style.display = '';
  } else {
    empty.style.display = 'none';
    board.style.display = '';
    tabs.innerHTML = '';
    keymap.layers.forEach((layer, index) => {
      const b = document.createElement('button');
      b.className = 'chip-tab' + (index === selected ? ' selected' : '');
      b.textContent = layer.name;
      b.onclick = () => Bridge.invoke('select_layer', { index });
      tabs.appendChild(b);
    });

    board.innerHTML = '';
    const current = keymap.layers[selected];
    if (current) {
      board.appendChild(KeyboardView.render(current, keymap.layout, { pressed: new Set(live.pressed) }));
      fitBoard();
    }
  }

  // Typing speed in the corner
  const wpm = document.getElementById('wpm');
  const show = live.settings.showWpmInHud && !!live.device;
  wpm.style.display = show ? '' : 'none';
  if (show) {
    document.getElementById('wpm-num').textContent = String(live.wpm);
    document.getElementById('wpm-line').setAttribute('points', wpmGraphPoints(live.wpmHistory));
  }
  applyFade();
}

new ResizeObserver(fitBoard).observe(document.getElementById('board'));

// ---------- What wakes the HUD ----------

Store.onChange((what, before) => {
  if (!Store.keymap || !Store.live) return;
  renderAll();
  if (what !== 'live') return;

  const live = Store.live;
  if (!before) return wake();
  const layerChanged =
    before.selection.liveIndex !== live.selection.liveIndex || before.selection.displayedIndex !== live.selection.displayedIndex;
  const hudJustEnabled = !before.settings.hudEnabled && live.settings.hudEnabled;
  const wasPressed = new Set(before.pressed);
  const newPress = live.pressed.some((p) => !wasPressed.has(p));
  // Every key press keeps the HUD awake, not only layer changes; the timeout
  // starts once the last held key is released
  const releasedAll = before.pressed.length > 0 && live.pressed.length === 0;
  if (layerChanged || hudJustEnabled || newPress || releasedAll || before.settings.hudTimeout !== live.settings.hudTimeout) wake();
});

Store.init();
