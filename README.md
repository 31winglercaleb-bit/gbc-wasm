# gbc-wasm

Game Boy Color emulator in Rust -> WebAssembly with IndexedDB save states and iPad Safari frontend.

This repository contains a scaffold of an LR35902 CPU core and a wasm-bindgen-friendly frontend.

## Development / Running in Browser (iOS Safari)

1. Build the wasm package using wasm-pack (recommended):

   ```bash
   wasm-pack build --target web
   ```

   This will output a `pkg/` directory with `gbc_wasm.js` and the wasm binary. The `static/` frontend expects `./pkg/gbc_wasm.js`.

2. Serve the repository with a static file server (do NOT open the file:// url). Example using Python:

   ```bash
   python3 -m http.server 8000
   ```

   Then open `http://<host>:8000/static/index.html` in Safari on iOS/iPadOS.

3. Load a `.gb` or `.gbc` ROM using the file chooser and press Start.

Notes for iOS/Safari compatibility:

- The frontend avoids SharedArrayBuffer and other features that require cross-origin isolation. It uses ImageData + Canvas which is available in Safari.
- Retina devices are supported by scaling the canvas using devicePixelRatio.

## What I changed / added

- Added a minimal web frontend at `static/index.html` and `static/main.js` which loads the wasm package, accepts a ROM file, runs frame stepping, renders to a canvas, and stores save states in IndexedDB.
- Updated README with build and run instructions.

## Next steps (suggested)

- Implement full MMU, PPU (pixel-accurate rendering), timers, interrupts, and complete opcode set in Rust.
- Add accurate timing (synchronizing cycles to real time) and audio support via WebAudio.
- Add touch/gamepad controls and on-screen buttons for iOS input mapping.
- Add a small service worker or bundler configuration if you want to package the web UI for distribution.
