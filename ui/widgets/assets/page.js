const BOOT = JSON.parse(document.getElementById('boot').textContent);
const KEYMAP = BOOT.keymap || [];
let currentRevision = BOOT.revision || 0;

function el(node) {
  if (!node) return document.createTextNode('');
  if (node.text != null) return document.createTextNode(node.text);
  const element = document.createElement(node.tag || 'div');
  if (node.cls) element.className = node.cls;
  if (node.id) element.dataset.node = node.id;
  if (node.tabindex != null) element.tabIndex = Math.trunc(node.tabindex);
  if (node.ds) for (const [key, value] of Object.entries(node.ds)) element.dataset[key] = value;
  if (node.title != null) element.title = node.title;
  if (node.style) element.setAttribute('style', node.style);
  (node.children || []).forEach((child) => element.appendChild(el(child)));
  return element;
}

function mount(fragment) {
  const next = document.getElementById('screen');
  if (!next) return;
  // Replace content without reparsing markup: rebuild from JSON nodes.
  const temp = document.createDocumentFragment();
  temp.appendChild(el(fragment && fragment.root ? fragment.root : fragment));
  next.replaceChildren();
  next.appendChild(temp);
  document.body.dataset.theme = (fragment && fragment.theme) || BOOT.theme;
}

mount(BOOT);

const palette = document.getElementById('palette');
const paletteInput = document.getElementById('palette-input');
const paletteList = document.getElementById('palette-list');
let visible = KEYMAP;
let selected = 0;

function normalize(event) {
  const parts = [];
  if (event.metaKey) parts.push('super');
  if (event.ctrlKey) parts.push('ctrl');
  if (event.altKey) parts.push('alt');
  if (event.shiftKey) parts.push('shift');
  let key = event.key.toLowerCase();
  if (key === ' ') key = 'space';
  if (key === '/') key = 'slash';
  if (key === 'enter') key = 'return';
  if (key === 'escape') key = 'escape';
  parts.push(key);
  return parts.join('+');
}

function prettyKeys(keys) {
  return keys.split('+').map((part) => {
    if (part === 'super') return '⌘';
    if (part === 'shift') return '⇧';
    if (part === 'alt') return '⌥';
    if (part === 'ctrl') return '⌃';
    return part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1);
  }).join(' ');
}

function renderPalette() {
  paletteList.innerHTML = '';
  visible.forEach((binding, index) => {
    const button = document.createElement('button');
    button.className = 'palette-item' + (index === selected ? ' selected' : '');
    button.innerHTML = '<span class="label"></span><kbd></kbd>';
    button.querySelector('.label').textContent = binding.label;
    button.querySelector('kbd').textContent = prettyKeys(binding.keys);
    button.addEventListener('click', () => run(binding.action));
    paletteList.appendChild(button);
  });
  if (visible.length === 0) {
    paletteList.innerHTML = '<div class="palette-item"><span class="label">No matching command</span></div>';
  }
}

function openPalette(query = '') {
  palette.classList.add('open');
  palette.setAttribute('aria-hidden', 'false');
  selected = 0;
  visible = KEYMAP;
  paletteInput.value = query;
  filterPalette(query);
  paletteInput.focus();
}

function closePalette() {
  palette.classList.remove('open');
  palette.setAttribute('aria-hidden', 'true');
  paletteInput.value = '';
}

function filterPalette(query) {
  const needle = query.trim().toLowerCase();
  visible = needle.length === 0
    ? KEYMAP
    : KEYMAP.filter((binding) =>
        binding.label.toLowerCase().includes(needle) ||
        binding.action.toLowerCase().includes(needle) ||
        binding.keys.toLowerCase().includes(needle));
  selected = 0;
  renderPalette();
}

function moveSelection(delta) {
  if (visible.length === 0) return;
  selected = (selected + delta + visible.length) % visible.length;
  renderPalette();
  const node = paletteList.querySelectorAll('.palette-item')[selected];
  if (node) node.scrollIntoView({ block: 'nearest' });
}

