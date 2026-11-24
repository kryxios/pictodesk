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
- **`format`** — the `.pdk` file format. v2 = header + DEFLATE
  (flate2/miniz_oxide) over the complete v1 payload; v3 adds an
  extension block (color mode, alpha/spot channels, paths, layer
  links, CMYK alpha planes, the provenance history log), every part
  of it optional so older v3 files still load. The loader dispatches
  on version and reads v1/v2 files forever. Saves for recovery are
  atomic (write temp, rename), so a crash mid-save leaves the
  previous file intact.

## Shell (`pictodesk-app`)

- **`viewport`** — pure document↔screen math: fit, zoom-at-cursor
  (the document point under the cursor stays under the cursor),
  pan, clamping. Unit-tested separately from any window code.
- **inline WGSL shader** (in `main.rs`) — one fullscreen triangle
  that presents the CPU-drawn workbench framebuffer as a GPU
  texture; the document area inside it is the engine's flattened
  render over a checkerboard, with a dark surround outside the
  document.
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

## Phase 4 additions (v0.4.0)

- `adjust.rs` — the `Adjustment` enum is the single source of
  truth for adjustment math: the destructive Image > Adjust
  commands and adjustment layers both call `Adjustment::apply`.
  Six kinds: Levels, Curves, Brightness/Contrast, Hue/Saturation,
  Color Balance, Photo Filter (tint toward a filter color at the
  pixel's own luminance, blended by density).
- Layers grow two optional fields: a sparse tile-plane mask
  (`mask`, with `mask_fill` as the absent-tile value and a
  `mask_enabled` switch) and an `adjustment`. Compositing
  multiplies a layer's alpha by its mask factor; an adjustment
  layer transforms the accumulated composite beneath it,
  weighted by opacity × mask factor. CMYK documents route the
  composite through RGB for the adjustment and back into ink
  space. Masks survive transforms, flips, and image resizes.
- `merge_down` gains a bake path: when either layer carries a
  mask, or the top layer is an adjustment layer, the pair is
  rendered in isolation and the result inherits the bottom
  layer's blend mode and opacity — the composite is bit-identical
  after the merge (test-covered). The old fast path dropped
  masks silently; it now only runs for plain layer pairs.
- `.pdk`: masks, adjustment parameters, and guides ride as one
  more optional trailing section of the v3 envelope; files
  written before Phase 4 load unchanged.
- Guides live on the Document (`guides_x` / `guides_y`). The
  shell adds drag-from-the-ruler guide creation, Move-tool guide
  dragging (drag off-canvas to delete), a toggleable grid, and
  snapping of Move-tool drags to guides and grid. The Navigator
  palette shows a thumbnail with the visible-area rectangle and
  click/drag panning. Right-click context menus cover the canvas
  and the Layers panel rows.

## Phase 5 additions (v0.5.0)

- `history.rs`: the undo stack becomes a state list with a
  position cursor — linear branching (push truncates the redo
  tail) or non-linear insertion, `goto_state`, and
  `doc_at_state` reconstruction for the History Brush source.
  Commands store absolute tile states, so jumps land exactly.
  Snapshots are named full-document clones, session-only.
- `effects.rs`: the classic five effects render from the
  layer's alpha — `below_buffer` (Outer Glow, Drop Shadow)
  composites under the content; `apply_within` (Inner Shadow,
  Inner Glow, Bevel) modifies the content buffer. The mask is
  applied to content alpha before effects, so shadows follow
  the masked shape. Known simplification: a below-effect's
  blend mode is honored where it meets the layer's own pixels;
  the merged below-buffer composites under the layer as Normal,
  so the mode does not reach the backdrop. Effected CMYK
  documents flatten through the RGB composite (documented
  approximation in `flatten()`).
- `text.rs` is spec-only: a type layer stores its `TextSpec`
  (text, style runs, alignment, vertical flag) and the shell
  (`typo.rs`) renders it with rustybuzz shaping + fontdue
  rasterization. The layer's `offset` is the text block origin,
  so the Move tool carries type like any layer content.
- `filter.rs` completes the gallery: the Distort family shares
  an inverse-map driver with clipped (transparent-outside)
  bilinear sampling; 3D Transform ray-casts cube/sphere/cylinder
  UVs; Extract mattes a foreground from highlight/fill masks by
  nearest-color distance with decontamination.
- `psd.rs` gains export (8-bit layered PSD with honest fidelity
  warnings) and `io.rs` DCS 2.0 separations. An importer bug
  was found and fixed in this phase: the layer count is an i16
  in the spec, not i32 — real-world layered PSDs now parse.
- Working-space conversion (`mode::convert_working_space`)
  re-encodes every pixel through XYZ between sRGB, Adobe RGB,
  and ProPhoto RGB using the published matrices in `color.rs`.

## Phase 4.5 additions (v0.4.5)

- `blend.rs` grows to the full classic 17 modes. Dissolve is not
  a channel formula: `Document::render_pixel` / `composite_inks`
  gate the whole pixel on `dissolve_hash(x, y, layer_index)` vs
  its final alpha, so renders are stable and checksummable.
- `selection.rs`: Modify operations run on a two-pass chamfer
  distance transform of the thresholded mask (Expand/Contract;
  Border = expand(a) minus contract(b); Smooth = feather +
  re-threshold). `color_range` builds a graduated selection from
  the composite by RGB distance to a sample color.
- `filter.rs`: twelve filters share one RGB-region pipeline
  (`rgb_filter_rect`); CMYK documents round-trip the region
  through RGB, the same route Photo Filter takes. Clouds is
  deterministic value-noise fBm between two colors; Custom is a
  5x5 convolution with scale/offset. `brush::stamp_tone` powers
  Dodge/Burn/Sponge with range weighting by pixel luminance.
- Dialogs now apply their default values as the opening preview
  (`open_dialog` refreshes once), so OK commits an untouched
  dialog instead of silently doing nothing.
- Actions: the shell records steps (adjustments, filters,
  geometry, layer operations) into named actions, replays them
  as one undoable step, and batch-processes a folder — each
  image imported, stepped, and written back as
  `<name>-edited.png` beside the untouched original.

## Phase 3 additions (v0.3.0)

- Layers become the working surface: the document's layer stack
  gets the full operation set (`duplicate_layer`, `delete_layer`,
  `move_layer`, `merge_down`, `merge_visible`, `flatten` — the
  merge/flatten bakes are mode-aware, with ink-space paths for
  CMYK documents). Layers carry a `linked` flag (persisted as an
  optional trailing section in the `.pdk` extension block, so
  older v3 files still load).
- `blend.rs` — the component modes (Hue, Saturation, Color,
  Luminosity) join the eight per-channel modes, using the classic
  Lum/Sat/ClipColor formulation on whole RGB triples; ink-space
  compositing routes them through RGB.
- `transform.rs` — `translate_layer` plus `transform_layer`: Free
  Transform (scale / rotate / skew about the painted-bounds
  center, bilinear, mode-aware sampling; off-canvas content is
  kept because layers are unbounded tile maps).
- History metadata: `Document::history_log` records provenance
  events (Created / Opened / Saved, plus the label of every
  undoable edit, pushed centrally by the shell's `push_cmd`), is
  stored as an optional trailing section of the `.pdk`
  extension block (capped at 500 entries), and is shown by the
  shell's File > History Log dialog. Undo states themselves are
  never stored — session-only, as in the classic editors.
- `psd.rs` — PSD / PSB import: 1/8/16-bit Bitmap / Grayscale /
  Indexed / RGB / CMYK / Multichannel, raw, PackBits and ZIP
  (plain and delta-predicted) channel data, layer records (stored
  top-first in the file) with names, bounds (kept as layer
  offsets), opacity, visibility, classic blend keys, and layer
  masks (baked into transparency and preserved as alpha
  channels). Everything that cannot come across — groups as
  structure, type/effects layers, unknown blend keys — is
  reported in a `PsdReport`, never silently dropped. A file with
  no layer data imports its merged composite instead.
- The shell gains a Layer menu (with a Blend Mode submenu),
  a third dock tab — Layers, beside Channels and Paths — with
  thumbnails rendered from `Document::render_layer_pixel`, a
  scrolling panel that keeps the active layer visible, an
  on-canvas Free Transform (bounding box with handles, live
  preview re-derived from the pre-transform document each mouse
  move), autosave to the core's atomic recovery file every 20 s
  with recovery offered on launch, and
  linked-layer dragging in the Move tool (one multi-layer
  `MoveLayerCommand`).

## Phase 2 additions (v0.2.0)

- `path.rs` — Bézier paths: anchors with in/out direction points,
  subpaths (open/closed), adaptive flattening, even-odd fill to a
  selection, stroke point generation, and a crack-following boundary
  trace that turns a selection back into a path.
- Document model grows: a color mode (RGB / CMYK / Grayscale), alpha
  channels (an 8-bit buffer each; a channel may carry a spot color),
  saved paths, a work path, and a clipping-path index. `.pdk` is
  now v3 — the v2 envelope carries an extension block; v1/v2 files
  load with defaults.
- `mode.rs` + `color.rs` — mode conversion. Since v0.2.1 a CMYK
  document stores real ink values: the four tile slots hold C, M,
  Y, K and a sparse per-layer alpha plane (`Layer::cmyk_alpha`)
  holds coverage. Rendering composites in ink space and converts
  to RGB for display; painting, adjustments, filters, and
  transforms all have ink-space paths, and tile-snapshot undo
  captures the alpha tiles alongside the ink tiles. The `.pdk`
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

## Phase 6 additions (v0.6.0)

- `effects.rs` grows to the full ten-effect style set: Satin,
  Color Overlay, Gradient Overlay, Pattern Overlay and Stroke
  join the original five. Shadows, glows and Bevel carry a
  `contour` curve (Linear / Cone / Ring / Cove) applied to the
  falloff, and Bevel carries an optional procedural texture
  (checker / dots / diagonal lines). Pattern overlays and
  bevel textures sample the same procedural pattern set.
- `shapes.rs`: vector shape layers (rectangle, rounded
  rectangle, ellipse, polygon/star, line, custom) and fill
  layers (solid, gradient, pattern) are point-sampled at pixel
  centers by the compositor — the spec stays editable until
  rasterized. In CMYK documents shape/fill layers fall back to
  their stored pixels (v1).
- Advanced blending lives in the compositor: Fill Opacity
  scales content but not below-effects; Blend If applies hard
  edges (no split-slider feather); clipping groups clip to the
  nearest non-clipped visible layer below; Knockout punches
  to the backdrop (Shallow is treated as Deep in v1);
  per-channel blend masks restore un-blended channels after
  compositing. Layer sets are one level deep: set opacity and
  visibility multiply/hide members; a set's blend mode is
  stored, with Pass Through honored.
- `liquify.rs`: a document-sized displacement field with
  brush stamps (warp, twirl both ways, pucker, bloat, shift,
  reflection) honoring a frozen mask, four reconstruct modes,
  and a bilinear bake into the layer.
- `warp.rs`: the fifteen classic Warp Text styles as a
  region warp; zero bend is an exact identity for every style.
- `transform.rs::perspective_crop`: warps the document so a
  user quad becomes the new rectangular canvas (RGB/grayscale
  documents; CMYK is refused rather than warped wrong).
- The `.pdk` extension block appends a Phase 6 section: the
  new effects, fill opacity, Blend If, knockout, channel
  mask, group/clipping/locks/color, shape and fill specs,
  text box/warp fields, layer sets, notes, slices, custom
  shapes and the global light altitude. Older files load with
  Phase 6 defaults.
- Droplets are a shell script + a `.steps` file written next
  to the document; running the app with `--droplet-run`
  applies the serialized steps to each image argument and
  writes `<name>-edited.png`. Only droplet-safe steps
  serialize; the rest are counted and reported at creation.

## Phase 6.1 additions (v0.6.1)

The full-behavior pass over the Phase 6 surface:

- The compositor's three render loops share one set-group
  state machine. A layer set with its own blend mode renders
  its members into an isolated group buffer that is flushed
  over the backdrop with the set's mode and opacity; a
  Pass Through set blends its members directly, scaled by
  the set opacity.
- Blend If gains split-slider feathering: each of the four
  thresholds carries a split width (`blend_if_split`,
  persisted as a trailing `.pdk` block) and the visibility
  factor ramps linearly across it instead of stepping.
- Knockout Shallow now stops at the group base (the backdrop
  accumulated just above the bottom rendered layer, or the
  empty group buffer inside a set); Deep still punches to
  transparency.
- Mask Hides Effects multiplies the below-effects buffer by
  the layer's mask factor; Transparency Shapes off forces
  content alpha to 1 inside the layer's painted bounds;
  Blend Clipped Layers as Group makes clipped layers blend
  with their base layer's mode. Blend Interior Effects as
  Group describes the pipeline's standing behavior (interior
  effects are composited as a group) and defaults on.
- CMYK documents evaluate shape and fill layers in ink
  space, and Perspective Crop accepts CMYK documents,
  resampling all five channels.
- The brush engine enforces the transparent-pixels lock:
  paint recolors only pixels that already carry alpha and
  never changes their alpha; erasers are inert on a
  transparent-locked layer.
- Save for Web gains Weighted Optimization
  (`io::export_weighted_jpeg`): the document is encoded at
  a high and a low JPEG quality, the two are composited per
  pixel by a weight map (the first alpha channel), and the
  composite is re-encoded at the high quality.
- Notes carry optional audio: a voice annotation stored as
  WAV bytes (16-bit mono PCM) in a trailing `.pdk` block,
  recorded and played back in the app through the system
  audio devices (cpal). Older files load with no audio.
- The Liquify preview samples the source through the
  displacement field, so the preview shows the warp as it
  will bake; the Layers panel renders a folder header row
  above each run of set members.

## Phase 6.5 additions (v0.6.5)

- The filter gallery is complete for the classic era: 99
  filters, all parameter dialogs riding one generic
  preview/commit flow, all recordable as action steps.
- All eight color modes are real document modes: RGB, CMYK,
  Grayscale, Lab (CIE D50), Indexed (with color table
  editing), Bitmap (threshold / pattern / diffusion),
  Duotone (1–4 inks with curve presets), and Multichannel
  (separation planes painted directly). Conversions flatten
  first, as the classic editor did.
- Channel arithmetic: Apply Image and Calculations composite
  any document plane (composite, channel, layer, alpha)
  into the active layer with blend modes, scaling/offset
  arithmetic, optional masks and per-channel targets; Split
  and Merge Channels round-trip through grayscale documents.
- Actions are durable: sets and actions persist between
  sessions in `~/.pictodesk/actions.txt`, every step kind
  serializes to one text line (shared with droplets and
  adjustment-settings files), playback can pause at an
  inserted Stop and can call any menu command through an
  inserted Menu Item step, and the panel has a Button Mode.
- Libraries: the swatch palette and brush preset grid are
  app state, saved and loaded as `.ACO` swatch files and a
  compact brush-preset file; adjustment dialogs save and
  load their settings through the same step-line format.
- Formats and tail: PCX and Targa export, arbitrary-angle
  canvas rotation, Fade after any filter, File Info (IPTC)
  fields stored in the document, Paths exported as an
  Illustrator-style EPS text file, the Options Bar, and
  full-screen display modes.
