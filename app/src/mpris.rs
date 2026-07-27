//! Linux MPRIS 服务，供 Waybar、桌面媒体键和播放器控件使用。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use lx_core::model::song::SongInfo;
use lx_core::model::source::PlayerState;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, Value};

const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";

#[derive(Debug, Clone)]
pub enum MprisCommand {
    Quit,
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    SeekBy(i64),
    SetPosition(Duration),
    SetVolume(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MprisSnapshot {
    playback_status: &'static str,
    loop_status: &'static str,
    shuffle: bool,
    track_path: String,
    title: String,
    artist: String,
    album: String,
    art_url: String,
    source_url: String,
    duration_micros: i64,
    position_micros: i64,
    /// 当前进度所属的连续时间线，跳转会递增
    position_epoch: u64,
    volume: f64,
    can_go_next: bool,
    can_go_previous: bool,
}

impl Default for MprisSnapshot {
    fn default() -> Self {
        Self {
            playback_status: "Stopped",
            loop_status: "None",
            shuffle: false,
            track_path: "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art_url: String::new(),
            source_url: String::new(),
            duration_micros: 0,
            position_micros: 0,
            position_epoch: 0,
            volume: 0.8,
            can_go_next: false,
            can_go_previous: false,
        }
    }
}

impl MprisSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: PlayerState,
        song: Option<&SongInfo>,
        position: Duration,
        duration: Duration,
        volume: u32,
        play_mode: crate::playlist::mode::PlayMode,
        queue_len: usize,
        position_epoch: u64,
    ) -> Self {
        let playback_status = match state {
            PlayerState::Playing | PlayerState::Loading => "Playing",
            PlayerState::Paused => "Paused",
            PlayerState::Idle | PlayerState::Stopped => "Stopped",
        };
        let loop_status = match play_mode {
            crate::playlist::mode::PlayMode::SingleLoop => "Track",
            crate::playlist::mode::PlayMode::ListLoop => "Playlist",
            _ => "None",
        };
        let shuffle = play_mode == crate::playlist::mode::PlayMode::Random;
        let mut snapshot = Self {
            playback_status,
            loop_status,
            shuffle,
            position_micros: micros(position),
            position_epoch,
            duration_micros: micros(duration),
            volume: f64::from(volume.min(100)) / 100.0,
            can_go_next: queue_len > 1,
            can_go_previous: queue_len > 1,
            ..Self::default()
        };

        if let Some(song) = song {
            snapshot.track_path = track_path(song);
            snapshot.title = song.name.clone();
            snapshot.artist = song.singer.clone();
            snapshot.album = song.album_name.clone();
            snapshot.art_url = song.cover_url.as_deref().map_or_else(String::new, art_url);
            snapshot.source_url = song.file_path.as_deref().map_or_else(String::new, file_url);
            if snapshot.duration_micros == 0 {
                snapshot.duration_micros = micros(song.duration);
            }
        }

        snapshot
    }

    fn can_play(&self) -> bool {
        !self.title.is_empty()
    }

    fn can_seek(&self) -> bool {
        self.duration_micros > 0
    }

    fn metadata_changed(&self, other: &Self) -> bool {
        self.track_path != other.track_path
            || self.title != other.title
            || self.artist != other.artist
            || self.album != other.album
            || self.art_url != other.art_url
            || self.source_url != other.source_url
            || self.duration_micros != other.duration_micros
    }
}

#[derive(Clone)]
pub struct MprisHandle {
    update_tx: tokio::sync::mpsc::UnboundedSender<MprisSnapshot>,
}

impl MprisHandle {
    pub fn update(&self, snapshot: MprisSnapshot) {
        let _ = self.update_tx.send(snapshot);
    }
}

