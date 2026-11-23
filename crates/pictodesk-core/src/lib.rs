//! PictoDesk core engine — Phase 0.
//!
//! A UI-free library: tiled document model (layers from day one),
//! 16-bit color pipeline, blend/compositing, command history, and the
//! `.pdesk` file format with atomic autosave. The desktop UI will sit
//! on top of this; nothing in here knows a window exists.

pub mod adjust;
pub mod blend;
pub mod brush;
pub mod color;
pub mod document;
pub mod filter;
pub mod format;
pub mod histogram;
pub mod history;
pub mod icc;
pub mod mode;
pub mod path;
pub mod io;
pub mod layer;
pub mod pixel;
pub mod selection;
pub mod tile;
pub mod transform;

pub use blend::BlendMode;
pub use document::ColorMode;
pub use path::Path;
pub use color::WorkingSpace;
pub use document::Document;
pub use history::History;
pub use pixel::Pixel16;
pub use selection::Selection;
