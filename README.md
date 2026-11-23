# PictoDesk

An open-source desktop photo and image editor — local files, no cloud,
no account, no subscription. Built feature by feature, in the open.

**Status: v0.1.2 — Phase 1 complete: a real photo editor.** The UI
follows the classic early-90s editor design: a File / Edit / Image /
Mode / Filter / Select / Window menu bar, a compact two-column
toolbox, modal dialogs with live preview — and, for now, a single
level of Undo.

What exists today:

- The core library (`crates/pictodesk-core`): a tiled document engine
  with layers in the model from day one, a 16-bit-per-channel color
  pipeline, blend modes and compositing, a command history (undo/redo),
  real ICC profile support (load any `.icc`/`.icm`, profile-to-profile
  conversion, via the pure-Rust moxcms engine), and the `.pdesk` file
  format — v2, DEFLATE-compressed, with atomic autosave/recovery
  (v1 files still load).
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
- A desktop shell (`apps/pictodesk-app`): the classic 1990-era workbench
  — platinum menu bar and toolbox, rulers, floating Brushes/Histogram
  palettes, modal dialogs with live preview, one level of Undo.
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
