//! TUI 封面：渲染职责归 app，获取/缓存职责归 runtime。

mod layout;
mod render;

pub use layout::CoverGeometry;
pub use render::CoverRenderer;
pub use voicefox_runtime::{CoverService, CoverState, sweep_temp_files};