function run(action) {
  closePalette();
  dispatch(action);
}

function toast(text) {
  const element = document.getElementById('toast');
  element.textContent = text;
  element.classList.add('show');
  clearTimeout(element._timer);
  element._timer = setTimeout(() => element.classList.remove('show'), 1400);
}

function focusMove(direction) {
  const focusable = Array.from(document.querySelectorAll('.focusable'));
  if (focusable.length === 0) return;
  const current = focusable.indexOf(document.activeElement);
  let next;
  if (current === -1) {
    next = 0;
  } else if (direction === 'left' || direction === 'up') {
    next = (current - 1 + focusable.length) % focusable.length;
  } else {
    next = (current + 1) % focusable.length;
  }
  focusable[next].focus();
}

async function dispatch(action) {
  if (action === 'palette.open') return openPalette();
  if (action === 'help.show') return openPalette();
  if (action === 'ui.reload') return location.reload();
  if (action.startsWith('focus.')) return focusMove(action.split('.')[1]);
  if (action.startsWith('workspace.')) {
    toast('Workspace ' + action.split('.')[1]);
    return;
  }
  try {
    await fetch('/action?name=' + encodeURIComponent(action));
    toast(action.replace('.', ' '));
    await refresh(true);
  } catch (error) {
    toast('Action failed: ' + action);
  }
}

function updateClock(element) {
  const format = element.dataset.format || '24h';
  const seconds = element.dataset.seconds === 'true';
  const now = new Date();
  let hours = now.getHours();
  let suffix = '';
  if (format === '12h') {
    suffix = hours >= 12 ? ' PM' : ' AM';
    hours = hours % 12 || 12;
  }
  const pad = (value) => String(value).padStart(2, '0');
  const time = pad(hours) + ':' + pad(now.getMinutes()) + (seconds ? ':' + pad(now.getSeconds()) : '') + suffix;
  const target = element.querySelector('.clock-time');
  if (target) target.textContent = time;
}

async function refresh(force) {
  try {
    const stateResponse = await fetch('/state');
    const state = await stateResponse.json();
    if (!force && state.revision === currentRevision) return;
    currentRevision = state.revision;
    document.body.dataset.theme = state.theme;
    const fragment = await fetch('/fragment');
    const next = await fragment.json();
    mount(next);
  } catch (error) {
    /* the host may be restarting */
  }
}

paletteInput.addEventListener('input', () => filterPalette(paletteInput.value));

document.addEventListener('keydown', (event) => {
  if (palette.classList.contains('open')) {
    if (event.key === 'Escape') { closePalette(); event.preventDefault(); return; }
    if (event.key === 'ArrowDown') { moveSelection(1); event.preventDefault(); return; }
    if (event.key === 'ArrowUp') { moveSelection(-1); event.preventDefault(); return; }
    if (event.key === 'Enter') {
      if (visible[selected]) run(visible[selected].action);
      event.preventDefault();
      return;
    }
    return;
  }

  const target = event.target;
  const typing = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
  if (!typing && event.key === '/') { openPalette(); event.preventDefault(); return; }
  if (!typing && event.key === '?') { openPalette(); event.preventDefault(); return; }

  const combo = normalize(event);
  const binding = KEYMAP.find((entry) => entry.keys === combo);
  if (binding) {
    event.preventDefault();
    dispatch(binding.action);
  }
});

document.addEventListener('click', (event) => {
  const target = event.target.closest('[data-action]');
  if (target && (target.classList.contains('button') || target.classList.contains('card'))) {
    dispatch(target.dataset.action);
  }
});

setInterval(() => {
  document.querySelectorAll('.clock').forEach(updateClock);
}, 1000);
document.querySelectorAll('.clock').forEach(updateClock);
setInterval(() => refresh(false), 1500);

if (new URLSearchParams(location.search).has('palette')) {
  openPalette();
}
