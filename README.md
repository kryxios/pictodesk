# PictoDesk

An open-source desktop photo and image editor — local files, no cloud,
no account, no subscription. Built feature by feature, in the open.

**Status: v0.5.0 — Phase 5: the professional tier. A full History palette with snapshots and a History Brush, editable type layers, layer effects, the complete classic filter gallery, PSD export, color management, Automate, printing, and Save for Web. Public 1.0 feature set.**
The UI follows the classic early-90s editor design: a File / Edit /
Image / Mode / Filter / Select / Window menu bar, a compact
two-column toolbox, modal dialogs with live preview, docked tabbed
palettes — and, for now, a single level of Undo.

What exists today:

- The core library (`crates/pictodesk-core`): a tiled document engine
  with layers in the model from day one, a 16-bit-per-channel color
  pipeline, blend modes and compositing, a command history (undo/redo),
  real ICC profile support (load any `.icc`/`.icm`, profile-to-profile
  conversion, via the pure-Rust moxcms engine), and the `.pdk` file
  format — v3, DEFLATE-compressed, with atomic autosave/recovery
  (v1/v2 files still load).
- Phase 1 editing, all selection-aware and undoable: Open/Save PNG,
  JPEG, TIFF, BMP (embedded ICC profiles honored on import);
  selections (Rectangular/Elliptical Marquee, Lasso, Magic Wand,
  Feather, Inverse); painting tools (Brush, Pencil, Eraser, Clone
  Stamp, Paint Bucket, Gradient, Blur/Sharpen/Smudge); the full
  Image > Adjust set (Brightness/Contrast, Hue/Saturation, Color
  Balance, Levels, Curves, Invert, Desaturate, Posterize, Threshold);
  Filters (Gaussian Blur, Sharpen, Unsharp Mask, Add Noise);
  Image Size, Canvas Size, Crop, Rotate, Flip; a Histogram palette.
- **Windows, Linux, and macOS** from one codebase (winit + wgpu —
  no platform-specific code). Keyboard shortcuts follow each
  platform's convention: Ctrl on Windows/Linux, Cmd on macOS, and
  the menus label them accordingly.
- Phase 2: the Pen tool (click for corner points, drag for smooth
  points, click the first anchor to close) with a Paths panel — save
  named paths, fill/stroke them, convert path <-> selection, and mark
  a clipping path; a Channels panel with per-channel viewing, alpha
  channels (saved selections) you can paint on directly, and basic
  spot channels; real RGB / CMYK / Grayscale modes — a CMYK
  document stores actual ink values (plus a per-pixel alpha plane),
  and painting, adjustments, and filters all work in ink space;
  spot channels export as their own plates in a Separation TIFF;
  selection-to-path uses real Bézier curve fitting, and pen/path
  edits are undoable like everything else; Select > Grow and
  Similar; 16-bit files now import bit-exact, and an embedded ICC
  profile on a 16-bit file is converted into the working space at
  full 16-bit precision, same as 8-bit files; CMYK documents save
  as true CMYK TIFFs with the clipping path embedded. The
  `.pdk` format is now v3 (v1/v2 files still load).
- Phase 3: layers — a Layer menu and a Layers panel with live
  thumbnails: new / duplicate / delete / reorder layers, opacity,
  12 blend modes, linking (linked layers move and transform
  together), Merge Down / Merge Visible / Flatten. Free Transform
  (Ctrl+T) works on the canvas: drag the bounding box to move,
  handles to scale, outside to rotate, Ctrl+drag to skew, with a
  live preview — Enter applies, Esc cancels. PSD and PSB files
  import completely: 1/8/16-bit, Bitmap / Grayscale / Indexed /
  RGB / CMYK / Multichannel, raw, PackBits and ZIP channel data,
  layer masks (baked into transparency and kept as alpha
  channels); anything that cannot come across (groups as
  structure, type/effects layers) is reported, never silently
  dropped. The Layers panel scrolls (mouse wheel, footer buttons)
  and always keeps the active layer visible. The app autosaves a
  recovery copy of unsaved work every 20 seconds and offers it
  back after a crash. Like the classic editors, `.pdk` files
  carry a provenance history log — created / opened / saved and
  every edit's label, with timestamps — viewable any time via
  File > History Log. (The undo stack itself stays session-only,
  same as the classics.)
