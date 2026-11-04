// Talks to the Rust side through Tauri. In a plain browser (no Tauri) it
// falls back to a small mock, so the UI can be developed and looked at with
// any static file server: open index.html or hud.html, add ?sim=1 for a fake
// keyboard.

const Bridge = (() => {
  const tauri = window.__TAURI__;
  if (tauri) {
    return {
      mock: false,
      invoke: (cmd, args) => tauri.core.invoke(cmd, args),
      listen: (event, handler) => tauri.event.listen(event, (e) => handler(e.payload)),
      window: () => tauri.window.getCurrentWindow(),
    };
  }

  // ---------- Browser mock ----------
  const listeners = {};
  let state = null;

  const emit = (event, payload) => (listeners[event] || []).forEach((h) => h(payload));
  const emitAll = () => {
    emit('keymap', state.keymap);
    emit('live', state.live);
  };

  const ready = fetch('dev/mock-state.json')
    .then((r) => r.json())
    .then((s) => {
      state = s;
      if (new URLSearchParams(location.search).has('sim')) startSimulation();
      return s;
    });

  function startSimulation() {
    const live = state.live;
    live.device = 'Simulated keyboard';
    live.selection.liveIndex = 0;
    let tick = 0;
    let held = null;
    setInterval(() => {
      tick++;
      const layers = state.keymap.keymap.layers.length;
      if (tick % 18 === 0) {
        const layer = Math.floor(tick / 18) % layers;
        live.selection.liveIndex = layer;
        if (live.settings.followLiveLayer) live.selection.displayedIndex = layer;
        live.liveLayerName = state.keymap.keymap.layers[layer].name;
      }
      if (held !== null) {
        live.pressed = live.pressed.filter((p) => p !== held);
        held = null;
      } else {
        held = (tick * 7) % state.keymap.keymap.layers[0].bindings.length;
        live.pressed = [held];
        live.wpm = 40 + ((tick * 13) % 50);
        live.wpmHistory = [...live.wpmHistory, live.wpm].slice(-30);
      }
      emit('live', live);
    }, 220);
  }

  const commands = {
    get_state: () => state,
    select_layer: ({ index }) => {
      state.live.selection.displayedIndex = index;
      emit('live', state.live);
    },
    set_setting: ({ key, value }) => {
      state.live.settings[key] = value;
      emit('live', state.live);
    },
    set_key_name: ({ displayText, name }) => {
      const names = state.keymap.keyNames;
      if (name && name.trim()) names[displayText] = name.trim();
      else delete names[displayText];
      for (const layer of state.keymap.keymap.layers) {
        for (const b of layer.bindings) b.name = names[b.displayText] || null;
      }
      emitAll();
    },
    set_shortcut: ({ accelerator }) => {
      state.live.settings.shortcut = accelerator;
      emit('live', state.live);
    },
    toggle_hud: () => {
      state.live.settings.hudEnabled = !state.live.settings.hudEnabled;
      emit('live', state.live);
    },
  };

  return {
    mock: true,
    invoke: async (cmd, args) => {
      await ready;
      const handler = commands[cmd];
      if (handler) return handler(args || {});
      console.log('[mock] invoke', cmd, args);
    },
    listen: async (event, handler) => {
      (listeners[event] = listeners[event] || []).push(handler);
    },
    window: () => ({
      startDragging: async () => {},
      startResizeDragging: async () => {},
    }),
  };
})();

/** The app's data, kept up to date from the Rust side. */
const Store = {
  keymap: null, // { keymap, filePath, error, recentKeymaps, keyNames }
  live: null, // { selection, pressed, wpm, wpmHistory, device, liveLayerName, settings, version }
  _handlers: [],

  onChange(handler) {
    this._handlers.push(handler);
  },

  _changed(what) {
    this._handlers.forEach((h) => h(what, this));
  },

  /** Counts keymap payloads, so windows can tell a new keymap from a repeat */
  keymapVersion: 0,

  async init() {
    await Bridge.listen('keymap', (payload) => {
      this.keymap = payload;
      this.keymapVersion++;
      this._changed('keymap');
    });
    await Bridge.listen('live', (payload) => {
      const before = this.live;
      this.live = payload;
      this._changed('live', before);
    });
    const initial = await Bridge.invoke('get_state');
    this.keymap = initial.keymap;
    this.keymapVersion++;
    this.live = initial.live;
    this._changed('keymap');
    this._changed('live', null);
  },

  get layer() {
    const km = this.keymap && this.keymap.keymap;
    if (!km || !this.live) return null;
    return km.layers[this.live.selection.displayedIndex] || null;
  },
};