pub async fn start() -> anyhow::Result<(
    MprisHandle,
    tokio::sync::mpsc::UnboundedReceiver<MprisCommand>,
)> {
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (update_tx, update_rx) = tokio::sync::mpsc::unbounded_channel();
    let initial = MprisSnapshot::default();
    let root = MediaPlayer2 {
        command_tx: command_tx.clone(),
    };
    let player = MprisPlayer {
        command_tx,
        state: initial,
    };
    let bus_name = format!(
        "org.mpris.MediaPlayer2.voicefox.instance{}",
        std::process::id()
    );
    let connection = zbus::connection::Builder::session()?
        .serve_at(MPRIS_PATH, root)?
        .serve_at(MPRIS_PATH, player)?
        .name(bus_name)?
        .build()
        .await?;

    tokio::spawn(async move {
        if let Err(error) = run_updates(connection, update_rx).await {
            tracing::warn!("MPRIS update service stopped: {error}");
        }
    });

    Ok((MprisHandle { update_tx }, command_rx))
}

struct MediaPlayer2 {
    command_tx: tokio::sync::mpsc::UnboundedSender<MprisCommand>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl MediaPlayer2 {
    fn raise(&self) {}

    fn quit(&self) {
        let _ = self.command_tx.send(MprisCommand::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &'static str {
        "voicefox"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &'static str {
        "voicefox"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&'static str> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

struct MprisPlayer {
    command_tx: tokio::sync::mpsc::UnboundedSender<MprisCommand>,
    state: MprisSnapshot,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    fn next(&self) {
        let _ = self.command_tx.send(MprisCommand::Next);
    }

    fn previous(&self) {
        let _ = self.command_tx.send(MprisCommand::Previous);
    }

    fn pause(&self) {
        let _ = self.command_tx.send(MprisCommand::Pause);
    }

    fn play_pause(&self) {
        let _ = self.command_tx.send(MprisCommand::Toggle);
    }

    fn stop(&self) {
        let _ = self.command_tx.send(MprisCommand::Stop);
    }

    fn play(&self) {
        let _ = self.command_tx.send(MprisCommand::Play);
    }

    fn seek(&self, offset: i64) {
        let _ = self.command_tx.send(MprisCommand::SeekBy(offset));
    }

    fn set_position(&self, track_id: OwnedObjectPath, position: i64) {
        if track_id.as_str() != self.state.track_path || position < 0 {
            return;
        }
        if self.state.duration_micros > 0 && position > self.state.duration_micros {
            return;
        }
        let _ = self
            .command_tx
            .send(MprisCommand::SetPosition(Duration::from_micros(
                position as u64,
            )));
    }

    fn open_uri(&self, _uri: &str) {}

    #[zbus(property)]
    fn playback_status(&self) -> &'static str {
        self.state.playback_status
    }

    #[zbus(property)]
    fn loop_status(&self) -> &'static str {
        self.state.loop_status
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.state.shuffle
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'static>> {
        let mut values = HashMap::new();
        let track_path = OwnedObjectPath::try_from(self.state.track_path.clone())
            .unwrap_or_else(|_| OwnedObjectPath::try_from("/").unwrap());
        values.insert("mpris:trackid".to_string(), Value::new(track_path));
        if self.state.duration_micros > 0 {
            values.insert(
                "mpris:length".to_string(),
                Value::new(self.state.duration_micros),
            );
        }
        if !self.state.art_url.is_empty() {
            values.insert(
                "mpris:artUrl".to_string(),
                Value::new(self.state.art_url.clone()),
            );
        }
        if !self.state.title.is_empty() {
            values.insert(
                "xesam:title".to_string(),
                Value::new(self.state.title.clone()),
            );
        }
        if !self.state.artist.is_empty() {
            values.insert(
                "xesam:artist".to_string(),
                Value::new(vec![self.state.artist.clone()]),
            );
        }
        if !self.state.album.is_empty() {
            values.insert(
                "xesam:album".to_string(),
                Value::new(self.state.album.clone()),
            );
        }
        if !self.state.source_url.is_empty() {
            values.insert(
                "xesam:url".to_string(),
                Value::new(self.state.source_url.clone()),
            );
        }
        values
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.state.volume
    }

    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        let _ = self
            .command_tx
            .send(MprisCommand::SetVolume(volume.clamp(0.0, 1.0)));
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.state.position_micros
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.state.can_go_next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.state.can_go_previous
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.state.can_play()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.state.can_play()
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.state.can_seek()
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}

async fn run_updates(
    connection: zbus::Connection,
    mut update_rx: tokio::sync::mpsc::UnboundedReceiver<MprisSnapshot>,
) -> zbus::Result<()> {
    while let Some(snapshot) = update_rx.recv().await {
        let interface_ref = connection
            .object_server()
            .interface::<_, MprisPlayer>(MPRIS_PATH)
            .await?;
        let emitter = interface_ref.signal_emitter().clone();
        let mut interface = interface_ref.get_mut().await;
        let previous = interface.state.clone();
        interface.state = snapshot;

        if previous.position_epoch != interface.state.position_epoch {
            MprisPlayer::seeked(&emitter, interface.state.position_micros).await?;
        }
        if previous.playback_status != interface.state.playback_status {
            interface.playback_status_changed(&emitter).await?;
        }
        if previous.metadata_changed(&interface.state) {
            interface.metadata_changed(&emitter).await?;
        }
        if previous.volume != interface.state.volume {
            interface.volume_changed(&emitter).await?;
        }
        if previous.loop_status != interface.state.loop_status {
            interface.loop_status_changed(&emitter).await?;
        }
        if previous.shuffle != interface.state.shuffle {
            interface.shuffle_changed(&emitter).await?;
        }
        if previous.can_go_next != interface.state.can_go_next {
            interface.can_go_next_changed(&emitter).await?;
        }
        if previous.can_go_previous != interface.state.can_go_previous {
            interface.can_go_previous_changed(&emitter).await?;
        }
        if previous.can_play() != interface.state.can_play() {
            interface.can_play_changed(&emitter).await?;
            interface.can_pause_changed(&emitter).await?;
        }
        if previous.can_seek() != interface.state.can_seek() {
            interface.can_seek_changed(&emitter).await?;
        }
    }
    Ok(())
}

fn micros(duration: Duration) -> i64 {
    duration.as_micros().min(i64::MAX as u128) as i64
}

fn track_path(song: &SongInfo) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    song.source.hash(&mut hasher);
    song.id.hash(&mut hasher);
    format!("/org/mpris/MediaPlayer2/track/{}", hasher.finish())
}

fn art_url(value: &str) -> String {
    if value.starts_with('/') {
        file_url(std::path::Path::new(value))
    } else {
        value.to_string()
    }
}

fn file_url(path: &std::path::Path) -> String {
    format!("file://{}", path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::{MprisSnapshot, art_url, track_path};
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::{PlayerState, SourceId};
    use std::time::Duration;

    #[test]
    fn snapshot_exposes_track_metadata() {
        let mut song = SongInfo::new(
            "42".to_string(),
            SourceId::Bili,
            "Song".to_string(),
            "Artist".to_string(),
        );
        song.album_name = "Album".to_string();
        song.duration = Duration::from_secs(90);
        song.cover_url = Some("https://example.com/cover.jpg".to_string());
        let snapshot = MprisSnapshot::new(
            PlayerState::Playing,
            Some(&song),
            Duration::from_secs(3),
            Duration::ZERO,
            75,
            crate::playlist::mode::PlayMode::SingleLoop,
            3,
            0,
        );

        assert_eq!(snapshot.playback_status, "Playing");
        assert_eq!(snapshot.loop_status, "Track");
        assert_eq!(snapshot.duration_micros, 90_000_000);
        assert_eq!(snapshot.position_micros, 3_000_000);
        assert_eq!(snapshot.track_path, track_path(&song));
        assert!(snapshot.can_go_next);
    }

    #[test]
    fn local_cover_path_is_exposed_as_file_uri() {
        assert_eq!(art_url("/tmp/cover.jpg"), "file:///tmp/cover.jpg");
    }
}
