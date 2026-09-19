use std::sync::{Arc, RwLock};

use lx_core::model::playlist::{Playlist, PlaylistCategory};
use lx_core::model::song::SongInfo;
use lx_core::model::source::SourceId;
use lx_source::manager::SourceManager;

#[derive(Clone, Default)]
pub struct PlaylistService {
    playlists: Arc<RwLock<Vec<Playlist>>>,
    sources: Option<Arc<SourceManager>>,
}

impl PlaylistService {
    pub fn new() -> Self {
        Self {
            playlists: Arc::new(RwLock::new(Vec::new())),
            sources: None,
        }
    }

    pub fn with_sources(sources: Arc<SourceManager>) -> Self {
        Self {
            playlists: Arc::new(RwLock::new(Vec::new())),
            sources: Some(sources),
        }
    }

    pub async fn list(
        &self,
        source: SourceId,
        category: &str,
        page: u32,
    ) -> Result<Vec<Playlist>, String> {
        self.sources
            .as_ref()
            .ok_or_else(|| "playlist source manager unavailable".to_string())?
            .playlists(source, category, page)
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn search(
        &self,
        source: SourceId,
        keyword: &str,
        page: u32,
    ) -> Result<Vec<Playlist>, String> {
        self.sources
            .as_ref()
            .ok_or_else(|| "playlist source manager unavailable".to_string())?
            .search_playlists(source, keyword, page)
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn songs(
        &self,
        source: SourceId,
        playlist_id: &str,
    ) -> Result<Vec<SongInfo>, String> {
        self.sources
            .as_ref()
            .ok_or_else(|| "playlist source manager unavailable".to_string())?
            .playlist_detail(source, playlist_id, 1)
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn categories(&self, source: SourceId) -> Result<Vec<PlaylistCategory>, String> {
        self.sources
            .as_ref()
            .ok_or_else(|| "playlist source manager unavailable".to_string())?
            .playlist_categories(source)
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn user(&self, source: SourceId, page: u32) -> Result<Vec<Playlist>, String> {
        self.sources
            .as_ref()
            .ok_or_else(|| "playlist source manager unavailable".to_string())?
            .user_playlists(source, page, 30)
            .await
            .map_err(|e| e.to_string())
    }
    pub fn snapshot(&self) -> Vec<Playlist> {
        self.playlists
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn replace(&self, playlists: Vec<Playlist>) {
        *self.playlists.write().unwrap_or_else(|e| e.into_inner()) = playlists;
    }

    pub fn open(&self, playlist: Playlist) -> Playlist {
        playlist
    }

    pub fn upsert(&self, playlist: Playlist) {
        let mut playlists = self.playlists.write().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = playlists
            .iter_mut()
            .find(|item| item.id == playlist.id && item.source == playlist.source)
        {
            *existing = playlist;
        } else {
            playlists.push(playlist);
        }
    }

    pub fn remove(&self, source: lx_core::model::source::SourceId, id: &str) -> bool {
        let mut playlists = self.playlists.write().unwrap_or_else(|e| e.into_inner());
        let before = playlists.len();
        playlists.retain(|item| !(item.source == source && item.id == id));
        before != playlists.len()
    }
}
