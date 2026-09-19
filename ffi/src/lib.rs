//! Flutter <-> Rust boundary for Voicefox.
//!
//! This crate deliberately contains no TUI code and does not expose Rust
//! synchronization primitives, player backends, or source-manager internals.
//! The actual platform bootstrap is supplied by the host/runtime layer.

mod frb_generated;

use std::time::Duration;

use lx_core::model::song::SongInfo;
use voicefox_application::{
    ApplicationCommand, ApplicationEvent, ApplicationEventKind, ApplicationService,
};

pub mod api;

#[derive(Debug, Clone)]
pub struct SongDto {
    pub id: String,
    pub source: String,
    pub name: String,
    pub singer: String,
    pub album_name: String,
    pub duration_ms: u64,
    pub cover_url: Option<String>,
}

impl From<SongInfo> for SongDto {
    fn from(song: SongInfo) -> Self {
        Self {
            id: song.id,
            source: song.source.as_str().to_string(),
            name: song.name,
            singer: song.singer,
            album_name: song.album_name,
            duration_ms: song.duration.as_millis() as u64,
            cover_url: song.cover_url,
        }
    }
}

impl SongDto {
    fn into_song(self) -> Result<SongInfo, String> {
        use lx_core::model::source::SourceId;
        let source = match self.source.as_str() {
            "kw" => SourceId::Kw,
            "kg" => SourceId::Kg,
            "tx" => SourceId::Tx,
            "wy" => SourceId::Wy,
            "mg" => SourceId::Mg,
            "bili" => SourceId::Bili,
            "soda" => SourceId::Soda,
            "qianqian" => SourceId::Qianqian,
            "joox" => SourceId::Joox,
            "jamendo" => SourceId::Jamendo,
            "fivesing" => SourceId::Fivesing,
            "apple" => SourceId::Apple,
            "local" => SourceId::Local,
            other => return Err(format!("unknown source: {other}")),
        };
        let mut song = SongInfo::new(self.id, source, self.name, self.singer);
        song.album_name = self.album_name;
        song.duration = Duration::from_millis(self.duration_ms);
        song.cover_url = self.cover_url;
        Ok(song)
    }
}

pub struct VoicefoxState {
    pub playback: PlaybackDto,
    pub queue: Vec<SongDto>,
    pub queue_index: u32,
    pub search: SearchDto,
    pub lyrics: LyricsDto,
    pub playlists: Vec<PlaylistDto>,
}

#[derive(Debug, Clone)]
pub struct SearchDto {
    pub keyword: String,
    pub page: u32,
    pub has_more: bool,
    pub items: Vec<SongDto>,
    pub request_id: u64,
}

