//! 跨音源歌单同步适配层。
//!
//! 同步算法位于 lx-core::sync；本模块只负责把各平台 API 映射到统一接口。
//! 新增平台时只需要实现 SyncProvider，不修改匹配器和执行引擎。

use async_trait::async_trait;
use lx_core::model::song::SongInfo;
use lx_core::model::source::SourceId;
use lx_core::sync::{SyncCollection, SyncCollectionKind, SyncError, SyncProvider};
use lx_core::traits::source::MusicSource;

use crate::http;
use crate::http::SendWithRetry;
use crate::{tx, wy};

pub struct WySyncProvider {
    source: wy::WySource,
}
pub struct TxSyncProvider {
    source: tx::TxSource,
}

impl Default for WySyncProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WySyncProvider {
    pub fn new() -> Self {
        Self {
            source: wy::WySource::new(),
        }
    }
}
impl Default for TxSyncProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TxSyncProvider {
    pub fn new() -> Self {
        Self {
            source: tx::TxSource::new(),
        }
    }
}

fn map_error(error: impl ToString) -> SyncError {
    SyncError::Provider(error.to_string())
}

fn collection_from(
    kind: SyncCollectionKind,
    playlist: lx_core::model::playlist::Playlist,
    songs: Vec<SongInfo>,
) -> SyncCollection {
    SyncCollection {
        kind,
        id: playlist.id,
        name: playlist.name,
        source: playlist.source,
        songs,
    }
}

#[async_trait]
impl SyncProvider for WySyncProvider {
    fn source_id(&self) -> SourceId {
        SourceId::Wy
    }
    fn source_name(&self) -> &str {
        "网易云音乐"
    }

    async fn list_collections(
        &self,
        kind: SyncCollectionKind,
    ) -> Result<Vec<SyncCollection>, SyncError> {
        if !self.source.is_logged_in() {
            return Err(SyncError::NotLoggedIn(self.source_name().into()));
        }
        let playlists = wy::playlist::get_user_playlists(1, 100)
            .await
            .map_err(map_error)?;
        let selected = match kind {
            SyncCollectionKind::Playlist => playlists,
            SyncCollectionKind::Favorites => playlists
                .into_iter()
                .filter(|p| p.name == "我喜欢")
                .collect(),
        };
        let mut result = Vec::with_capacity(selected.len());
        for playlist in selected {
            let songs = wy::playlist::get_detail(&playlist.id, 0)
                .await
                .unwrap_or_default();
            result.push(collection_from(kind, playlist, songs));
        }
        Ok(result)
    }

    async fn get_collection(
        &self,
        kind: SyncCollectionKind,
        id: &str,
    ) -> Result<SyncCollection, SyncError> {
        self.list_collections(kind)
            .await?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| SyncError::CollectionNotFound(id.into()))
    }

    async fn create_collection(
        &self,
        kind: SyncCollectionKind,
        name: &str,
    ) -> Result<SyncCollection, SyncError> {
        if kind == SyncCollectionKind::Favorites {
            return Err(SyncError::WriteUnsupported("网易云“我喜欢”不能创建".into()));
        }
        let url = format!(
            "https://music.163.com/api/playlist/create?name={}",
            urlencoding::encode(name)
        );
        let json: serde_json::Value = wy::with_cookie(http::client().get(url))
            .header("Referer", "https://music.163.com/")
            .send_with_retry(http::RETRY_ATTEMPTS)
            .await
            .map_err(map_error)?
            .json()
            .await
            .map_err(map_error)?;
        let id = json["id"]
            .as_u64()
            .map(|v| v.to_string())
            .or_else(|| json["playlist"]["id"].as_u64().map(|v| v.to_string()))
            .ok_or_else(|| SyncError::Provider("网易云创建歌单失败".into()))?;
        Ok(collection_from(
            SyncCollectionKind::Playlist,
            lx_core::model::playlist::Playlist::new(id, name, SourceId::Wy),
            Vec::new(),
        ))
    }

    async fn add_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError> {
        if songs.is_empty() {
            return Ok(0);
        }
        if collection.kind == SyncCollectionKind::Favorites {
            let mut added = 0;
            for song in songs {
                let url = format!(
                    "https://music.163.com/api/song/like?id={}&like=true",
                    song.id
                );
                let json: serde_json::Value = wy::with_cookie(http::client().get(url))
                    .header("Referer", "https://music.163.com/")
                    .send_with_retry(http::RETRY_ATTEMPTS)
                    .await
                    .map_err(map_error)?
                    .json()
                    .await
                    .map_err(map_error)?;
                if json["code"].as_i64() == Some(200) {
                    added += 1;
                }
            }
            return Ok(added);
        }
        let ids = songs
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "https://music.163.com/api/playlist/tracks?op=add&pid={}&tracks={ids}",
            collection.id
        );
        let json: serde_json::Value = wy::with_cookie(http::client().get(url))
            .header("Referer", "https://music.163.com/")
            .send_with_retry(http::RETRY_ATTEMPTS)
            .await
            .map_err(map_error)?
            .json()
            .await
            .map_err(map_error)?;
        match json["code"].as_i64() {
            Some(200) => Ok(songs.len()),
            Some(502) => Ok(0),
            _ => Err(SyncError::Provider(format!(
                "网易云添加歌曲失败: {}",
                json["code"]
            ))),
        }
    }

    async fn remove_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError> {
        if songs.is_empty() {
            return Ok(0);
        }
        if collection.kind == SyncCollectionKind::Favorites {
            let mut removed = 0;
            for song in songs {
                let url = format!(
                    "https://music.163.com/api/song/like?id={}&like=false",
                    song.id
                );
                let json: serde_json::Value = wy::with_cookie(http::client().get(url))
                    .header("Referer", "https://music.163.com/")
                    .send_with_retry(http::RETRY_ATTEMPTS)
                    .await
                    .map_err(map_error)?
                    .json()
                    .await
                    .map_err(map_error)?;
                if json["code"].as_i64() == Some(200) {
                    removed += 1;
                }
            }
            return Ok(removed);
        }
        let ids = songs
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "https://music.163.com/api/playlist/tracks?op=del&pid={}&tracks={ids}",
            collection.id
        );
        let json: serde_json::Value = wy::with_cookie(http::client().get(url))
            .header("Referer", "https://music.163.com/")
            .send_with_retry(http::RETRY_ATTEMPTS)
            .await
            .map_err(map_error)?
            .json()
            .await
            .map_err(map_error)?;
        if json["code"].as_i64() == Some(200) {
            Ok(songs.len())
        } else {
            Err(SyncError::Provider(format!(
                "网易云删除歌曲失败: {}",
                json["code"]
            )))
        }
    }

    async fn search_song(&self, song: &SongInfo) -> Result<Vec<SongInfo>, SyncError> {
        let queries = [
            format!("{} {}", song.name, song.singer),
            format!("{} {}", song.name, song.album_name),
            song.name.clone(),
        ];
        let mut result = Vec::new();
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            let page = self.source.search(&query, 1, 10).await.map_err(map_error)?;
            for candidate in page.items {
                if !result.iter().any(|item: &SongInfo| {
                    item.id == candidate.id && item.source == candidate.source
                }) {
                    result.push(candidate);
                }
            }
            if result.len() >= 30 {
                break;
            }
        }
        Ok(result)
    }
}

