//! 网易云音乐 (wy) 音源
//!
//! API 协议参考: lx-music src/renderer/utils/musicSdk/wy/

mod crypto;
pub mod leaderboard;
pub mod login;
pub mod lyric;
pub mod parse;
pub mod playlist;
pub mod search;
pub mod session;
pub mod url;

use serde_json::Value;

use async_trait::async_trait;

use lx_core::model::leaderboard::LeaderboardInfo;
use lx_core::model::lyric::LyricData;
use lx_core::model::playlist::Playlist;
use lx_core::model::song::SongInfo;
use lx_core::model::source::{Quality, SourceId};
use lx_core::traits::source::{
    FetchError, MusicSource, SearchError, SearchResult, SongUrl, SourceCapabilities,
};

use crate::http;
use crate::http::SendWithRetry;

/// wy 模块统一 UA / Referer。
///
/// 登录、eapi、带 cookie 的公开 GET 曾各用一套指纹（桌面客户端 / 浏览器 /
/// 全局默认 `voicefox/0.1`），带着同一份 `MUSIC_U` 换指纹请求对风控不友好，
/// 所以整个模块统一成桌面客户端 UA。
pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Safari/537.36 Chrome/91.0.4472.164 NeteaseMusicDesktop/3.0.18.203152";
pub(crate) const REFERER: &str = "https://music.163.com/";
/// 账号信息接口：登录验证、取 uid / 昵称共用这一个端点。
pub(crate) const ACCOUNT_API: &str = "https://music.163.com/api/nuser/account/get";

pub struct WySource;

impl WySource {
    pub fn new() -> Self {
        Self
    }
}

/// 给请求带上登录 cookie；未登录时原样返回。
///
/// 网易云的 VIP / 无损地址依赖登录态，其它接口带上 cookie 也无副作用。
pub(crate) fn with_cookie(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    match session::cookie_header() {
        Some(cookie) => request.header("Cookie", cookie),
        None => request,
    }
}

/// 接口返回「需要登录」的业务码（weapi 301 / eapi 512）。
fn response_requires_login(json: &Value) -> bool {
    matches!(json["code"].as_i64(), Some(301) | Some(512))
}

/// 记录「会话已失效」；返回给用户的提示按登录与否区分措辞。
fn login_required_error() -> FetchError {
    session::mark_login_expired();
    if session::is_logged_in() {
        FetchError::Other("网易云登录已失效，请重新扫码登录".to_string())
    } else {
        FetchError::Other("请先在设置页登录网易云".to_string())
    }
}

/// eapi 加密 POST 的公共实现：`path` 参与 eapi 签名，`endpoint` 是实际请求地址。
///
/// 只负责传输与解析；业务码（`code`）由调用方按接口语义判定。
pub(crate) async fn eapi_post(
    endpoint: &str,
    path: &str,
    data: &Value,
) -> Result<Value, FetchError> {
    let encrypted = crypto::eapi(path, data);
    let response = with_cookie(http::client().post(endpoint))
        .header("User-Agent", USER_AGENT)
        .header("origin", "https://music.163.com")
        .header("Referer", REFERER)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("params={encrypted}"))
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(FetchError::Network(format!("HTTP {}", response.status())));
    }
    let text = response
        .text()
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    serde_json::from_str(&text).map_err(|error| FetchError::Parse(error.to_string()))
}

/// 网易云频控返回的业务码。形状是 HTTP 200 + `{"code":405,"message":"操作频繁，请稍候再试"}`，
/// 因此 `send_with_retry` 这类只看传输层错误的重试完全覆盖不到它。
const THROTTLE_CODE: i64 = 405;
/// 频控退避重试次数与间隔（毫秒）。
const THROTTLE_RETRIES: usize = 4;
const THROTTLE_BACKOFF_MS: [u64; THROTTLE_RETRIES] = [600, 1_200, 2_400, 4_000];

