//! UI 无关的 Voicefox Application Service 层。
//!
//! 这一层是 Flutter FFI 与现有 TUI 之间的稳定业务边界：
//! - 不依赖 ratatui/crossterm
//! - 不暴露 Arc/Mutex/Tokio channel 给 frontend
//! - frontend 通过 Command/Event/State DTO 与服务交互。
//!
//! 当前服务复用已有 core/source/player/lyric 能力；TUI 的旧 AppAction
//! 会在迁移期由 frontend adapter 映射到 ApplicationCommand。

pub mod event;
pub mod service;
pub mod state;

pub use event::{ApplicationCommand, ApplicationEvent, ApplicationEventKind};
pub use service::{ApplicationEventStream, ApplicationEventStreamError, ApplicationService};
pub use state::{ApplicationState, PlaybackSnapshot, QueueState, SearchState};

pub use services::{
    LyricsService, PlaybackEffects, PlaybackService, PlaylistService, QueueService, SearchService,
};

mod services;
