//! JSON 文件存储 — 歌曲/歌单收藏、播放历史和播放会话

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use lx_core::model::playlist::Playlist;
use lx_core::model::song::SongInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SavedPlayerState {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub playlist: Vec<SongInfo>,
    pub current_index: usize,
    pub position: Duration,
    pub state: SavedPlayerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPlaylist {
    pub id: String,
    pub name: String,
    pub songs: Vec<SongInfo>,
    pub created_at_unix_nanos: u128,
    pub updated_at_unix_nanos: u128,
}

#[derive(Debug, Clone)]
pub struct CustomPlaylistSummary {
    pub id: String,
    pub name: String,
    pub cover_url: Option<String>,
    pub song_count: u32,
}

pub struct Storage {
    data_dir: PathBuf,
    favorites: RwLock<Vec<SongInfo>>,
    favorite_playlists: RwLock<Vec<Playlist>>,
    custom_playlists: RwLock<Vec<CustomPlaylist>>,
    history: RwLock<Vec<SongInfo>>,
}

impl Storage {
    pub fn new() -> Self {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("voicefox")
            .join("data");
        fs::create_dir_all(&dir).ok();
        let favorites = Self::load_file(&dir.join("favorites.json"));
        let favorite_playlists = Self::load_file(&dir.join("favorite_playlists.json"));
        let custom_playlists = Self::load_file(&dir.join("custom_playlists.json"));
        let history = Self::load_file(&dir.join("history.json"));
        Self {
            data_dir: dir,
            favorites: RwLock::new(favorites),
            favorite_playlists: RwLock::new(favorite_playlists),
            custom_playlists: RwLock::new(custom_playlists),
            history: RwLock::new(history),
        }
    }

    // ── 收藏 ──────────────────────────────────────────

    pub fn add_favorite(&self, song: &SongInfo) -> bool {
        let mut favs = self.favorites.write().unwrap();
        if favs.iter().any(|favorite| songs_equivalent(favorite, song)) {
            return false;
        }
        favs.push(song.clone());
        self.save_favorites(&favs);
        true
    }

    pub fn remove_favorite(&self, song: &SongInfo) -> bool {
        let mut favs = self.favorites.write().unwrap();
        let old_len = favs.len();
        favs.retain(|favorite| !songs_equivalent(favorite, song));
        if favs.len() != old_len {
            self.save_favorites(&favs);
            return true;
        }
        false
    }

    pub fn is_favorite(&self, song: &SongInfo) -> bool {
        self.favorites
            .read()
            .unwrap()
            .iter()
            .any(|favorite| songs_equivalent(favorite, song))
    }

    // ── 歌单收藏 ──────────────────────────────────────

    pub fn add_favorite_playlist(&self, playlist: &Playlist) -> bool {
        let mut favorites = self.favorite_playlists.write().unwrap();
        if favorites
            .iter()
            .any(|favorite| favorite.id == playlist.id && favorite.source == playlist.source)
        {
            return false;
        }
        favorites.push(playlist.clone());
        self.save_favorite_playlists(&favorites);
        true
    }

    pub fn remove_favorite_playlist(&self, playlist: &Playlist) -> bool {
        let mut favorites = self.favorite_playlists.write().unwrap();
        let old_len = favorites.len();
        favorites
            .retain(|favorite| favorite.id != playlist.id || favorite.source != playlist.source);
        if favorites.len() == old_len {
            return false;
        }
        self.save_favorite_playlists(&favorites);
        true
    }

    pub fn is_favorite_playlist(&self, playlist: &Playlist) -> bool {
        self.favorite_playlists
            .read()
            .unwrap()
            .iter()
            .any(|favorite| favorite.id == playlist.id && favorite.source == playlist.source)
    }

    pub fn load_favorite_playlists(&self) -> Vec<Playlist> {
        self.favorite_playlists.read().unwrap().clone()
    }

    // ── 自定义歌单 ────────────────────────────────────

    pub fn custom_playlist_summaries(&self) -> Vec<CustomPlaylistSummary> {
        self.custom_playlists
            .read()
            .unwrap()
            .iter()
            .map(|playlist| CustomPlaylistSummary {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                cover_url: playlist
                    .songs
                    .iter()
                    .find_map(|song| song.cover_url.clone()),
                song_count: u32::try_from(playlist.songs.len()).unwrap_or(u32::MAX),
            })
            .collect()
    }

    pub fn custom_playlist_choices(&self) -> Vec<(String, String)> {
        self.custom_playlists
            .read()
            .unwrap()
            .iter()
            .map(|playlist| (playlist.id.clone(), playlist.name.clone()))
            .collect()
    }

    pub fn custom_playlist(&self, playlist_id: &str) -> Option<CustomPlaylist> {
        self.custom_playlists
            .read()
            .unwrap()
            .iter()
            .find(|playlist| playlist.id == playlist_id)
            .cloned()
    }

    pub fn create_custom_playlist(&self, name: &str) -> Result<CustomPlaylist, String> {
        let name = validate_custom_playlist_name(name)?;
        self.update_custom_playlists(|playlists| {
            ensure_unique_custom_playlist_name(playlists, &name, None)?;
            let now = unix_nanos();
            let playlist = CustomPlaylist {
                id: format!("custom-{now}-{}", std::process::id()),
                name,
                songs: Vec::new(),
                created_at_unix_nanos: now,
                updated_at_unix_nanos: now,
            };
            playlists.push(playlist.clone());
            Ok(playlist)
        })
    }

    pub fn rename_custom_playlist(&self, playlist_id: &str, name: &str) -> Result<(), String> {
        let name = validate_custom_playlist_name(name)?;
        self.update_custom_playlists(|playlists| {
            ensure_unique_custom_playlist_name(playlists, &name, Some(playlist_id))?;
            let playlist = playlists
                .iter_mut()
                .find(|playlist| playlist.id == playlist_id)
                .ok_or_else(|| "自定义歌单不存在".to_string())?;
            playlist.name = name;
            playlist.updated_at_unix_nanos = unix_nanos();
            Ok(())
        })
    }

    pub fn delete_custom_playlist(&self, playlist_id: &str) -> Result<bool, String> {
        self.update_custom_playlists(|playlists| {
            let old_len = playlists.len();
            playlists.retain(|playlist| playlist.id != playlist_id);
            Ok(playlists.len() != old_len)
        })
    }

    pub fn add_song_to_custom_playlist(
        &self,
        playlist_id: &str,
        song: &SongInfo,
    ) -> Result<bool, String> {
        self.update_custom_playlists(|playlists| {
            let playlist = playlists
                .iter_mut()
                .find(|playlist| playlist.id == playlist_id)
                .ok_or_else(|| "自定义歌单不存在".to_string())?;
            if playlist
                .songs
                .iter()
                .any(|item| same_song_identity(item, song))
            {
                return Ok(false);
            }
            playlist.songs.push(song.clone());
            playlist.updated_at_unix_nanos = unix_nanos();
            Ok(true)
        })
    }

    pub fn remove_song_from_custom_playlist(
        &self,
        playlist_id: &str,
        song: &SongInfo,
    ) -> Result<bool, String> {
        self.update_custom_playlists(|playlists| {
            let playlist = playlists
                .iter_mut()
                .find(|playlist| playlist.id == playlist_id)
                .ok_or_else(|| "自定义歌单不存在".to_string())?;
            let old_len = playlist.songs.len();
            playlist
                .songs
                .retain(|item| !same_song_identity(item, song));
            if playlist.songs.len() == old_len {
                return Ok(false);
            }
            playlist.updated_at_unix_nanos = unix_nanos();
            Ok(true)
        })
    }

    pub fn remove_local_path_from_custom_playlists(&self, path: &Path) -> Result<usize, String> {
        self.update_custom_playlists(|playlists| {
            let mut removed = 0;
            for playlist in playlists {
                let old_len = playlist.songs.len();
                playlist
                    .songs
                    .retain(|song| !local_song_matches_path(song, path));
                let removed_from_playlist = old_len.saturating_sub(playlist.songs.len());
                if removed_from_playlist > 0 {
                    removed += removed_from_playlist;
                    playlist.updated_at_unix_nanos = unix_nanos();
                }
            }
            Ok(removed)
        })
    }

    fn update_custom_playlists<T>(
        &self,
        update: impl FnOnce(&mut Vec<CustomPlaylist>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut current = self.custom_playlists.write().unwrap();
        let mut updated = current.clone();
        let result = update(&mut updated)?;
        let json = serde_json::to_vec_pretty(&updated).map_err(|error| error.to_string())?;
        save_atomic(&self.data_dir.join("custom_playlists.json"), &json)?;
        *current = updated;
        Ok(result)
    }

    // ── 播放历史 ──────────────────────────────────────

    pub fn add_history(&self, song: &SongInfo, limit: usize) {
        let mut history = self.history.write().unwrap();
        history.retain(|s| !(s.id == song.id && s.source == song.source));
        history.insert(0, song.clone());
        history.truncate(limit.max(1));
        self.save_history(&history);
    }

    pub fn remove_history(&self, song: &SongInfo) -> bool {
        let mut history = self.history.write().unwrap();
        let old_len = history.len();
        history.retain(|item| !(item.id == song.id && item.source == song.source));
        if history.len() == old_len {
            return false;
        }
        self.save_history(&history);
        true
    }

    pub fn clear_history(&self) -> bool {
        let mut history = self.history.write().unwrap();
        if history.is_empty() {
            return false;
        }
        history.clear();
        self.save_history(&history);
        true
    }

    pub fn trim_history(&self, limit: usize) -> bool {
        let mut history = self.history.write().unwrap();
        let old_len = history.len();
        history.truncate(limit.max(1));
        if history.len() == old_len {
            return false;
        }
        self.save_history(&history);
        true
    }

    // ── 内部序列化 ────────────────────────────────────

    pub fn load_favorites(&self) -> Vec<SongInfo> {
        self.favorites.read().unwrap().clone()
    }

    fn save_favorites(&self, favs: &[SongInfo]) {
        self.save_file("favorites.json", favs);
    }

    fn save_favorite_playlists(&self, playlists: &[Playlist]) {
        self.save_file("favorite_playlists.json", playlists);
    }

    pub fn load_history(&self) -> Vec<SongInfo> {
        self.history.read().unwrap().clone()
    }

    // ── 播放会话 ──────────────────────────────────────

    pub fn load_playback_session(&self) -> Option<PlaybackSession> {
        let path = self.data_dir.join("playback_state.json");
        let json = fs::read_to_string(path).ok()?;
        let mut session = serde_json::from_str::<PlaybackSession>(&json).ok()?;
        if session.playlist.is_empty() {
            return None;
        }
        session.current_index = session
            .current_index
            .min(session.playlist.len().saturating_sub(1));
        Some(session)
    }

    pub fn save_playback_session(&self, session: &PlaybackSession) -> Result<(), String> {
        let path = self.data_dir.join("playback_state.json");
        let json = serde_json::to_vec_pretty(session).map_err(|error| error.to_string())?;
        save_atomic(&path, &json)
    }

    pub fn clear_playback_session(&self) -> Result<(), String> {
        let path = self.data_dir.join("playback_state.json");
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn load_file<T: DeserializeOwned>(path: &std::path::Path) -> Vec<T> {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => vec![],
            }
        } else {
            vec![]
        }
    }

    fn save_history(&self, history: &[SongInfo]) {
        self.save_file("history.json", history);
    }

    fn save_file<T: Serialize + ?Sized>(&self, file_name: &str, value: &T) {
        let path = self.data_dir.join(file_name);
        if let Ok(json) = serde_json::to_string_pretty(value) {
            let _ = fs::write(&path, json);
        }
    }
}

fn save_atomic(path: &std::path::Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temp_path = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let result = (|| {
        fs::write(&temp_path, content).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        if path.exists() {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temp_path, path).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

fn songs_equivalent(left: &SongInfo, right: &SongInfo) -> bool {
    if left.source == right.source && left.id == right.id {
        return true;
    }
    let left_name = normalize_text(&left.name);
    let right_name = normalize_text(&right.name);
    let left_singer = normalize_singer(&left.singer);
    let right_singer = normalize_singer(&right.singer);
    !left_name.is_empty()
        && left_name == right_name
        && !left_singer.is_empty()
        && left_singer == right_singer
}

fn normalize_text(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn normalize_singer(value: &str) -> String {
    let mut singers = value
        .split(['、', ',', '&', '/', ';', '，'])
        .map(normalize_text)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    singers.sort();
    singers.dedup();
    singers.join("|")
}

fn unix_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn validate_custom_playlist_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("歌单名称不能为空".to_string());
    }
    if name.chars().count() > 80 {
        return Err("歌单名称不能超过 80 个字符".to_string());
    }
    Ok(name.to_string())
}

fn ensure_unique_custom_playlist_name(
    playlists: &[CustomPlaylist],
    name: &str,
    except_id: Option<&str>,
) -> Result<(), String> {
    if playlists.iter().any(|playlist| {
        Some(playlist.id.as_str()) != except_id && playlist.name.eq_ignore_ascii_case(name)
    }) {
        return Err("已经存在同名自定义歌单".to_string());
    }
    Ok(())
}

pub(crate) fn same_song_identity(left: &SongInfo, right: &SongInfo) -> bool {
    if left.source == right.source && !left.id.is_empty() && left.id == right.id {
        return true;
    }
    left.source == lx_core::model::source::SourceId::Local
        && right.source == lx_core::model::source::SourceId::Local
        && left.file_path.is_some()
        && left.file_path == right.file_path
}

pub(crate) fn local_song_matches_path(song: &SongInfo, path: &Path) -> bool {
    song.source == lx_core::model::source::SourceId::Local
        && (song.file_path.as_deref() == Some(path) || song.id == path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::{
        CustomPlaylist, PlaybackSession, SavedPlayerState, Storage, same_song_identity,
        songs_equivalent,
    };
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::SourceId;
    use std::sync::RwLock;
    use std::time::Duration;

    fn song(id: &str, source: SourceId, name: &str, singer: &str) -> SongInfo {
        SongInfo::new(id.to_string(), source, name.to_string(), singer.to_string())
    }

    #[test]
    fn same_song_from_another_source_matches_favorite() {
        let favorite = song("1", SourceId::Kw, "晴天 (Live)", "周杰伦、五月天");
        let toggled = song("2", SourceId::Kg, "晴天 Live", "五月天 & 周杰伦");

        assert!(songs_equivalent(&favorite, &toggled));
    }

    #[test]
    fn different_singers_do_not_match() {
        let left = song("1", SourceId::Kw, "后来", "刘若英");
        let right = song("2", SourceId::Wy, "后来", "其他歌手");

        assert!(!songs_equivalent(&left, &right));
    }

    #[test]
    fn empty_metadata_does_not_match_across_sources() {
        let left = song("1", SourceId::Kw, "", "");
        let right = song("2", SourceId::Wy, "", "");

        assert!(!songs_equivalent(&left, &right));
    }

    #[test]
    fn missing_singers_do_not_match_across_sources() {
        let left = song("1", SourceId::Kw, "纯音乐", "");
        let right = song("2", SourceId::Wy, "纯音乐", "");

        assert!(!songs_equivalent(&left, &right));
    }

    #[test]
    fn history_limit_delete_and_clear_are_persisted_in_memory() {
        let data_dir = std::env::temp_dir().join(format!(
            "voicefox-history-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let storage = Storage {
            data_dir: data_dir.clone(),
            favorites: RwLock::new(Vec::new()),
            favorite_playlists: RwLock::new(Vec::new()),
            custom_playlists: RwLock::new(Vec::new()),
            history: RwLock::new(Vec::new()),
        };
        let first = song("1", SourceId::Kw, "One", "Artist");
        let second = song("2", SourceId::Kg, "Two", "Artist");
        let third = song("3", SourceId::Wy, "Three", "Artist");

        storage.add_history(&first, 2);
        storage.add_history(&second, 2);
        storage.add_history(&third, 2);
        assert_eq!(
            storage
                .load_history()
                .iter()
                .map(|song| song.id.as_str())
                .collect::<Vec<_>>(),
            vec!["3", "2"]
        );
        assert!(storage.remove_history(&second));
        assert_eq!(storage.load_history().len(), 1);
        assert!(storage.clear_history());
        assert!(storage.load_history().is_empty());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn playback_session_round_trips_and_clamps_the_index() {
        let data_dir = std::env::temp_dir().join(format!(
            "voicefox-playback-state-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let storage = Storage {
            data_dir: data_dir.clone(),
            favorites: RwLock::new(Vec::new()),
            favorite_playlists: RwLock::new(Vec::new()),
            custom_playlists: RwLock::new(Vec::new()),
            history: RwLock::new(Vec::new()),
        };
        let session = PlaybackSession {
            playlist: vec![song("1", SourceId::Bili, "第一 P", "UP主")],
            current_index: 8,
            position: Duration::from_secs(37),
            state: SavedPlayerState::Paused,
        };

        storage.save_playback_session(&session).unwrap();
        let restored = storage.load_playback_session().unwrap();

        assert_eq!(restored.current_index, 0);
        assert_eq!(restored.position, Duration::from_secs(37));
        assert_eq!(restored.state, SavedPlayerState::Paused);
        storage.clear_playback_session().unwrap();
        assert!(storage.load_playback_session().is_none());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn custom_playlists_store_network_and_local_songs_and_support_editing() {
        let data_dir = std::env::temp_dir().join(format!(
            "voicefox-custom-playlist-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let storage = Storage {
            data_dir: data_dir.clone(),
            favorites: RwLock::new(Vec::new()),
            favorite_playlists: RwLock::new(Vec::new()),
            custom_playlists: RwLock::new(Vec::new()),
            history: RwLock::new(Vec::new()),
        };
        assert!(storage.create_custom_playlist("   ").is_err());
        let playlist = storage.create_custom_playlist("通勤").unwrap();
        assert!(storage.create_custom_playlist("通勤").is_err());
        let network = song("network", SourceId::Wy, "Network", "Artist");
        let mut local = song("local", SourceId::Local, "Local", "Artist");
        local.file_path = Some(data_dir.join("local.flac"));

        assert!(
            storage
                .add_song_to_custom_playlist(&playlist.id, &network)
                .unwrap()
        );
        assert!(
            storage
                .add_song_to_custom_playlist(&playlist.id, &local)
                .unwrap()
        );
        assert!(
            !storage
                .add_song_to_custom_playlist(&playlist.id, &local)
                .unwrap()
        );
        storage
            .rename_custom_playlist(&playlist.id, "夜间通勤")
            .unwrap();

        let stored = storage.custom_playlist(&playlist.id).unwrap();
        assert_eq!(stored.name, "夜间通勤");
        assert_eq!(stored.songs.len(), 2);
        assert_eq!(stored.songs[1].file_path, local.file_path);
        let summaries = storage.custom_playlist_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].song_count, 2);
        let persisted: Vec<CustomPlaylist> =
            Storage::load_file(&data_dir.join("custom_playlists.json"));
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].name, "夜间通勤");
        assert_eq!(persisted[0].songs.len(), 2);
        assert!(
            storage
                .remove_song_from_custom_playlist(&playlist.id, &network)
                .unwrap()
        );
        assert_eq!(
            storage.custom_playlist(&playlist.id).unwrap().songs.len(),
            1
        );
        assert!(storage.delete_custom_playlist(&playlist.id).unwrap());
        assert!(storage.custom_playlist_summaries().is_empty());

        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn empty_song_ids_do_not_collapse_unrelated_tracks() {
        let first = song("", SourceId::Kw, "First", "Artist");
        let second = song("", SourceId::Kw, "Second", "Artist");

        assert!(!same_song_identity(&first, &second));
    }

    #[test]
    fn deleting_a_local_file_cleans_all_custom_playlists_and_persists() {
        let data_dir = std::env::temp_dir().join(format!(
            "voicefox-custom-local-delete-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let storage = Storage {
            data_dir: data_dir.clone(),
            favorites: RwLock::new(Vec::new()),
            favorite_playlists: RwLock::new(Vec::new()),
            custom_playlists: RwLock::new(Vec::new()),
            history: RwLock::new(Vec::new()),
        };
        let first_playlist = storage.create_custom_playlist("通勤").unwrap();
        let second_playlist = storage.create_custom_playlist("夜晚").unwrap();
        let path = data_dir.join("local.flac");
        let mut local = song(&path.to_string_lossy(), SourceId::Local, "Local", "Artist");
        local.file_path = Some(path.clone());
        let legacy_local = song(&path.to_string_lossy(), SourceId::Local, "Legacy", "Artist");
        storage
            .add_song_to_custom_playlist(&first_playlist.id, &local)
            .unwrap();
        storage
            .add_song_to_custom_playlist(&second_playlist.id, &legacy_local)
            .unwrap();

        assert_eq!(
            storage
                .remove_local_path_from_custom_playlists(&path)
                .unwrap(),
            2
        );
        assert!(
            storage
                .custom_playlist(&first_playlist.id)
                .unwrap()
                .songs
                .is_empty()
        );
        assert!(
            storage
                .custom_playlist(&second_playlist.id)
                .unwrap()
                .songs
                .is_empty()
        );
        let persisted: Vec<CustomPlaylist> =
            Storage::load_file(&data_dir.join("custom_playlists.json"));
        assert!(persisted.iter().all(|playlist| playlist.songs.is_empty()));

        let _ = std::fs::remove_dir_all(data_dir);
    }
}
