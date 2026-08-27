// static/main.js
// Minimal frontend to load wasm (wasm-pack output expected in ./pkg) and run the emulator.
// Designed to be compatible with Safari on iOS/iPadOS (avoid SharedArrayBuffer, use ImageData + canvas).

const WIDTH = 160;
const HEIGHT = 144;

let wasm;
let Emulator;
let emu;
let running = false;
let rafId = null;

const fileInput = document.getElementById('file');
const startBtn = document.getElementById('start');
const pauseBtn = document.getElementById('pause');
const saveBtn = document.getElementById('save');
const loadBtn = document.getElementById('load');
const statusEl = document.getElementById('status');
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

// Scale canvas for devicePixelRatio (important on Retina iPads)
function resizeCanvasForDPR() {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = WIDTH * dpr;
  canvas.height = HEIGHT * dpr;
  canvas.style.width = WIDTH + 'px';
  canvas.style.height = HEIGHT + 'px';
  ctx.imageSmoothingEnabled = false;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}
resizeCanvasForDPR();
window.addEventListener('resize', resizeCanvasForDPR);

async function initWasm() {
  statusEl.textContent = 'Loading wasm...';
  try {
    // Expect wasm-pack / wasm-bindgen output under ./pkg/gbc_wasm.js
    // If you built with `wasm-pack build --target web`, the path will be ./pkg/gbc_wasm.js
    const m = await import('../pkg/gbc_wasm.js');
    wasm = m;
    Emulator = m.Emulator;
    statusEl.textContent = 'WASM loaded.';
  } catch (e) {
    statusEl.textContent = 'Failed to load wasm — make sure you built the package with wasm-pack and are serving ./pkg/ from the server console.';
    console.error(e);
  }
}

function bytesToUint8Array(bytes) {
  return new Uint8Array(bytes);
}

function drawFrame(buffer) {
  // buffer is a Uint8Array from wasm: RGBA per pixel
  const clamped = new Uint8ClampedArray(buffer.buffer ? buffer.buffer : buffer);
  const img = new ImageData(clamped, WIDTH, HEIGHT);
  ctx.putImageData(img, 0, 0);
}

function frameLoop() {
  if (!running || !emu) return;
  // Step one frame in wasm
  emu.step_frame();
  const fb = emu.render_frame(); // Uint8Array produced by wasm-bindgen
  drawFrame(fb);
  rafId = requestAnimationFrame(frameLoop);
}

fileInput.addEventListener('change', async (ev) => {
  const f = ev.target.files && ev.target.files[0];
  if (!f) return;
  const arr = new Uint8Array(await f.arrayBuffer());
  if (!Emulator) await initWasm();
  emu = new Emulator();
  // Load ROM bytes into wasm
  emu.load_rom(arr);
  startBtn.disabled = false;
  pauseBtn.disabled = false;
  saveBtn.disabled = false;
  loadBtn.disabled = false;
  statusEl.textContent = `Loaded ROM: ${f.name} (${arr.length} bytes)`;
});

startBtn.addEventListener('click', () => {
  if (!emu) return;
  if (running) return;
  running = true;
  frameLoop();
  statusEl.textContent = 'Running';
});

pauseBtn.addEventListener('click', () => {
  running = false;
  if (rafId) cancelAnimationFrame(rafId);
  statusEl.textContent = 'Paused';
});

// Simple IndexedDB wrapper for save states
function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open('gbc-wasm', 1);
    req.onupgradeneeded = () => {
      req.result.createObjectStore('states');
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function saveState() {
  if (!emu) return;
  const db = await openDb();
  const tx = db.transaction('states', 'readwrite');
  const store = tx.objectStore('states');
  const state = emu.save_state(); // Uint8Array
  store.put(state, 'slot0');
  return new Promise((res, rej) => {
    tx.oncomplete = () => { statusEl.textContent = 'State saved.'; res(); };
    tx.onerror = () => { statusEl.textContent = 'Save failed.'; rej(tx.error); };
  });
}

async function loadState() {
  const db = await openDb();
  const tx = db.transaction('states', 'readonly');
  const store = tx.objectStore('states');
  const req = store.get('slot0');
  return new Promise((resolve, reject) => {
    req.onsuccess = () => {
      const v = req.result;
      if (!v) { statusEl.textContent = 'No save found.'; resolve(false); return; }
      // req.result may be a Uint8Array already
      emu.load_state(v);
      statusEl.textContent = 'State loaded.';
      resolve(true);
    };
    req.onerror = () => { statusEl.textContent = 'Load failed.'; reject(req.error); };
  });
}

saveBtn.addEventListener('click', () => saveState());
loadBtn.addEventListener('click', () => loadState());

// Initialize wasm early so the user gets feedback
initWasm();

// Prevent touch scrolling on the canvas so gestures while playing don't trigger browser nav
canvas.addEventListener('touchstart', (e) => { e.preventDefault(); }, { passive: false });
canvas.addEventListener('touchmove', (e) => { e.preventDefault(); }, { passive: false });
