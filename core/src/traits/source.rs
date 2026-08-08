use async_trait::async_trait;
use std::time::Duration;

use crate::model::leaderboard::LeaderboardInfo;
use crate::model::lyric::LyricData;
use crate::model::playlist::{Album, Artist, Playlist, Tag};
use crate::model::song::SongInfo;
use crate::model::source::{Quality, SourceId};

/// 搜索结果
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub items: Vec<SongInfo>,
    pub total: u32,
    pub has_more: bool,
}

/// 播放 URL 结果
#[derive(Debug, Clone)]
pub struct SongUrl {
    pub url: String,
    pub quality: Quality,
    pub duration: Duration,
    pub cover_url: Option<String>,
    pub qualities: Vec<Quality>,
    pub headers: Vec<(String, String)>,
}

/// 音源统一接口
#[async_trait]
pub trait MusicSource: Send + Sync {
    /// 音源唯一标识
    fn id(&self) -> SourceId;
    /// 音源显示名称
    fn name(&self) -> &str;

    /// 搜索歌曲
    async fn search(
        &self,
        keyword: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, SearchError>;
    /// 获取播放 URL
    async fn get_song_url(&self, song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError>;
    /// 获取歌词
    async fn get_lyric(&self, song: &SongInfo) -> Result<LyricData, FetchError>;
    /// 获取封面 URL
    async fn get_cover_url(&self, song: &SongInfo) -> Result<String, FetchError>;

    /// 支持的音质列表
    fn supported_qualities(&self) -> Vec<Quality>;

    // --- 可选实现 ---
    async fn get_playlist_tags(&self) -> Result<Vec<Tag>, FetchError> {
        Ok(vec![])
    }
    async fn get_playlists(&self, _tag_id: &str, _page: u32) -> Result<Vec<Playlist>, FetchError> {
        Ok(vec![])
    }
    async fn get_playlist_detail(
        &self,
        _id: &str,
        _page: u32,
    ) -> Result<Vec<SongInfo>, FetchError> {
        Ok(vec![])
    }
    /// 获取歌手歌曲。未实现专用接口的音源会回退到搜索并按歌手过滤。
    async fn get_artist_songs(
        &self,
        artist: &Artist,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, SearchError> {
        let result = self.search(&artist.name, page, limit).await?;
        let artist_name = normalize_artist_name(&artist.name);
        let items = result
            .items
            .into_iter()
            .filter(|song| {
                normalize_artist_name(&song.singer).contains(&artist_name)
                    || artist_name.contains(&normalize_artist_name(&song.singer))
            })
            .collect::<Vec<_>>();
        Ok(SearchResult {
            total: items.len() as u32,
            has_more: result.has_more,
            items,
        })
    }
    /// 获取歌手专辑。默认从歌手歌曲结果中按专辑去重。
    async fn get_artist_albums(
        &self,
        artist: &Artist,
        page: u32,
        limit: u32,
    ) -> Result<Vec<Album>, SearchError> {
        let result = self.get_artist_songs(artist, page, limit.max(100)).await?;
        let mut albums = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for song in result.items {
            if song.album_name.trim().is_empty() {
                continue;
            }
            let key = if song.album_id.trim().is_empty() {
                format!("name:{}", song.album_name.trim().to_lowercase())
            } else {
                format!("id:{}", song.album_id)
            };
            if !seen.insert(key) {
                continue;
            }
            albums.push(Album {
                id: if song.album_id.trim().is_empty() {
                    song.album_name.clone()
                } else {
                    song.album_id.clone()
                },
                name: song.album_name,
                source: song.source,
                cover_url: song.cover_url,
                artist: artist.name.clone(),
            });
        }
        Ok(albums)
    }
    /// 获取专辑曲目。默认搜索歌手和专辑名，再按专辑标识或名称过滤。
    async fn get_album_songs(
        &self,
        album: &Album,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, SearchError> {
        let keyword = format!("{} {}", album.artist, album.name);
        let result = self.search(&keyword, page, limit).await?;
        let artist_name = normalize_artist_name(&album.artist);
        let album_name = album.name.trim().to_lowercase();
        let items = result
            .items
            .into_iter()
            .filter(|song| {
                let artist_matches = artist_name.is_empty()
                    || normalize_artist_name(&song.singer).contains(&artist_name);
                let album_matches = (!album.id.trim().is_empty()
                    && !song.album_id.trim().is_empty()
                    && song.album_id == album.id)
                    || (!album_name.is_empty()
                        && song.album_name.trim().to_lowercase() == album_name);
                artist_matches && album_matches
            })
            .collect::<Vec<_>>();
        Ok(SearchResult {
            total: items.len() as u32,
            has_more: result.has_more,
            items,
        })
    }
    async fn get_leaderboard_boards(&self) -> Result<Vec<LeaderboardInfo>, SearchError> {
        Err(SearchError::Other("该音源不支持排行榜".to_string()))
    }
    async fn get_leaderboard(
        &self,
        _id: &str,
        _page: u32,
        _limit: u32,
    ) -> Result<SearchResult, SearchError> {
        Err(SearchError::Other("该音源不支持排行榜".to_string()))
    }
}

fn normalize_artist_name(value: &str) -> String {
    value
        .split(['、', ',', '&', '/', '|'])
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 搜索错误
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("api error: {0}")]
    Api(String),
    #[error("{0}")]
    Other(String),
}

/// 获取错误
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("network error: {0}")]
    Network(String),
    #[error("not found")]
    NotFound,
    #[error("too many requests")]
    TooManyRequests,
    #[error("parse error: {0}")]
    Parse(String),
    #[error("{0}")]
    Other(String),
}
