// The HUD: the Swift app's floating panel. A transparent always-on-top
// window with the layer tabs and the keyboard, scaled to fit. It fades after
// a timeout, wakes on layer changes, key presses and hover.

const hud = document.getElementById('hud');
const state = { inactive: false, hovering: false, timer: null, shownHud: false };

// macOS: the window itself fades and resizes (with its native blur);
// elsewhere the page does it
const nativeWindow = navigator.userAgent.includes('Mac') && !Bridge.mock;
if (nativeWindow) document.body.classList.add('mac');

// ---------- Fade ----------

// The window fades natively (see windows::fade_hud); asked once per change
let requestedAlpha = null;
function fadeWindowTo(target) {
  if (requestedAlpha !== null && Math.abs(requestedAlpha - target) < 0.005) return;
  requestedAlpha = target;
  Bridge.invoke('fade_hud', { alpha: target });
}

function applyFade() {
  const live = Store.live;
  const opacity = live ? live.settings.hudOpacity : 0.9;
  hud.classList.toggle('inactive', state.inactive);
  if (nativeWindow) {
    fadeWindowTo(state.inactive ? 0.15 : opacity);
  } else {
    hud.style.opacity = state.inactive ? '' : String(opacity);
  }
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

// Hover comes from the app, which watches the mouse: the page's own
// mouseleave does not arrive while another app is in front
function setHovering(inside) {
  if (inside === state.hovering) return;
  state.hovering = inside;
  if (inside) {
    clearTimeout(state.timer);
    setInactive(false);
  } else {
    wake();
  }
}
if (Bridge.mock) {
  hud.addEventListener('mouseenter', () => setHovering(true));
  hud.addEventListener('mouseleave', () => setHovering(false));
} else {
  Bridge.listen('hud-hover', (inside) => setHovering(!!inside));
}

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
    if (nativeWindow) resizeByHand(edge.dataset.dir, e);
    else Bridge.window().startResizeDragging(edge.dataset.dir);
  });
});

/** macOS can't drag the edges of a borderless window natively: follow the
 *  mouse and set the frame. Drags keep arriving outside the window. */
function resizeByHand(dir, down) {
  const start = { x: window.screenX, y: window.screenY, w: window.innerWidth, h: window.innerHeight };
  const [minW, minH] = [280, 140];
  let pending = null;
  const move = (e) => {
    const dx = e.screenX - down.screenX;
    const dy = e.screenY - down.screenY;
    let { x, y, w, h } = start;
    if (dir.includes('East')) w = start.w + dx;
    if (dir.includes('South')) h = start.h + dy;
    if (dir.includes('West')) {
      w = Math.max(minW, start.w - dx);
      x = start.x + (start.w - w);
    }
    if (dir.includes('North')) {
      h = Math.max(minH, start.h - dy);
      y = start.y + (start.h - h);
    }
    w = Math.max(minW, w);
    h = Math.max(minH, h);
    if (pending) return;
    pending = requestAnimationFrame(() => {
      pending = null;
      Bridge.invoke('set_hud_frame', { x, y, width: w, height: h });
    });
  };
  const up = () => {
    window.removeEventListener('mousemove', move, true);
    window.removeEventListener('mouseup', up, true);
  };
  window.addEventListener('mousemove', move, true);
  window.addEventListener('mouseup', up, true);
}

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
    delete tabs.dataset.drawn;
    delete board.dataset.drawn;
    board.style.display = 'none';
    empty.style.display = '';
  } else {
    empty.style.display = 'none';
    board.style.display = '';
    // Redrawn only when they change: a redraw under the mouse swallows the click
    const tabsKey = `${Store.keymapVersion}:${selected}`;
    if (tabs.dataset.drawn !== tabsKey) {
      tabs.dataset.drawn = tabsKey;
      tabs.innerHTML = '';
      keymap.layers.forEach((layer, index) => {
        const b = document.createElement('button');
        b.className = 'chip-tab' + (index === selected ? ' selected' : '');
        b.textContent = layer.name;
        b.onclick = () => Bridge.invoke('select_layer', { index });
        tabs.appendChild(b);
      });
    }

    const boardKey = `${tabsKey}:${live.pressed.join(',')}`;
    if (board.dataset.drawn !== boardKey) {
      board.dataset.drawn = boardKey;
      board.innerHTML = '';
      const current = keymap.layers[selected];
      if (current) {
        board.appendChild(KeyboardView.render(current, keymap.layout, { pressed: new Set(live.pressed) }));
        fitBoard();
      }
    }
  }

  document.body.classList.toggle('material', !!live.settings.hudUseMaterial);

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
