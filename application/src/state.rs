use std::time::Duration;

use lx_core::model::lyric::LyricState;
use lx_core::model::playlist::Playlist;
use lx_core::model::song::SongInfo;
use lx_core::model::source::PlayerState;

#[derive(Debug, Clone, Default)]
pub struct ApplicationState {
    pub playback: PlaybackSnapshot,
    pub queue: QueueState,
    pub search: SearchState,
    pub lyrics: Option<LyricState>,
    pub playlists: Vec<Playlist>,
}

#[derive(Debug, Clone)]
pub struct PlaybackSnapshot {
    pub state: PlayerState,
    pub song: Option<SongInfo>,
    pub position: Duration,
    pub duration: Duration,
    pub volume: u32,
    pub generation: u64,
}

impl Default for PlaybackSnapshot {
    fn default() -> Self {
        Self {
            state: PlayerState::Idle,
            song: None,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 100,
            generation: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueState {
    pub songs: Vec<SongInfo>,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub keyword: String,
    pub source: Option<lx_core::model::source::SourceId>,
    pub page: u32,
    pub has_more: bool,
    pub items: Vec<SongInfo>,
    pub request_id: u64,
}

/// Stable frontend DTO. It deliberately uses strings instead of exposing
/// internal Rust-only enums to FFI.
#[derive(Debug, Clone)]
pub struct PlaybackStateDto {
    pub state: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u32,
    pub generation: u64,
}

impl From<&PlaybackSnapshot> for PlaybackStateDto {
    fn from(value: &PlaybackSnapshot) -> Self {
        Self {
            state: match value.state {
                PlayerState::Idle => "idle",
                PlayerState::Loading => "loading",
                PlayerState::Playing => "playing",
                PlayerState::Paused => "paused",
                PlayerState::Stopped => "stopped",
            }
            .to_string(),
            position_ms: value.position.as_millis() as u64,
            duration_ms: value.duration.as_millis() as u64,
            volume: value.volume,
            generation: value.generation,
        }
    }
}
