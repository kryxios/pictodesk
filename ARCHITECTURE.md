# PictoDesk architecture

How the code that exists today is put together. This describes the
engine as built, not future plans.

## Crates

- **`crates/pictodesk-core`** — the engine. No UI dependencies, ever.
  Everything an editor needs that isn't a window lives here, so it can
  be tested headlessly and reused by any shell.
- **`apps/pictodesk-app`** — the desktop shell (winit + wgpu). It owns
  a window, uploads the engine's flattened render as a GPU texture,
  and handles pan/zoom input. It contains no image logic.
- **`apps/pictodesk-cli`** — a headless harness that builds a document,
  exercises history and the file format, and exports a preview. It is
  the engine's smoke test in executable form.

## Core engine (`pictodesk-core`)

- **`pixel`** — `Pixel16`: RGBA, 16 bits per channel, straight alpha.
  The internal pipeline is 16-bit from the first commit; 8-bit exists
  only at import/export/display boundaries.
- **`tile`** — images are stored in 256×256 tiles. Layers allocate
  tiles sparsely: unpainted regions cost no memory. Tiles carry a
  dirty flag for render/autosave invalidation. Tile equality is
  content equality (the dirty flag is bookkeeping, not content).
- **`layer`** — a sparse tile map plus opacity, visibility, blend
  mode, and offset. Layers are in the document model from Phase 0,
  before any UI exposes them.
- **`document`** — canvas size, working space, layer stack (index 0 =
  bottom). `render_pixel` / `render_region_rgba8` composite layers
  bottom-up; `render_checksum` gives tests a fast whole-image
  fingerprint.
- **`blend`** — blend-mode math (Normal, Multiply, Screen, Overlay,
  Soft Light, Darken, Lighten, Difference) and Porter-Duff "over"
  compositing with blending, operating on 0..=1 floats.
- **`color`** — the built-in working spaces (sRGB, Adobe RGB,
  ProPhoto): transfer functions and XYZ-matrix conversion.
- **`icc`** — real ICC profiles via moxcms (pure Rust): load any
  `.icc`/`.icm` from bytes or file, convert RGBA8 or RGB f32 data
  between profiles. This is the seam where color management beyond
  the built-in spaces lives.
- **`history`** — undo/redo as a stack of commands. Pixel edits are
  `TileEditCommand`s: snapshot the tiles a stroke will touch, paint,
  capture the result. Memory cost scales with tiles touched, not
  canvas size.
- **`format`** — the `.pdesk` file format. v2 = header + DEFLATE
  (flate2/miniz_oxide) over the complete v1 payload; the loader
  dispatches on version and reads v1 files forever. Saves for
  recovery are atomic (write temp, rename), so a crash mid-save
  leaves the previous file intact.

## Shell (`pictodesk-app`)

- **`viewport`** — pure document↔screen math: fit, zoom-at-cursor
  (the document point under the cursor stays under the cursor),
  pan, clamping. Unit-tested separately from any window code.
- **`shader.wgsl`** — one fullscreen triangle; the fragment shader
  maps screen pixels through the viewport, samples the document
  texture, draws a checkerboard behind transparency and a dark
  surround outside the document.
- **`--shot <file.ppm>`** — verification mode: renders one frame
  offscreen through the same pipeline and writes it out, so the GPU
  path is testable on machines without a display.

## Testing

`cargo test` runs unit tests in every module plus an integration
test (`crates/pictodesk-core/tests/phase0.rs`) that walks the
Phase 0 exit criteria end to end: multi-tile multi-layer render,
history undo/redo exactness, color conversion, and a lossless
format round trip. Pixel-output (golden) checks use render
checksums and known-value spot checks.
