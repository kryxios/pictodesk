# PictoDesk

An open-source desktop photo and image editor — local files, no cloud,
no account, no subscription. Built feature by feature, in the open.

**Status: Phase 0 — engine foundation, complete.** What exists today:

- The core library (`crates/pictodesk-core`): a tiled document engine
  with layers in the model from day one, a 16-bit-per-channel color
  pipeline, blend modes and compositing, a command history (undo/redo),
  real ICC profile support (load any `.icc`/`.icm`, profile-to-profile
  conversion, via the pure-Rust moxcms engine), and the `.pdesk` file
  format — v2, DEFLATE-compressed, with atomic autosave/recovery
  (v1 files still load).
- A desktop shell (`apps/pictodesk-app`): a wgpu window displaying the
  engine's flattened render on a GPU texture — wheel zooms at the
  cursor, drag pans, `R` refits, `Esc` quits. Verify it headlessly
  with `cargo run -p pictodesk-app -- --shot out.ppm`.
- A headless harness (`apps/pictodesk-cli`) that exercises the engine
  end to end.

## Run the Phase 0 demo

```sh
cargo run -p pictodesk-cli -- ./phase0-demo
cargo run -p pictodesk-app
cargo test
```

## Layout

- `crates/pictodesk-core` — the engine. No UI dependencies, ever.
- `apps/pictodesk-cli` — headless demo/test harness for the engine.
- `apps/pictodesk-app` — the wgpu/winit desktop shell.

## License

GPL-3.0-or-later. See `LICENSE`.
