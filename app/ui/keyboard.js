// Draws the keyboard grid. Same geometry as the SwiftUI KeyboardGridView:
// 54 pt keys (48 pt on boards with more than 10 keys in the first row),
// 44 pt tall, 3 pt apart, a 30 pt gap between the halves, thumb row inset.

const KeyboardView = (() => {
  const KEY_HEIGHT = 44;
  const SPACING = 3;
  const SPLIT_GAP = 30;

  function keyWidth(layout) {
    const first = layout.keysPerRow[0];
    return first !== undefined && first > 10 ? 48 : 54;
  }

  /** Size of the grid at scale 1 */
  function naturalSize(layout) {
    const maxKeys = Math.max(0, ...layout.keysPerRow) || 10;
    const width = maxKeys * keyWidth(layout) + (maxKeys - 1) * SPACING + (layout.isSplit ? SPLIT_GAP : 0);
    const rows = layout.rowCount;
    const height = rows * KEY_HEIGHT + (rows - 1) * SPACING + (layout.hasThumbCluster ? 8 : 0);
    return { width, height };
  }

  const charCount = (text) => Array.from(text).length;

  /** The label as shown: a long name wraps after its first word */
  function labelOf(binding) {
    const text = binding.name || binding.displayText;
    if (binding.name && charCount(text) > 8) {
      const space = text.indexOf(' ');
      if (space !== -1 && space <= 10) return text.slice(0, space) + '\n' + text.slice(space + 1);
    }
    return text;
  }

  function fontSizeOf(label) {
    const n = charCount(label);
    if (n > 6) return 8;
    if (n > 4) return 9;
    if (n > 2) return 11;
    return 12;
  }

  /** Colour class of a key, by what it does (the name never changes it) */
  function kindOf(binding, isThumb) {
    const text = binding.displayText;
    const raw = binding.rawCode;
    if (text === '▽') return 'trans';
    if (text === '✕') return 'none';
    if (isThumb) return 'thumb';
    if (text.includes('\n')) return 'tap';
    if (raw.startsWith('&mo') || raw.startsWith('&to') || raw.startsWith('&tog')) return 'layer';
    if (['BT', 'RESET', 'BOOT', 'STUDIO'].some((s) => text.includes(s))) return 'system';
    return '';
  }

  function el(tag, className, styles) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (styles) Object.assign(node.style, styles);
    return node;
  }

  function keyElement(binding, position, isThumb, width, options) {
    const label = labelOf(binding);
    const classes = ['key', kindOf(binding, isThumb)];
    if (options.pressed && options.pressed.has(position)) classes.push('pressed');
    const key = el('div', classes.filter(Boolean).join(' '), { width: width + 'px' });
    const text = el('span', 'label');
    text.textContent = label;
    text.style.fontSize = fontSizeOf(label) + 'px';
    key.appendChild(text);

    // Tooltip with the raw binding after a short hover
    let timer = null;
    key.addEventListener('mouseenter', () => {
      timer = setTimeout(() => {
        const tip = el('div', 'tooltip');
        tip.textContent = binding.rawCode;
        key.appendChild(tip);
      }, 500);
    });
    key.addEventListener('mouseleave', () => {
      clearTimeout(timer);
      key.querySelectorAll('.tooltip').forEach((t) => t.remove());
    });

    if (options.onContext) {
      key.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        options.onContext(binding, e);
      });
    }
    return key;
  }

  /** Builds the grid for one layer. options: { pressed: Set, onContext } */
  function render(layer, layout, options = {}) {
    const root = el('div', 'kb');
    const width = keyWidth(layout);
    const maxKeys = Math.max(0, ...layout.keysPerRow);

    for (let row = 0; row < layout.rowCount; row++) {
      const keysInRow = layout.keysPerRow[row] || 0;
      const entries = layer.bindings
        .map((binding, position) => ({ binding, position }))
        .filter((e) => e.binding.row === row)
        .sort((a, b) => a.binding.column - b.binding.column);
      const isThumbRow = row === layout.rowCount - 1 && layout.hasThumbCluster;
      const half = Math.floor(keysInRow / 2);

      const keysDifference = maxKeys - keysInRow;
      const oneKey = width + SPACING;
      const extraGap = layout.isSplit && !isThumbRow && keysInRow < maxKeys ? keysDifference * oneKey : 0;
      const baseGap = layout.isSplit ? (isThumbRow ? SPLIT_GAP + 20 : SPLIT_GAP) : 0;
      const gap = baseGap + extraGap;
      const outer = layout.isSplit && isThumbRow && keysInRow < maxKeys ? Math.floor(keysDifference / 2) * oneKey : 0;

      const rowNode = el('div', 'krow', { paddingTop: isThumbRow ? '8px' : '0' });
      if (outer > 0) rowNode.appendChild(el('div', '', { width: outer + 'px', flex: 'none' }));

      const content = el('div', 'krow splitrow');
      const makeHalf = (from, to) => {
        const h = el('div', 'khalf');
        for (let i = from; i < to; i++) {
          const entry = entries[i];
          if (entry) h.appendChild(keyElement(entry.binding, entry.position, isThumbRow, width, options));
        }
        return h;
      };

      if (layout.isSplit) {
        content.appendChild(makeHalf(0, half));
        content.appendChild(el('div', '', { width: gap + 'px', flex: 'none' }));
        content.appendChild(makeHalf(half, keysInRow));
      } else {
        content.appendChild(makeHalf(0, keysInRow));
      }
      rowNode.appendChild(content);
      if (outer > 0) rowNode.appendChild(el('div', '', { width: outer + 'px', flex: 'none' }));
      root.appendChild(rowNode);
    }
    return root;
  }

  return { render, naturalSize, KEY_HEIGHT };
})();

/** Typing speed graph: a line over the samples, scaled to the highest one
 *  (at least 40 WPM, so slow typing doesn't fill the graph) */
function wpmGraphPoints(values, width = 54, height = 14, historyLength = 30) {
  if (values.length < 2) return '';
  const top = Math.max(Math.max(...values), 40);
  const step = width / (historyLength - 1);
  const startX = width - (values.length - 1) * step;
  return values.map((v, i) => `${(startX + i * step).toFixed(1)},${(height - (v / top) * height).toFixed(1)}`).join(' ');
}
