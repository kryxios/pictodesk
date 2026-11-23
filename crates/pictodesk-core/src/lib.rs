//! PictoDesk core engine — Phase 0.
//!
//! A UI-free library: tiled document model (layers from day one),
//! 16-bit color pipeline, blend/compositing, command history, and the
//! `.pdesk` file format with atomic autosave. The desktop UI will sit
//! on top of this; nothing in here knows a window exists.

pub mod blend;
pub mod color;
pub mod document;
pub mod format;
pub mod history;
pub mod icc;
pub mod layer;
pub mod pixel;
pub mod tile;

pub use blend::BlendMode;
pub use color::WorkingSpace;
pub use document::Document;
pub use history::History;
pub use pixel::Pixel16;