- Phase 4: non-destructive editing — adjustment layers (Levels,
  Curves, Brightness/Contrast, Hue/Saturation, Color Balance,
  Photo Filter) that stay editable forever and apply live to
  everything beneath them; layer masks on any layer (paint to
  hide/reveal, feather, disable without deleting); merging a
  masked or adjustment layer bakes the pair's render, so the
  picture itself never changes; an Actions palette that records
  a sequence of edits and replays it on another image, plus
  batch processing that runs an action over a whole folder of
  images (originals untouched — results land as
  `<name>-edited.png`); a Navigator palette; guides dragged out
  of the rulers, a grid, and snapping; and right-click context
  menus. Masks, adjustment settings, and guides all save in the
  `.pdk` file.
- Phase 4.5 (gap pass): the blend-mode set completes to the full
  classic 17 (Dissolve — a stable per-pixel hash gate in the
  compositor — Hard Light, Color Dodge, Color Burn, Exclusion);
  the Dodge, Burn, and Sponge toning tools (range and exposure
  live in the Brushes palette, O cycles them); Select > Color
  Range (samples the foreground color, graduated fuzziness) and
  Select > Modify (Border, Smooth, Expand, Contract, via a
  chamfer distance transform of the mask); Image > Adjust >
  Equalize; twelve more filters in the classic grouped Filter
  menu — Motion and Radial Blur, Despeckle, Median, Mosaic,
  Clouds (deterministic fractal noise between the foreground
  and background colors), Emboss, Find Edges, Solarize, High
  Pass, Offset, and a Custom 5x5 kernel editor; Edit > Copy
  Merged and Paste Into; and Bring to Front / Send to Back
  completing Layer > Arrange. Every new filter records and
  replays in Actions like the rest.
- Phase 5 (the professional tier): a History palette with
  multiple undo/redo, snapshots, a History Brush and Art
  History Brush, Fill with History, and Erase to History;
  editable type layers (system fonts, real text shaping,
  per-range styling, horizontal and vertical type) that stay
  editable forever; layer effects — Drop Shadow, Inner Shadow,
  Outer Glow, Inner Glow, Bevel and Emboss — live on the layer
  with copy/paste and a shared global light angle; the rest of
  the classic toolbox (Polygonal and Magnetic Lasso, Freeform
  and Magnetic Pen, Pattern Stamp, Magic and Background
  Erasers, Color Sampler, Measure tool, Info palette); the
  filter gallery completed — the Distort family (Pinch,
  Spherize, Twirl, Ripple, Ocean Ripple, Shear, ZigZag, Polar
  Coordinates), 3D Transform, Extract, and the remaining
  Noise/Pixelate/Sketch/Stylize/Texture filters; full Free
  Transform with Distort and Perspective corner drags,
  Transform Selection, and Transform Again; Channel Mixer and
  hue-range Hue/Saturation; working-space color management
  (Assign/Convert Profile, CMYK soft proofing, the web-safe
  color warning); Actions organized into sets; File > Automate
  (Batch, Conditional Mode Change, Contact Sheet II, Fit Image,
  Picture Package, Web Photo Gallery); layered PSD export and
  EPS DCS 2.0 separations; Page Setup and printing; and Save
  for Web with live encoded sizes (JPEG/PNG/GIF).
- A desktop shell (`apps/pictodesk-app`): the classic 1990-era workbench
  — platinum menu bar and toolbox, rulers, docked tabbed palettes
  (Channels/Paths, Brushes, Histogram, Color), modal dialogs with
  live preview, and a History palette with multiple undo.
  Open a file with `pictodesk-app photo.png`, or drag a file onto the
  window. Verify the UI headlessly with
  `cargo run -p pictodesk-app -- --shot out.ppm [--menu N] [--dialog levels]`.
- A headless harness (`apps/pictodesk-cli`) that exercises the engine
  end to end.

## Run it

```sh
cargo run -p pictodesk-app -- photo.png
cargo run -p pictodesk-cli -- ./phase0-demo
cargo test
```

## Layout

- `crates/pictodesk-core` — the engine. No UI dependencies, ever.
- `apps/pictodesk-cli` — headless demo/test harness for the engine.
- `apps/pictodesk-app` — the wgpu/winit desktop shell.

## License

GPL-3.0-or-later. See `LICENSE`.
