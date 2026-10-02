//! 平台媒体会话的共享数据层。
//!
//! Linux 走 MPRIS（[`crate::mpris`]）、Windows 走 SMTC（[`crate::smtc`]），
//! 两个后端只做"协议翻译"：往桌面/系统方向推送 [`MediaSnapshot`]，
//! 往播放器方向回传 [`MediaCommand`]。快照与命令本身是平台无关的纯数据，
//! 集中在这里让主循环的接入点只有一套。

use std::hash::{Hash, Hasher};
use std::time::Duration;

use lx_core::model::song::SongInfo;
use lx_core::model::source::PlayerState;

/// 桌面媒体控件（媒体键 / 系统浮层）请求播放器执行的动作。
///
/// Linux MPRIS 会产生全部变体；Windows SMTC 不会产生 `Quit` / `SetVolume`。
/// 保留同一份命令集，主循环的执行逻辑才能只有一份。
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum MediaCommand {
    Quit,
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    /// 相对跳转，单位微秒（负值后退）。
    SeekBy(i64),
    SetPosition(Duration),
    SetVolume(f64),
}

/// 一次媒体状态的完整快照；后端自行对比字段决定要更新哪些协议属性。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSnapshot {
    pub playback_status: &'static str,
    pub loop_status: &'static str,
    pub shuffle: bool,
    /// 曲目标识。MPRIS 用它当 D-Bus 对象路径，SMTC 只当变更检测的令牌。
    pub track_path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    pub source_url: String,
    pub duration_micros: i64,
    pub position_micros: i64,
    /// 当前进度所属的连续时间线，跳转会递增。
    pub position_epoch: u64,
    pub volume: f64,
    pub can_go_next: bool,
    pub can_go_previous: bool,
}

impl Default for MediaSnapshot {
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

impl MediaSnapshot {
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
            // 队列只有一首时列表循环仍可“下一首”，因此只要在播放就报告可用
            can_go_next: queue_len > 0,
            can_go_previous: queue_len > 0,
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

    /// 是否有可播放的曲目（MPRIS 的 can-play 属性）。
    #[allow(dead_code)]
    pub fn can_play(&self) -> bool {
        !self.title.is_empty()
    }

    /// 曲目是否可跳转进度（MPRIS 的 can-seek 属性）。
    #[allow(dead_code)]
    pub fn can_seek(&self) -> bool {
        self.duration_micros > 0
    }

    /// 歌曲本身是否发生了变化（不含进度 / 音量这类播放态）。
    pub fn metadata_changed(&self, other: &Self) -> bool {
        self.track_path != other.track_path
            || self.title != other.title
            || self.artist != other.artist
            || self.album != other.album
            || self.art_url != other.art_url
            || self.source_url != other.source_url
            || self.duration_micros != other.duration_micros
    }
}

/// 向后端推送快照的入口；后端内部用通道转到自己的服务线程。
#[derive(Clone)]
pub struct MediaHandle {
    update_tx: tokio::sync::mpsc::UnboundedSender<MediaSnapshot>,
}

impl MediaHandle {
    pub fn update(&self, snapshot: MediaSnapshot) {
        let _ = self.update_tx.send(snapshot);
    }
}

/// 新建一个 handle + 配套的更新接收端，供后端启动函数装配。
pub fn channel() -> (
    MediaHandle,
    tokio::sync::mpsc::UnboundedReceiver<MediaSnapshot>,
) {
    let (update_tx, update_rx) = tokio::sync::mpsc::unbounded_channel();
    (MediaHandle { update_tx }, update_rx)
}

pub fn micros(duration: Duration) -> i64 {
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
    use super::{MediaSnapshot, art_url, track_path};
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
        let snapshot = MediaSnapshot::new(
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
