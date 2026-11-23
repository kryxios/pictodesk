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

## Phase 1 additions (v0.1.0)

Core modules, all selection-aware and history-wrapped:

- `selection.rs` — selections as a grayscale mask (255 = selected):
  rect/ellipse/lasso polygon/magic wand constructors, feather
  (separable box blur), inverse, bounds. History helpers snapshot
  exactly the tiles a selection touches (`Selection::tiles_in_rect`).
- `adjust.rs` — the Image > Adjust menu. A single `map_layer_pixels`
  walker applies a per-pixel function over a layer's painted tiles,
  blended by the selection mask; alpha is never created or destroyed.
- `filter.rs` — region-snapshot filters (Gaussian Blur, Sharpen,
  Unsharp Mask, Add Noise with a deterministic hash RNG).
- `transform.rs` — Image Size (bilinear), Canvas Size (3x3 anchor),
  Crop, Rotate, Flip; structural edits undo via `DocumentCommand`
  (whole-document snapshot) in `history.rs`, which also gained a
  configurable undo-depth limit and `MoveLayerCommand`.
- `brush.rs` — the painting tools as stamp/stroke functions with
  classic spacing (25% of diameter): Brush, Pencil, Eraser, Clone
  Stamp (aligned), Paint Bucket, Gradient, Blur/Sharpen/Smudge
  stamps, plus the eyedropper's averaged sampler.
- `io.rs` — PNG/JPEG/TIFF/BMP via the pure-Rust `image` crate;
  embedded ICC profiles are converted into the working space on
  import. `histogram.rs` feeds the Histogram palette.

The app shell is now a classic 1990-era workbench drawn on the CPU into
a framebuffer (platinum chrome, `font8x8` pixel text) and uploaded
as one GPU texture per frame: menu bar with submenus, two-column
toolbox, FG/BG swatches + color palette, Brushes and Histogram
palettes, modal dialogs with live preview (revert-from-snapshot,
re-apply, commit on OK), a file-open panel, drag-and-drop, and the
1.0-authentic single level of Undo (`History::set_limit(Some(1))` —
the engine itself is unlimited; later phases raise the cap).

## Phase 2 additions (v0.2.0)

- `path.rs` — Bézier paths: anchors with in/out direction points,
  subpaths (open/closed), adaptive flattening, even-odd fill to a
  selection, stroke point generation, and a crack-following boundary
  trace that turns a selection back into a path.
- Document model grows: a color mode (RGB / CMYK / Grayscale), alpha
  channels (an 8-bit buffer each; a channel may carry a spot color),
  saved paths, a work path, and a clipping-path index. `.pdesk` is
  now v3 — the v2 envelope carries an extension block; v1/v2 files
  load with defaults.
- `mode.rs` + `color.rs` — mode conversion. Since v0.2.1 a CMYK
  document stores real ink values: the four tile slots hold C, M,
  Y, K and a sparse per-layer alpha plane (`Layer::cmyk_alpha`)
  holds coverage. Rendering composites in ink space and converts
  to RGB for display; painting, adjustments, filters, and
  transforms all have ink-space paths, and tile-snapshot undo
  captures the alpha tiles alongside the ink tiles. The `.pdesk`
  extension block appends the alpha planes (older v3 files load
  with empty planes).
- `io.rs` — 16-bit sources import bit-exact (ICC conversion still
  applies to 8-bit sources), and a hand-rolled Separation TIFF
  writer: Photometric 5, InkSet/InkNames/NumberOfInks tags, spot
  channels as extra ink samples (their own plates), one strip,
  plus tag 34377 carrying the clipping path as an image resource
  block when one is set. CMYK-mode documents saved as `.tif`
  route here automatically.
- `selection.rs` — Grow (boundary flood by color tolerance) and
  Similar (global color-range match).
- `brush.rs` — channel painting: tip stamps and flood fill that work
  on a raw channel buffer, for painting alpha/spot channels directly.

The shell gains the Pen tool (click = corner anchor, drag = smooth
anchor with mirrored handles, click the first anchor to close,
Backspace deletes the last anchor) and a right-hand dock of tabbed
palettes: a Channels/Paths group (channel viewing and paint targets,
path fill/stroke/convert/save/clip operations), Brushes, Histogram,
and Color. The dock resizes the canvas; rulers stop at the dock edge.
Painting with an alpha channel targeted routes brush, pencil,
eraser, bucket, fill, and clear into that channel's buffer, undoable
as a whole-document command.
