use std::collections::HashSet;
use std::time::Duration;

use lx_core::model::playlist::Playlist;
use lx_core::model::playlist::PlaylistCategory;
use lx_core::model::song::SongInfo;
use lx_core::model::source::SourceId;
use lx_core::traits::source::{FetchError, SearchError};
use serde_json::Value;

use crate::http;
use crate::http::SendWithRetry;

/// 网易云「我喜欢的音乐」在歌单列表里的 `specialType`。
///
/// 用这个字段识别红心歌单而不是靠名称：名称用户可以随时改（改完名字就再也
/// 匹配不上），而 `specialType` 是服务端行为，改名也不变。
const SPECIAL_TYPE_FAVORITES: &str = "5";

/// 判断歌单是否为「我喜欢的音乐」。
///
/// 优先看 `specialType`；没有该字段的响应（热门歌单、歌单搜索）退回名称匹配。
pub fn is_favorites(playlist: &Playlist) -> bool {
    match playlist.extra.get("specialType") {
        Some(special) => special == SPECIAL_TYPE_FAVORITES,
        None => playlist.name.ends_with("喜欢的音乐"),
    }
}

/// 网易云频控返回的业务码。形状是 HTTP 200 + `{"code":405,"message":"操作频繁，请稍候再试"}`，
/// 因此 `send_with_retry` 这类只看传输层错误的重试完全覆盖不到它。
const THROTTLE_CODE: i64 = 405;
/// 频控退避重试次数与间隔（毫秒）。
const THROTTLE_RETRIES: usize = 4;
const THROTTLE_BACKOFF_MS: [u64; THROTTLE_RETRIES] = [600, 1_200, 2_400, 4_000];