#[derive(Debug, Clone)]
pub struct LyricLineDto {
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct LyricsDto {
    pub current_line: u32,
    pub position_ms: u64,
    pub lines: Vec<LyricLineDto>,
    pub translation: Option<String>,
    pub is_empty: bool,
}

#[derive(Debug, Clone)]
pub struct PlaylistDto {
    pub id: String,
    pub name: String,
    pub source: String,
    pub cover_url: Option<String>,
    pub song_count: u32,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaybackDto {
    pub state: String,
    pub song: Option<SongDto>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: u32,
    pub generation: u64,
}

impl From<voicefox_application::ApplicationState> for VoicefoxState {
    fn from(value: voicefox_application::ApplicationState) -> Self {
        let lyrics = value
            .lyrics
            .as_ref()
            .map(|state| LyricsDto {
                current_line: state.current_line as u32,
                position_ms: state.position_ms,
                lines: state
                    .lines
                    .iter()
                    .map(|line| LyricLineDto {
                        timestamp_ms: line.timestamp,
                        duration_ms: line.duration,
                        text: line.text.clone(),
                    })
                    .collect(),
                translation: state.translation.clone(),
                is_empty: state.is_empty,
            })
            .unwrap_or_default();
        Self {
            playback: PlaybackDto {
                state: format!("{:?}", value.playback.state).to_lowercase(),
                song: value.playback.song.map(Into::into),
                position_ms: value.playback.position.as_millis() as u64,
                duration_ms: value.playback.duration.as_millis() as u64,
                volume: value.playback.volume,
                generation: value.playback.generation,
            },
            queue: value.queue.songs.into_iter().map(Into::into).collect(),
            queue_index: value.queue.index as u32,
            search: SearchDto {
                keyword: value.search.keyword,
                page: value.search.page,
                has_more: value.search.has_more,
                items: value.search.items.into_iter().map(Into::into).collect(),
                request_id: value.search.request_id,
            },
            lyrics,
            playlists: value
                .playlists
                .into_iter()
                .map(|playlist| PlaylistDto {
                    id: playlist.id,
                    name: playlist.name,
                    source: playlist.source.as_str().to_string(),
                    cover_url: playlist.cover_url,
                    song_count: playlist.song_count,
                    description: playlist.description,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoicefoxEvent {
    pub kind: String,
    pub request_id: u64,
    pub keyword: Option<String>,
    pub page: u32,
    pub append: bool,
    pub has_more: bool,
    pub message: Option<String>,
    pub song: Option<SongDto>,
    pub items: Vec<SongDto>,
    pub queue: Vec<SongDto>,
    pub queue_index: u32,
}

impl From<ApplicationEvent> for VoicefoxEvent {
    fn from(value: ApplicationEvent) -> Self {
        let kind = match value.kind() {
            ApplicationEventKind::StateChanged => "state_changed",
            ApplicationEventKind::Search => "search",
            ApplicationEventKind::Playback => "playback",
            ApplicationEventKind::Queue => "queue",
            ApplicationEventKind::Lyrics => "lyrics",
            ApplicationEventKind::Playlist => "playlist",
            ApplicationEventKind::Notification => "notification",
        }
        .to_string();
        let empty = || Self {
            kind: kind.clone(),
            request_id: 0,
            keyword: None,
            page: 0,
            append: false,
            has_more: false,
            message: None,
            song: None,
            items: vec![],
            queue: vec![],
            queue_index: 0,
        };
        match value {
            ApplicationEvent::SearchStarted {
                request_id,
                keyword,
            } => Self {
                kind,
                request_id,
                keyword: Some(keyword),
                page: 1,
                append: false,
                has_more: false,
                message: None,
                song: None,
                items: vec![],
                queue: vec![],
                queue_index: 0,
            },
            ApplicationEvent::SearchCompleted {
                request_id,
                keyword,
                page,
                append,
                items,
                has_more,
            } => Self {
                kind,
                request_id,
                keyword: Some(keyword),
                page,
                append,
                has_more,
                message: None,
                song: None,
                items: items.into_iter().map(Into::into).collect(),
                queue: vec![],
                queue_index: 0,
            },
            ApplicationEvent::SearchFailed { request_id, error }
            | ApplicationEvent::PlaybackFailed { request_id, error } => {
                let mut event = empty();
                event.kind = kind;
                event.request_id = request_id;
                event.message = Some(error);
                event
            }
            ApplicationEvent::PlaybackStarted {
                request_id, song, ..
            } => Self {
                kind,
                request_id,
                keyword: None,
                page: 0,
                append: false,
                has_more: false,
                message: None,
                song: Some(song.into()),
                items: vec![],
                queue: vec![],
                queue_index: 0,
            },
            ApplicationEvent::QueueChanged { queue, index } => Self {
                kind,
                request_id: 0,
                keyword: None,
                page: 0,
                append: false,
                has_more: false,
                message: None,
                song: None,
                items: vec![],
                queue: queue.into_iter().map(Into::into).collect(),
                queue_index: index as u32,
            },
            ApplicationEvent::Notification { message } => {
                let mut event = empty();
                event.kind = kind;
                event.message = Some(message);
                event
            }
            _ => empty(),
        }
    }
}

/// Thread-safe application handle owned by the Flutter-facing layer.
#[derive(Clone)]
pub struct VoicefoxController {
    application: ApplicationService,
}

impl VoicefoxController {
    /// Construct the desktop runtime without depending on the TUI crate.
    #[cfg(feature = "desktop-mpv")]
    pub fn new_desktop() -> Result<Self, String> {
        let (config, path) = voicefox_runtime::load_config().map_err(|e| e.to_string())?;
        let runtime = voicefox_runtime::ApplicationRuntime::desktop(config, path)
            .map_err(|e| e.to_string())?;
        Ok(Self::from_application(runtime.application()))
    }

    /// Host bootstrap supplies the already-configured ApplicationService.
    /// This keeps libmpv/native audio selection outside the Flutter ABI.
    pub fn from_application(application: ApplicationService) -> Self {
        Self { application }
    }

    pub fn state(&self) -> VoicefoxState {
        self.application.state().into()
    }

    pub async fn dispatch(&self, command: FfiCommand) -> Result<(), String> {
        self.application.dispatch(command.into_application()?).await
    }

    pub fn subscribe(&self) -> VoicefoxEventStream {
        VoicefoxEventStream {
            inner: self.application.subscribe(),
        }
    }
}

pub struct VoicefoxEventStream {
    inner: voicefox_application::ApplicationEventStream,
}

impl VoicefoxEventStream {
    pub async fn next(&mut self) -> Result<VoicefoxEvent, String> {
        self.inner
            .recv()
            .await
            .map(Into::into)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum FfiCommand {
    Pause,
    Resume,
    Toggle,
    Stop,
    SeekMs(u64),
    PlaySong { song: SongDto },
    QueueAdd { song: SongDto, next: bool },
    QueueRemove { index: u32 },
    QueueClear,
    Next,
    Previous,
    Search { keyword: String },
    SearchMore { keyword: String, page: u32 },
}

impl FfiCommand {
    fn into_application(self) -> Result<ApplicationCommand, String> {
        Ok(match self {
            FfiCommand::Pause => ApplicationCommand::Pause,
            FfiCommand::Resume => ApplicationCommand::Resume,
            FfiCommand::Toggle => ApplicationCommand::Toggle,
            FfiCommand::Stop => ApplicationCommand::Stop,
            FfiCommand::SeekMs(ms) => ApplicationCommand::Seek(Duration::from_millis(ms)),
            FfiCommand::PlaySong { song } => ApplicationCommand::Play {
                songs: vec![song.into_song()?],
                index: 0,
            },
            FfiCommand::QueueAdd { song, next } => ApplicationCommand::QueueAdd {
                song: song.into_song()?,
                next,
            },
            FfiCommand::QueueRemove { index } => ApplicationCommand::QueueRemove {
                index: index as usize,
            },
            FfiCommand::QueueClear => ApplicationCommand::QueueClear,
            FfiCommand::Next => ApplicationCommand::Next,
            FfiCommand::Previous => ApplicationCommand::Previous,
            FfiCommand::Search { keyword } => ApplicationCommand::Search {
                keyword,
                source: None,
            },
            FfiCommand::SearchMore { keyword, page } => ApplicationCommand::SearchMore {
                keyword,
                page,
                source: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicefox_application::ApplicationEvent;

    #[test]
    fn playback_event_maps_to_stable_ffi_shape() {
        let event = VoicefoxEvent::from(ApplicationEvent::PlaybackStarted {
            request_id: 42,
            song: SongInfo::new(
                "test".into(),
                lx_core::model::source::SourceId::Kw,
                "Test".into(),
                "Artist".into(),
            ),
            source_index: Some(3),
        });
        assert_eq!(event.kind, "playback");
        assert_eq!(event.request_id, 42);
        assert!(event.song.is_some());
    }

    #[test]
    fn command_mapping_does_not_expose_application_types() {
        let command = FfiCommand::SeekMs(1_250);
        assert!(
            matches!(command.into_application().unwrap(), ApplicationCommand::Seek(d) if d == Duration::from_millis(1_250))
        );
    }
}