#[async_trait]
impl SyncProvider for TxSyncProvider {
    fn source_id(&self) -> SourceId {
        SourceId::Tx
    }
    fn source_name(&self) -> &str {
        "QQ音乐"
    }

    async fn list_collections(
        &self,
        kind: SyncCollectionKind,
    ) -> Result<Vec<SyncCollection>, SyncError> {
        if !self.source.is_logged_in() {
            return Err(SyncError::NotLoggedIn(self.source_name().into()));
        }
        if kind == SyncCollectionKind::Favorites {
            let (playlist, songs) = tx::playlist::get_favorites().await.map_err(map_error)?;
            return Ok(vec![collection_from(kind, playlist, songs)]);
        }
        let playlists = tx::playlist::get_user_playlists(1, 100)
            .await
            .map_err(map_error)?;
        let mut result = Vec::with_capacity(playlists.len());
        for playlist in playlists {
            let songs = tx::playlist::get_detail(&playlist.id)
                .await
                .unwrap_or_default();
            result.push(collection_from(kind, playlist, songs));
        }
        Ok(result)
    }

    async fn get_collection(
        &self,
        kind: SyncCollectionKind,
        id: &str,
    ) -> Result<SyncCollection, SyncError> {
        self.list_collections(kind)
            .await?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| SyncError::CollectionNotFound(id.into()))
    }

    async fn create_collection(
        &self,
        kind: SyncCollectionKind,
        name: &str,
    ) -> Result<SyncCollection, SyncError> {
        if kind == SyncCollectionKind::Favorites {
            return Err(SyncError::WriteUnsupported("QQ“我喜欢”不能创建".into()));
        }
        let playlist = tx::playlist::create_user_playlist(name)
            .await
            .map_err(map_error)?;
        Ok(collection_from(kind, playlist, Vec::new()))
    }
    async fn add_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError> {
        if collection.kind == SyncCollectionKind::Favorites {
            return tx::playlist::add_songs_to_playlist("201", songs)
                .await
                .map_err(map_error);
        }
        tx::playlist::add_songs_to_playlist(&collection.id, songs)
            .await
            .map_err(map_error)
    }
    async fn remove_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError> {
        if collection.kind == SyncCollectionKind::Favorites {
            return tx::playlist::remove_songs_from_playlist("201", songs)
                .await
                .map_err(map_error);
        }
        tx::playlist::remove_songs_from_playlist(&collection.id, songs)
            .await
            .map_err(map_error)
    }
    async fn search_song(&self, song: &SongInfo) -> Result<Vec<SongInfo>, SyncError> {
        let queries = [
            format!("{} {}", song.name, song.singer),
            format!("{} {}", song.name, song.album_name),
            song.name.clone(),
        ];
        let mut result = Vec::new();
        for query in queries {
            if query.trim().is_empty() {
                continue;
            }
            let page = self.source.search(&query, 1, 10).await.map_err(map_error)?;
            for candidate in page.items {
                if !result.iter().any(|item: &SongInfo| {
                    item.id == candidate.id && item.source == candidate.source
                }) {
                    result.push(candidate);
                }
            }
            if result.len() >= 30 {
                break;
            }
        }
        Ok(result)
    }
    async fn supports_write(&self, _kind: SyncCollectionKind) -> bool {
        self.source.is_logged_in()
    }
}

/// 创建指定平台的同步适配器。
///
/// 这个入口是 UI / runtime 唯一需要知道的平台分派点；同步算法不关心具体平台。
pub fn provider(source: SourceId) -> Option<Box<dyn SyncProvider>> {
    match source {
        SourceId::Wy => Some(Box::new(WySyncProvider::new())),
        SourceId::Tx => Some(Box::new(TxSyncProvider::new())),
        _ => None,
    }
}
