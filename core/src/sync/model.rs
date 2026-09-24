use crate::model::song::SongInfo;
use crate::model::source::SourceId;
use serde::{Deserialize, Serialize};

/// 歌单与“我喜欢”共用的同步对象类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SyncCollectionKind {
    Playlist,
    Favorites,
}

/// 跨音源比较用的规范化歌曲身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalSong {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub isrc: Option<String>,
}
impl CanonicalSong {
    pub fn from_song(song: &SongInfo) -> Self {
        Self {
            title: normalize_text(&song.name),
            artist: normalize_artist(&song.singer),
            album: normalize_text(&song.album_name),
            duration_ms: song.duration.as_millis().try_into().unwrap_or(u64::MAX),
            isrc: song
                .extra
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("isrc"))
                .map(|(_, value)| value.trim().to_ascii_uppercase().replace(['-', ' '], ""))
                .filter(|value| !value.is_empty()),
        }
    }
}

/// 源平台 + 平台 ID；仅在同平台匹配时使用。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlatformSongId {
    pub source: SourceId,
    pub id: String,
}
impl PlatformSongId {
    pub fn from_song(song: &SongInfo) -> Self {
        Self {
            source: song.source,
            id: song.id.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchConfidence {
    Isrc,
    PlatformId,
    ExactMetadata,
    MetadataDuration,
    FuzzyMetadata,
    Unmatched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSongMatch {
    pub source: SongInfo,
    pub target: Option<SongInfo>,
    pub confidence: MatchConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCollection {
    pub kind: SyncCollectionKind,
    pub id: String,
    pub name: String,
    pub source: SourceId,
    pub songs: Vec<SongInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncDirection {
    SourceToTarget,
    TargetToSource,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncPolicy {
    Additive,
    Mirror,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOptions {
    pub direction: SyncDirection,
    pub policy: SyncPolicy,
    pub match_duration_tolerance_ms: u64,
    pub fuzzy_threshold: u16,
    pub batch_size: usize,
}
impl Default for SyncOptions {
    fn default() -> Self {
        Self {
            direction: SyncDirection::SourceToTarget,
            policy: SyncPolicy::Additive,
            match_duration_tolerance_ms: 5_000,
            fuzzy_threshold: 92,
            batch_size: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPlan {
    pub source: SyncCollection,
    pub target: SyncCollection,
    pub matches: Vec<SyncSongMatch>,
    pub additions: Vec<SongInfo>,
    pub removals: Vec<SongInfo>,
    pub unmatched: Vec<SongInfo>,
}
impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub source: SourceId,
    pub target: SourceId,
    pub collection_name: String,
    pub added: usize,
    pub removed: usize,
    pub already_present: usize,
    pub unmatched: usize,
    pub failed: Vec<SyncFailure>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFailure {
    pub song: String,
    pub artist: String,
    pub reason: String,
}

pub(crate) fn normalize_text(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && !matches!(ch, '　' | '·' | '•' | '・'))
        .map(|ch| match ch {
            '（' | '）' | '【' | '】' | '[' | ']' => ' ',
            '－' | '–' | '—' | '−' => '-',
            '“' | '”' | '‘' | '’' => '\'',
            _ => ch,
        })
        .collect()
}
pub(crate) fn normalize_artist(value: &str) -> String {
    let mut artists = value
        .split(['、', ',', '&', '/', '|', '；', ';'])
        .map(normalize_text)
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>();
    artists.sort();
    artists.dedup();
    artists.join("&")
}