/// 带频控退避的网易云 GET。
///
/// 只处理「HTTP 200 但 `code` 不是 200」这一层：`code=405` 按固定阶梯退避重试，
/// 其它业务码立刻返回可读错误，避免像以前那样把 `code=405` 的空响应当成
/// 「这个歌单是空的」静默吞掉。
async fn get_json(url: &str, what: &str) -> Result<Value, FetchError> {
    let mut throttle_attempt = 0usize;
    loop {
        let json: Value = super::with_cookie(http::client().get(url))
            .header("Referer", "https://music.163.com/")
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
        if code == Some(THROTTLE_CODE) && throttle_attempt < THROTTLE_RETRIES {
            let backoff = THROTTLE_BACKOFF_MS[throttle_attempt];
            throttle_attempt += 1;
            tracing::warn!("网易云频控（{what}），{backoff}ms 后重试");
            tokio::time::sleep(Duration::from_millis(backoff)).await;
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

/// 热门歌单列表；`category` 为空表示「全部」。
pub async fn get_list(category: &str, page: u32) -> Result<Vec<Playlist>, FetchError> {
    let category = if category.trim().is_empty() {
        "全部"
    } else {
        category.trim()
    };
    let offset = 30 * page.saturating_sub(1);
    let url = format!(
        "https://music.163.com/api/playlist/list?cat={}&order=hot&limit=30&offset={offset}",
        urlencoding::encode(category)
    );
    let json: Value = super::with_cookie(http::client().get(url))
        .header("Referer", "https://music.163.com/")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?
        .json()
        .await
        .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64() != Some(200) {
        return Err(FetchError::Other("网易云热门歌单请求失败".to_string()));
    }
    let items = json["playlists"]
        .as_array()
        .ok_or_else(|| FetchError::Parse("网易云热门歌单列表为空".to_string()))?;
    Ok(items.iter().filter_map(parse_playlist).collect())
}

/// 歌单分类目录：`/api/playlist/catalogue` 返回分类分组与子分类。
///
/// 与热门歌单、歌单搜索一样走公开接口，和网易云音源现有的取数方式保持一致；
/// 接口异常时返回空列表，界面上表现为「没有分类可选」，不影响其它功能。
pub async fn get_categories() -> Result<Vec<PlaylistCategory>, FetchError> {
    let json: Value =
        super::with_cookie(http::client().get("https://music.163.com/api/playlist/catalogue"))
            .header("Referer", "https://music.163.com/")
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64() != Some(200) {
        return Err(FetchError::Other("网易云歌单分类请求失败".to_string()));
    }

    let groups = json["categories"].as_object().cloned().unwrap_or_default();
    let mut categories = Vec::new();
    categories.push(PlaylistCategory {
        id: "全部".to_string(),
        name: "全部".to_string(),
        source: SourceId::Wy,
        group: Some("全部".to_string()),
        count: json["all"]["resourceCount"].as_u64().unwrap_or_default() as u32,
        hot: json["all"]["hot"].as_bool().unwrap_or(true),
        extra: Default::default(),
    });
    for item in json["sub"].as_array().into_iter().flatten() {
        let name = item["name"].as_str().unwrap_or_default().trim().to_string();
        if name.is_empty() {
            continue;
        }
        let group = item["category"]
            .as_i64()
            .map(|category| category.to_string())
            .and_then(|key| groups.get(&key).and_then(Value::as_str).map(str::to_string));
        categories.push(PlaylistCategory {
            id: name.clone(),
            name,
            source: SourceId::Wy,
            group,
            count: item["resourceCount"].as_u64().unwrap_or_default() as u32,
            hot: item["hot"].as_bool().unwrap_or(false),
            extra: Default::default(),
        });
    }
    Ok(categories)
}

/// 单次批量取歌曲详情的上限。网易云对 `song/detail` 的 `c` 参数长度有限制，
/// 100 首一批实测稳定。
const SONG_DETAIL_BATCH: usize = 100;

/// 歌单歌曲（全量）。
///
/// 这里不能再用 `playlist.tracks`：网易云对**收藏来的**（非本人创建的）歌单只
/// 返回前 20 首，完整列表在 `playlist.trackIds`，而 `n` 参数已被服务端忽略
/// （实测 `n=1000` 与 `n=100000` 都只给 20 首）。所以改为以 `trackIds` 为准，
/// 再分批用 `api/v3/song/detail` 补全歌曲详情。
///
/// `page` 保留只是为了兼容既有调用方：v3 详情接口没有 offset 参数，一次取全量。
pub async fn get_detail(id: &str, _page: u32) -> Result<Vec<SongInfo>, FetchError> {
    let json = get_json(
        &format!("https://music.163.com/api/v3/playlist/detail?id={id}&n=0&s=0"),
        "网易云歌单详情请求",
    )
    .await?;
    songs_from_detail(&json["playlist"]).await
}

/// 从 v3 歌单详情响应的 `playlist` 节点里取出**完整**曲目 id 列表。
///
/// `trackIds` 的元素是 `{"id":..}` 对象，但旧响应里也可能是裸数字，两种都兼容。
pub(crate) fn track_ids_from_detail(playlist: &Value) -> Vec<i64> {
    playlist["trackIds"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item["id"]
                        .as_i64()
                        .or_else(|| item.as_i64())
                        .filter(|id| *id > 0)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 用已经取到的歌单详情响应补全**全部**歌曲。
///
/// 抽出来是为了让链接直解（`parse::fetch_playlist`）复用同一条路径：那里原先
/// 直接读 `playlist.tracks`，同样只能拿到前 20 首。
pub(crate) async fn songs_from_detail(playlist: &Value) -> Result<Vec<SongInfo>, FetchError> {
    let ids = track_ids_from_detail(playlist);

    // 没有 trackIds 的响应（部分特殊歌单）退回 tracks，至少不丢已有数据。
    if ids.is_empty() {
        return playlist["tracks"]
            .as_array()
            .map(|items| items.iter().filter_map(super::search::parse_song).collect())
            .ok_or_else(|| FetchError::Parse("网易云歌单歌曲列表为空".to_string()));
    }

    let mut songs = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(SONG_DETAIL_BATCH) {
        songs.extend(get_song_details(chunk).await?);
    }
    Ok(songs)
}

/// 批量补全歌曲详情：`api/v3/song/detail?c=[{"id":..},..]`。
async fn get_song_details(ids: &[i64]) -> Result<Vec<SongInfo>, FetchError> {
    let payload = ids
        .iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect::<Vec<_>>();
    let body = serde_json::to_string(&payload).unwrap_or_default();
    let encoded = urlencoding::encode(&body);
    let json = get_json(
        &format!("https://music.163.com/api/v3/song/detail?c={encoded}"),
        "网易云歌曲详情请求",
    )
    .await?;
    Ok(json["songs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(super::search::parse_song)
        .collect())
}

/// 我的歌单：需要登录 cookie。
///
/// 用公开的 `api/user/playlist` 而不是 weapi 版本：voicefox 的网易云实现
/// 一直走公开接口（热门歌单、歌单搜索同理），少一套加密实现也少一处失效点。
pub async fn get_user_playlists(page: u32, limit: u32) -> Result<Vec<Playlist>, FetchError> {
    // 未登录时先给明确提示，不必等接口返回。
    if super::session::cookie_header().is_none() {
        return Err(FetchError::Other("请先在设置页登录网易云".to_string()));
    }
    let uid = user_id().await?;
    let limit = limit.max(1);
    let offset = limit.saturating_mul(page.saturating_sub(1));
    let json = get_json(
        &format!(
            "https://music.163.com/api/user/playlist?uid={uid}&limit={limit}&offset={offset}&includeVideo=true"
        ),
        "获取网易云个人歌单",
    )
    .await?;
    Ok(json["playlist"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_playlist)
        .collect())
}

/// 取全部「我的歌单」，自动翻页。
///
/// `api/user/playlist` 单页最多 100 个，账号歌单超过 100 个时旧实现（固定
/// `page=1, limit=100`）会静默截断掉后面的歌单。
pub async fn get_all_user_playlists() -> Result<Vec<Playlist>, FetchError> {
    const PAGE_SIZE: u32 = 100;
    const MAX_PAGES: u32 = 20;
    let mut all: Vec<Playlist> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for page in 1..=MAX_PAGES {
        let items = get_user_playlists(page, PAGE_SIZE).await?;
        let received = items.len();
        for item in items {
            if seen.insert(item.id.clone()) {
                all.push(item);
            }
        }
        if received < PAGE_SIZE as usize {
            break;
        }
    }
    Ok(all)
}

/// 取当前登录账号的 uid：`api/nuser/account/get` 同时返回账号与昵称。
async fn user_id() -> Result<String, FetchError> {
    let json = get_json(
        "https://music.163.com/api/nuser/account/get",
        "网易云账号信息请求",
    )
    .await?;
    let uid = json["account"]["id"]
        .as_i64()
        .or_else(|| json["profile"]["userId"].as_i64())
        .filter(|uid| *uid > 0)
        .map(|uid| uid.to_string());
    uid.ok_or_else(|| FetchError::Other("网易云登录已失效，请重新扫码".to_string()))
}

pub async fn search_list(keyword: &str, page: u32) -> Result<Vec<Playlist>, SearchError> {
    let offset = 30 * page.saturating_sub(1);
    let url = format!(
        "https://music.163.com/api/search/get/web?csrf_token=&s={}&type=1000&limit=30&offset={offset}",
        urlencoding::encode(keyword)
    );
    let json: Value = super::with_cookie(http::client().get(url))
        .header("Referer", "https://music.163.com/")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|e| SearchError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| SearchError::Parse(e.to_string()))?;
    if json["code"].as_i64() != Some(200) {
        return Err(SearchError::Api("网易云歌单搜索失败".to_string()));
    }
    Ok(json["result"]["playlists"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_playlist)
        .collect())
}

fn parse_playlist(item: &Value) -> Option<Playlist> {
    let id = value_string(&item["id"]);
    let name = item["name"].as_str()?.trim().to_string();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    // 记下 specialType：红心歌单（5）靠它识别，用户改名也不会认错。
    let mut extra = std::collections::HashMap::new();
    if let Some(special) = item["specialType"].as_i64() {
        extra.insert("specialType".to_string(), special.to_string());
    }
    Some(Playlist {
        id,
        name,
        source: SourceId::Wy,
        cover_url: non_empty_string(&item["coverImgUrl"]),
        song_count: value_u64(&item["trackCount"]).unwrap_or_default() as u32,
        description: non_empty_string(&item["description"]),
        play_count: value_u64(&item["playCount"]),
        creator: None,
        link: None,
        extra,
    })
}

fn non_empty_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn value_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_u64().map(|value| value.to_string()))
        .unwrap_or_default()
}

fn value_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn playlist_with(name: &str, special_type: Option<i64>) -> Playlist {
        let mut item = json!({ "id": 1, "name": name });
        if let Some(special) = special_type {
            item["specialType"] = json!(special);
        }
        parse_playlist(&item).expect("playlist")
    }

    #[test]
    fn favorites_is_detected_by_special_type_even_after_rename() {
        // 用户把红心歌单改名后，名称匹配就失效了，必须靠 specialType。
        assert!(is_favorites(&playlist_with("随便改的名字", Some(5))));
        assert!(is_favorites(&playlist_with("我喜欢的音乐", Some(5))));
    }

    #[test]
    fn favorites_falls_back_to_name_when_special_type_is_absent() {
        // 热门歌单/歌单搜索的响应没有 specialType。
        assert!(is_favorites(&playlist_with("我喜欢的音乐", None)));
        assert!(!is_favorites(&playlist_with("别人的喜欢的音乐合集", None)));
        assert!(!is_favorites(&playlist_with("我的歌单", None)));
    }

    #[test]
    fn a_normal_playlist_is_never_treated_as_favorites() {
        assert!(!is_favorites(&playlist_with("我喜欢的音乐", Some(0))));
    }

    #[test]
    fn track_ids_reads_objects_and_bare_numbers() {
        let playlist = json!({
            "trackIds": [{ "id": 11 }, 22, { "id": 33 }],
        });
        assert_eq!(track_ids_from_detail(&playlist), vec![11, 22, 33]);
    }

    #[test]
    fn track_ids_skips_invalid_entries() {
        let playlist = json!({
            "trackIds": [{ "id": 0 }, { "id": -5 }, { "id": "x" }, {}, { "id": 7 }],
        });
        assert_eq!(track_ids_from_detail(&playlist), vec![7]);
    }

    #[test]
    fn track_ids_is_empty_when_the_response_has_none() {
        assert!(track_ids_from_detail(&json!({ "tracks": [] })).is_empty());
        assert!(track_ids_from_detail(&json!({})).is_empty());
    }
}