/// 带频控退避的网易云公开 GET。
///
/// 只处理「HTTP 200 但 `code` 不是 200」这一层：`code=405` 按固定阶梯退避重试，
/// 「需要登录」记录会话失效并返回可操作的提示，其它业务码立刻返回可读错误。
/// 热门歌单、榜单、链接直解等所有公开 GET 都应走这里，避免各写一份行为分叉。
pub(crate) async fn get_json(url: &str, what: &str) -> Result<Value, FetchError> {
    let mut throttle_attempt = 0usize;
    loop {
        let json: Value = with_cookie(http::client().get(url))
            .header("User-Agent", USER_AGENT)
            .header("Referer", REFERER)
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
        let code = json["code"].as_i64();
        if code == Some(200) {
            return Ok(json);
        }
        if response_requires_login(&json) {
            return Err(login_required_error());
        }
        if code == Some(THROTTLE_CODE) && throttle_attempt < THROTTLE_RETRIES {
            let backoff = THROTTLE_BACKOFF_MS[throttle_attempt];
            throttle_attempt += 1;
            tracing::warn!("网易云频控（{what}），{backoff}ms 后重试");
            tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            continue;
        }
        let message = json["message"].as_str().unwrap_or_default().trim();
        return Err(FetchError::Other(format!(
            "{what}失败: code={} {message}",
            code.map(|code| code.to_string())
                .unwrap_or_else(|| "未知".into())
        )));
    }
}

/// `FetchError` → `SearchError` 的通用映射，供搜索类接口复用 `get_json` 等实现。
pub(crate) fn search_error_from_fetch(error: FetchError) -> SearchError {
    match error {
        FetchError::Network(message) => SearchError::Network(message),
        FetchError::Parse(message) => SearchError::Parse(message),
        other => SearchError::Api(other.to_string()),
    }
}

impl Default for WySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicSource for WySource {
    fn id(&self) -> SourceId {
        SourceId::Wy
    }

    fn name(&self) -> &str {
        "网易云音乐"
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            playlists: true,
            playlist_search: true,
            playlist_categories: true,
            album: true,
            artist: true,
            leaderboard: true,
            link_parse: true,
            login: true,
            qr_login: true,
            user_playlists: true,
            ..Default::default()
        }
    }

    async fn search(
        &self,
        keyword: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, SearchError> {
        search::search(keyword, page, limit).await
    }

    async fn get_song_url(&self, song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError> {
        url::get_song_url(song, quality).await
    }

    async fn get_lyric(&self, song: &SongInfo) -> Result<LyricData, FetchError> {
        lyric::get_lyric(song).await
    }

    async fn get_cover_url(&self, song: &SongInfo) -> Result<String, FetchError> {
        Ok(song.cover_url.clone().unwrap_or_default())
    }

    fn supported_qualities(&self) -> Vec<Quality> {
        vec![
            Quality::Low128,
            Quality::High320,
            Quality::Flac,
            Quality::Flac24,
        ]
    }

    async fn get_playlist_categories(
        &self,
    ) -> Result<Vec<lx_core::model::playlist::PlaylistCategory>, FetchError> {
        playlist::get_categories().await
    }

    // `tag_id` 在这里是分类名（如「华语」「摇滚」），空值表示全部。
    async fn get_playlists(&self, tag_id: &str, page: u32) -> Result<Vec<Playlist>, FetchError> {
        playlist::get_list(tag_id, page).await
    }

    async fn parse_link(
        &self,
        link: &str,
    ) -> Result<lx_core::traits::source::ParsedLink, FetchError> {
        parse::parse(link).await
    }

    async fn get_user_playlists(&self, page: u32, limit: u32) -> Result<Vec<Playlist>, FetchError> {
        playlist::get_user_playlists(page, limit).await
    }

    async fn create_qr_login(&self) -> Result<lx_core::model::login::QrLoginSession, FetchError> {
        login::create().await
    }

    async fn check_qr_login(
        &self,
        key: &str,
    ) -> Result<lx_core::model::login::QrLoginResult, FetchError> {
        login::check(key).await
    }

    fn logout(&self) -> Result<(), FetchError> {
        session::logout().map_err(FetchError::Other)
    }

    fn is_logged_in(&self) -> bool {
        session::is_logged_in()
    }

    async fn search_playlists(
        &self,
        keyword: &str,
        page: u32,
    ) -> Result<Vec<Playlist>, SearchError> {
        playlist::search_list(keyword, page).await
    }

    async fn get_playlist_detail(&self, id: &str, page: u32) -> Result<Vec<SongInfo>, FetchError> {
        playlist::get_detail(id, page).await
    }

    async fn get_leaderboard_boards(&self) -> Result<Vec<LeaderboardInfo>, SearchError> {
        leaderboard::get_boards().await
    }

    async fn get_leaderboard(
        &self,
        id: &str,
        page: u32,
        limit: u32,
    ) -> Result<SearchResult, SearchError> {
        leaderboard::get_list(id, page, limit).await
    }
}
