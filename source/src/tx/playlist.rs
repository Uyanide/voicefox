use lx_core::model::playlist::{Playlist, PlaylistCategory};
use lx_core::model::song::SongInfo;
use lx_core::model::source::SourceId;
use lx_core::traits::source::{FetchError, SearchError};
use serde_json::Value;

use crate::http;
use crate::http::SendWithRetry;

pub async fn get_list(page: u32) -> Result<Vec<Playlist>, FetchError> {
    let body = serde_json::json!({
        "comm": { "cv": 1602, "ct": 20 },
        "playlist": {
            "method": "get_playlist_by_tag",
            "param": {
                "id": 10000000,
                "sin": 36 * page.saturating_sub(1),
                "size": 36,
                "order": 5,
                "cur_page": page
            },
            "module": "playlist.PlayListPlazaServer"
        }
    });
    let json: Value =
        super::with_cookie(http::client().post("https://u.y.qq.com/cgi-bin/musicu.fcg"))
            .header("Referer", "https://y.qq.com/")
            .json(&body)
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64() != Some(0) || json["playlist"]["code"].as_i64() != Some(0) {
        return Err(FetchError::Other("QQ 热门歌单请求失败".to_string()));
    }
    let items = json["playlist"]["data"]["v_playlist"]
        .as_array()
        .ok_or_else(|| FetchError::Parse("QQ 热门歌单列表为空".to_string()))?;
    Ok(items.iter().filter_map(parse_playlist).collect())
}

pub async fn get_detail(id: &str) -> Result<Vec<SongInfo>, FetchError> {
    get_detail_with_meta(id).await.map(|(_, songs)| songs)
}

/// 歌单详情单页歌曲数（接口单次上限）与翻页上限（10 页 = 1 万首封顶）。
const DETAIL_PAGE_SIZE: u64 = 1000;
const DETAIL_MAX_PAGES: u64 = 10;

/// 歌单详情 + 元数据：链接直解需要歌单名与封面。
///
/// 详情接口单次最多返回 1000 首，超长歌单（实测存在 1981 首）必须按
/// `song_begin` 翻页补全，否则镜像会静默丢歌。
pub async fn get_detail_with_meta(id: &str) -> Result<(Playlist, Vec<SongInfo>), FetchError> {
    let disstid = id.parse::<u64>().unwrap_or_default();
    let mut all_songs: Vec<SongInfo> = Vec::new();
    let mut playlist: Option<Playlist> = None;
    for page in 0..DETAIL_MAX_PAGES {
        let (meta, songs) = fetch_detail_page(disstid, page * DETAIL_PAGE_SIZE).await?;
        let complete = songs.len() < DETAIL_PAGE_SIZE as usize;
        if playlist.is_none() {
            playlist = Some(meta);
        }
        let done = songs.is_empty() || complete;
        all_songs.extend(songs);
        if done {
            break;
        }
    }
    let mut playlist = playlist.ok_or_else(|| FetchError::Parse("QQ 歌单详情为空".to_string()))?;
    playlist.song_count = all_songs.len() as u32;
    Ok((playlist, all_songs))
}

/// 拉取详情的某一页：返回（歌单元数据，本页歌曲）。元数据以首页为准。
async fn fetch_detail_page(
    disstid: u64,
    song_begin: u64,
) -> Result<(Playlist, Vec<SongInfo>), FetchError> {
    let body = serde_json::json!({
        "comm": {
            "ct": 20,
            "cv": 1859,
            "uin": 0,
            "format": "json"
        },
        "req": {
            "module": "music.srfDissInfo.aiDissInfo",
            "method": "uniform_get_Dissinfo",
            "param": {
                "disstid": disstid,
                "enc_host_uin": "",
                "tag": 1,
                "userInfo": 1,
                "song_begin": song_begin,
                "song_num": DETAIL_PAGE_SIZE,
                "onlysonglist": 0
            }
        }
    });
    let json: Value =
        super::with_cookie(http::client().post("https://u.y.qq.com/cgi-bin/musicu.fcg"))
            .header("Referer", "https://y.qq.com/")
            .json(&body)
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64() != Some(0)
        || json["req"]["code"].as_i64() != Some(0)
        || json["req"]["data"]["code"].as_i64() != Some(0)
    {
        return Err(FetchError::Other("QQ 歌单详情请求失败".to_string()));
    }
    let items = json["req"]["data"]["songlist"]
        .as_array()
        .ok_or_else(|| FetchError::Parse("QQ 歌单歌曲列表为空".to_string()))?;
    let songs = items
        .iter()
        .filter_map(super::search::parse_song)
        .collect::<Vec<_>>();
    let detail = &json["req"]["data"];
    let name = detail["dirinfo"]["title"]
        .as_str()
        .or_else(|| detail["dissname"].as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("QQ 歌单 {disstid}"));
    let mut playlist = Playlist::new(disstid.to_string(), name, SourceId::Tx);
    playlist.song_count = songs.len() as u32;
    playlist.cover_url = detail["dirinfo"]["picurl"]
        .as_str()
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    playlist.description = detail["desc"].as_str().map(str::to_string);
    playlist.play_count = value_u64(&detail["visitnum"]);
    playlist.creator = detail["nickname"]
        .as_str()
        .or_else(|| detail["hostname"].as_str())
        .map(str::to_string);
    playlist.link = Some(format!("https://y.qq.com/n/ryqq/playlist/{disstid}"));
    Ok((playlist, songs))
}

/// 按专辑 MID 获取专辑曲目，避免走“歌手+专辑名搜索”兜底。
pub async fn get_album_songs(
    album_mid: &str,
    page: u32,
    limit: u32,
) -> Result<Vec<SongInfo>, FetchError> {
    let limit = limit.clamp(1, 1000);
    let begin = limit * page.saturating_sub(1);
    let body = serde_json::json!({
        "comm": { "ct": 24, "cv": 10000 },
        "albumSonglist": {
            "method": "GetAlbumSongList",
            "param": { "albumMid": album_mid, "albumID": 0, "begin": begin, "num": limit, "order": 2 },
            "module": "music.musichallAlbum.AlbumSongList"
        }
    });
    let json: Value =
        super::with_cookie(http::client().post("https://u.y.qq.com/cgi-bin/musicu.fcg"))
            .header("Referer", "https://y.qq.com/")
            .json(&body)
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
    let req = &json["albumSonglist"];
    if req["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(FetchError::Other("QQ 专辑曲目请求失败".to_string()));
    }
    let songs = req["data"]["songList"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| super::search::parse_song(&item["songInfo"]))
        .collect::<Vec<_>>();
    if songs.is_empty() {
        return Err(FetchError::NotFound);
    }
    Ok(songs)
}

/// 当前登录账号的个人歌单 = 创建的歌单 + 收藏的歌单。
///
/// 旧的 `fcg_user_created_diss.fcg`（带 `.fcg` 后缀）已下线，请求只会得到
/// 404 + 空 body；创建歌单改走无后缀的 `fcg_user_created_diss`，收藏歌单走
/// 网页版个人中心的 `fcg_get_profile_order_asset.fcg`（reqtype=3）。两份列表
/// 任一失败都整体报错：同步层靠「全部成功」才允许整体替换镜像缓存，半份
/// 成功的列表会把缺的歌单当成远端已删除。
pub async fn get_user_playlists(page: u32, limit: u32) -> Result<Vec<Playlist>, FetchError> {
    if !super::session::is_logged_in() {
        return Err(FetchError::Other("请先在设置页登录 QQ 音乐".to_string()));
    }
    let uin = super::session::snapshot()
        .user_id
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FetchError::Other("QQ 登录缺少账号 ID，请重新扫码".to_string()))?;
    let begin = limit.max(1) * page.saturating_sub(1);
    let size = limit.max(1);

    let created_url = format!(
        "https://c.y.qq.com/rsc/fcgi-bin/fcg_user_created_diss?hostUin=0&hostuin={uin}\
&sin={begin}&size={size}&g_tk=5381&loginUin={uin}&format=json&inCharset=utf8\
&outCharset=utf-8&notice=0&platform=yqq.json&needNewCode=0"
    );
    let created = decode_json(
        super::with_cookie(http::client().get(created_url))
            .header("Referer", "https://y.qq.com/")
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?,
    )
    .await?;
    if created["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(FetchError::Other(
            "获取 QQ 个人歌单失败，登录可能已失效".to_string(),
        ));
    }

    let collected_url = format!(
        "https://c.y.qq.com/fav/fcgi-bin/fcg_get_profile_order_asset.fcg?format=json\
&inCharset=utf8&outCharset=utf-8&platform=yqq.json&needNewCode=0&loginUin={uin}\
&hostUin=0&notice=0&g_tk=5381&ct=20&cid=205360956&userid={uin}&reqtype=3\
&sin={begin}&ein={}",
        begin + size - 1
    );
    let collected = decode_json(
        super::with_cookie(http::client().get(collected_url))
            .header("Referer", "https://y.qq.com/")
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?,
    )
    .await?;
    if collected["code"].as_i64().unwrap_or(-1) != 0 {
        return Err(FetchError::Other(
            "获取 QQ 收藏歌单失败，登录可能已失效".to_string(),
        ));
    }

    let mut result = created_playlists(&created);
    for item in collected["data"]["cdlist"].as_array().into_iter().flatten() {
        if let Some(playlist) = parse_user_playlist(item)
            && !result.iter().any(|existing| existing.id == playlist.id)
        {
            result.push(playlist);
        }
    }
    Ok(result)
}

/// 创建列表里真正能镜像的歌单：`dirid=201` 是「我喜欢」（红心，单独走
/// `get_favorites`，否则会以两种 kind 重复进缓存）；`tid=0` 的是 QZone 背景
/// 音乐 / 本地上传这类系统目录，没有全局 id，详情接口也按 disstid 拉不到。
fn created_playlists(created: &Value) -> Vec<Playlist> {
    created["data"]["disslist"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            item["dirid"].as_u64() != Some(201) && value_u64(&item["tid"]).unwrap_or_default() != 0
        })
        .filter_map(parse_user_playlist)
        .collect()
}

/// QQ 网关对下线/受限接口会直接返回 404 + 空 body 或 JSONP，reqwest 的
/// `.json()` 在这种场景只给一句没有信息量的 "error decoding response body"。
/// 统一按文本取回、剥壳再解析，失败时带上状态码与 body 开头。
async fn decode_json(response: reqwest::Response) -> Result<Value, FetchError> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let body = strip_jsonp(&text);
    serde_json::from_str(&body).map_err(|error| {
        let snippet: String = body.chars().take(120).collect();
        FetchError::Parse(format!("HTTP {status}: {error}，body 开头：{snippet}"))
    })
}

/// 全部个人歌单（创建 + 收藏），自动翻页。
///
/// 同步镜像依赖**完整**列表：只拉第一页时，第 101 个之后的歌单会被
/// 「全部成功才整体替换」的镜像逻辑当成远端已删除而清掉（与网易云的
/// `get_all_user_playlists` 同一套约定）。
pub async fn get_all_user_playlists() -> Result<Vec<Playlist>, FetchError> {
    const PAGE_SIZE: u32 = 100;
    const MAX_PAGES: u32 = 20;
    let mut all: Vec<Playlist> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
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

/// 新建 QQ 音乐个人歌单。
pub async fn create_user_playlist(name: &str) -> Result<Playlist, FetchError> {
    let uin = super::session::snapshot().user_id.unwrap_or_default();
    if uin.is_empty() {
        return Err(FetchError::Other("QQ 登录缺少账号 ID".into()));
    }
    let url = "https://c.y.qq.com/splcloud/fcgi-bin/create_playlist.fcg?g_tk=5381";
    let response: Value = super::with_cookie(http::client().post(url))
        .header("Referer", "https://y.qq.com/n/yqq/playlist")
        .form(&[
            ("loginUin", uin.as_str()),
            ("hostUin", "0"),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf8"),
            ("notice", "0"),
            ("platform", "yqq"),
            ("needNewCode", "0"),
            ("g_tk", "5381"),
            ("uin", uin.as_str()),
            ("name", name),
            ("show", "1"),
            ("formsender", "1"),
            ("utf8", "1"),
            ("qzreferrer", "https://y.qq.com/n/yqq/playlist"),
        ])
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| FetchError::Parse(e.to_string()))?;
    match response["code"].as_i64() {
        Some(0) => {
            let id = response["dirid"]
                .as_i64()
                .or_else(|| response["data"]["dirid"].as_i64())
                .ok_or_else(|| FetchError::Other("QQ 创建歌单未返回 dirid".into()))?
                .to_string();
            Ok(Playlist::new(id, name, SourceId::Tx))
        }
        Some(21) => Err(FetchError::Other("QQ 歌单名称已存在".into())),
        Some(1) | Some(1000) => Err(FetchError::Other("QQ 登录已失效".into())),
        _ => Err(FetchError::Other(format!(
            "QQ 创建歌单失败: {}",
            response["msg"]
        ))),
    }
}

/// 向 QQ 歌单批量添加歌曲；QQ 接口使用 songmid。
pub async fn add_songs_to_playlist(dirid: &str, songs: &[SongInfo]) -> Result<usize, FetchError> {
    if songs.is_empty() {
        return Ok(0);
    }
    let uin = super::session::snapshot().user_id.unwrap_or_default();
    // QQ 按位置配对 midlist 与 typelist：先筛出真正带 songmid 的歌曲，再从同一份
    // 列表派生两个参数，否则空 id 会让两个列表错位、加错歌曲。
    let usable = songs
        .iter()
        .filter(|s| !s.id.is_empty())
        .map(|s| (s.id.as_str(), "13"))
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Err(FetchError::Other("QQ 添加歌曲缺少 song mid".into()));
    }
    let mids = usable
        .iter()
        .map(|(mid, _)| *mid)
        .collect::<Vec<_>>()
        .join(",");
    let typelist = usable
        .iter()
        .map(|(_, ty)| *ty)
        .collect::<Vec<_>>()
        .join(",");
    let submitted = usable.len();
    let url = "https://c.y.qq.com/splcloud/fcgi-bin/fcg_music_add2songdir.fcg?g_tk=5381";
    let response: Value = super::with_cookie(http::client().get(url))
        .header("Referer", "https://y.qq.com/n/yqq/playlist")
        .query(&[
            ("midlist", mids.as_str()),
            ("typelist", typelist.as_str()),
            ("dirid", dirid),
            ("addtype", ""),
            ("formsender", "4"),
            ("r2", "0"),
            ("r3", "1"),
            ("utf8", "1"),
            ("g_tk", "5381"),
            ("uin", uin.as_str()),
        ])
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| FetchError::Parse(e.to_string()))?;
    match response["code"].as_i64() {
        Some(0) => Ok(submitted),
        Some(1000) | Some(301) => Err(FetchError::Other("QQ 登录已失效".into())),
        _ => Err(FetchError::Other(format!(
            "QQ 添加歌曲失败: {}",
            response["msg"]
        ))),
    }
}

/// 从 QQ 歌单批量删除歌曲；接口要求 song id，而不是 songmid。
pub async fn remove_songs_from_playlist(
    dirid: &str,
    songs: &[SongInfo],
) -> Result<usize, FetchError> {
    if songs.is_empty() {
        return Ok(0);
    }
    let uin = super::session::snapshot().user_id.unwrap_or_default();
    // QQ 按位置配对 ids 与 types：先筛出真正带 songId 的歌曲，再从同一份列表
    // 派生两个参数，否则缺一条 songId 就会让后续位置错位、删错歌曲。
    let usable = songs
        .iter()
        .filter_map(|s| s.extra.get("songId").map(|id| (id.as_str(), "3")))
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Err(FetchError::Other("QQ 删除歌曲缺少 song id".into()));
    }
    let ids = usable
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()
        .join(",");
    let types = usable
        .iter()
        .map(|(_, ty)| *ty)
        .collect::<Vec<_>>()
        .join(",");
    let submitted = usable.len();
    let url = "https://c.y.qq.com/qzone/fcg-bin/fcg_music_delbatchsong.fcg?g_tk=5381";
    let response: Value = super::with_cookie(http::client().get(url))
        .header("Referer", "https://y.qq.com/n/yqq/playlist")
        .query(&[
            ("loginUin", uin.as_str()),
            ("hostUin", "0"),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
            ("platform", "yqq.post"),
            ("needNewCode", "0"),
            ("uin", uin.as_str()),
            ("dirid", dirid),
            ("ids", ids.as_str()),
            ("source", "103"),
            ("types", types.as_str()),
            ("formsender", "4"),
            ("flag", "2"),
            ("utf8", "1"),
            ("from", "3"),
        ])
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| FetchError::Parse(e.to_string()))?;
    match response["code"].as_i64() {
        Some(0) => Ok(submitted),
        Some(1000) | Some(301) => Err(FetchError::Other("QQ 登录已失效".into())),
        _ => Err(FetchError::Other(format!(
            "QQ 删除歌曲失败: {}",
            response["msg"]
        ))),
    }
}

/// 兼容两套字段命名：创建歌单接口用 `diss_name`/`song_cnt`/`listen_num`
/// 这类下划线风格，收藏歌单接口用 `dissname`/`songnum`/`listennum`。
fn parse_user_playlist(item: &Value) -> Option<Playlist> {
    let id = {
        let id = value_string(&item["dissid"]);
        if id.is_empty() {
            value_string(&item["tid"])
        } else {
            id
        }
    };
    let name = item["dissname"]
        .as_str()
        .or_else(|| item["diss_name"].as_str())
        .or_else(|| item["title"].as_str())?
        .trim()
        .to_string();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    let mut playlist = Playlist::new(id.clone(), name, SourceId::Tx);
    playlist.cover_url = item["logo"]
        .as_str()
        .or_else(|| item["picurl"].as_str())
        .or_else(|| item["diss_cover"].as_str())
        .filter(|url| !url.is_empty())
        .map(|url| url.replace("http://", "https://"));
    playlist.song_count = value_u64(&item["songnum"])
        .or_else(|| value_u64(&item["song_count"]))
        .or_else(|| value_u64(&item["song_cnt"]))
        .or_else(|| value_u64(&item["song_num"]))
        .unwrap_or_default() as u32;
    playlist.play_count = value_u64(&item["listennum"])
        .or_else(|| value_u64(&item["visitnum"]))
        .or_else(|| value_u64(&item["listen_num"]));
    playlist.creator = item["creatorname"]
        .as_str()
        .or_else(|| item["nickname"].as_str())
        .or_else(|| item["creator"]["name"].as_str())
        .map(str::to_string);
    playlist.link = Some(format!("https://y.qq.com/n/ryqq/playlist/{id}"));
    Some(playlist)
}

/// 歌单分类目录：老版 `fcg_get_diss_tag_conf` 接口（与 music-lib 一致）。
pub async fn get_categories() -> Result<Vec<PlaylistCategory>, FetchError> {
    let json: Value = super::with_cookie(http::client()
        .get("https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_tag_conf.fcg?format=json&inCharset=utf8&outCharset=utf-8"))
        .header("Referer", "https://y.qq.com/")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?
        .json()
        .await
        .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64().unwrap_or_default() != 0 {
        return Err(FetchError::Other("QQ 歌单分类请求失败".to_string()));
    }
    let mut categories = vec![PlaylistCategory {
        id: String::new(),
        name: "全部".to_string(),
        source: SourceId::Tx,
        group: Some("全部".to_string()),
        count: 0,
        hot: true,
        extra: Default::default(),
    }];
    for group in json["data"]["categories"].as_array().into_iter().flatten() {
        let group_name = group["categoryGroupName"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        for item in group["items"].as_array().into_iter().flatten() {
            // usable=0 的分类已经下架；10000000 是「全部」的哨兵值。
            if item["usable"].as_i64().unwrap_or_default() == 0 {
                continue;
            }
            let id = item["categoryId"].as_i64().unwrap_or_default();
            let name = item["categoryName"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_string();
            if id == 0 || id == 10_000_000 || name.is_empty() {
                continue;
            }
            let mut category = PlaylistCategory::new(id.to_string(), name, SourceId::Tx);
            category.group = (!group_name.is_empty()).then(|| group_name.clone());
            categories.push(category);
        }
    }
    Ok(categories)
}

/// 分类歌单：`fcg_get_diss_by_tag`（分类为空时用「全部」的哨兵 ID）。
pub async fn get_category_list(category: &str, page: u32) -> Result<Vec<Playlist>, FetchError> {
    let category_id = if category.trim().is_empty() {
        "10000000"
    } else {
        category.trim()
    };
    let limit = 30u32;
    let offset = (page.max(1) - 1) * limit;
    let url = format!(
        "https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_by_tag.fcg?picmid=1&rnd=0.1&g_tk=5381&loginUin=0&hostUin=0&format=json&inCharset=utf8&outCharset=utf-8&notice=0&platform=yqq.json&needNewCode=0&categoryId={category_id}&sortId=5&sin={offset}&ein={}",
        offset + limit - 1
    );
    let json: Value = super::with_cookie(http::client().get(url))
        .header("Referer", "https://y.qq.com/")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?
        .json()
        .await
        .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64().unwrap_or_default() != 0 {
        return Err(FetchError::Other("QQ 分类歌单请求失败".to_string()));
    }
    Ok(json["data"]["list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = value_string(&item["dissid"]);
            let name = item["dissname"].as_str()?.trim().to_string();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            let mut playlist = Playlist::new(id.clone(), name, SourceId::Tx);
            playlist.cover_url = item["imgurl"]
                .as_str()
                .filter(|url| !url.is_empty())
                .map(|url| url.replace("http://", "https://"));
            playlist.song_count = value_u64(&item["song_count"])
                .or_else(|| value_u64(&item["song_num"]))
                .unwrap_or_default() as u32;
            playlist.play_count = value_u64(&item["listennum"]);
            playlist.creator = item["creator"]["name"].as_str().map(str::to_string);
            playlist.description = non_empty_string(&item["introduction"]);
            playlist.link = Some(format!("https://y.qq.com/n/ryqq/playlist/{id}"));
            Some(playlist)
        })
        .collect())
}

/// 歌单搜索：老版 `client_music_search_songlist`，响应是 JSONP，需要剥壳。
pub async fn search_playlists(keyword: &str, page: u32) -> Result<Vec<Playlist>, SearchError> {
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_music_search_songlist?query={}&page_no={}&num_per_page=20&format=json&remoteplace=txt.yqq.playlist&flag_qc=0",
        urlencoding::encode(keyword),
        page.saturating_sub(1)
    );
    let text = super::with_cookie(http::client().get(url))
        .header("Referer", "https://y.qq.com/portal/search.html")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| SearchError::Network(error.to_string()))?
        .text()
        .await
        .map_err(|error| SearchError::Network(error.to_string()))?;
    let json: Value = serde_json::from_str(&strip_jsonp(&text))
        .map_err(|error| SearchError::Parse(error.to_string()))?;
    Ok(json["data"]["list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = value_string(&item["dissid"]);
            let name = item["dissname"].as_str()?.trim().to_string();
            if id.is_empty() || name.is_empty() {
                return None;
            }
            let mut playlist = Playlist::new(id.clone(), name, SourceId::Tx);
            playlist.cover_url = item["imgurl"]
                .as_str()
                .filter(|url| !url.is_empty())
                .map(|url| url.replace("http://", "https://"));
            playlist.song_count = value_u64(&item["song_count"]).unwrap_or_default() as u32;
            playlist.play_count = value_u64(&item["listennum"]);
            playlist.creator = item["creator"]["name"].as_str().map(str::to_string);
            playlist.link = Some(format!("https://y.qq.com/n/ryqq/playlist/{id}"));
            Some(playlist)
        })
        .collect())
}

/// 剥掉 JSONP 外壳：`callback({...})` → `{...}`。
pub(super) fn strip_jsonp(body: &str) -> String {
    let trimmed = body.trim();
    match (trimmed.find('('), trimmed.rfind(')')) {
        (Some(start), Some(end)) if end > start => trimmed[start + 1..end].trim().to_string(),
        _ => trimmed.to_string(),
    }
}

fn parse_playlist(item: &Value) -> Option<Playlist> {
    let id = value_string(&item["tid"]);
    let name = item["title"].as_str()?.trim().to_string();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(Playlist {
        id,
        name,
        source: SourceId::Tx,
        cover_url: non_empty_string(&item["cover_url_medium"]),
        song_count: item["song_ids"]
            .as_array()
            .map(|items| items.len() as u32)
            .unwrap_or_default(),
        description: non_empty_string(&item["desc"]),
        play_count: value_u64(&item["access_num"]),
        creator: None,
        link: None,
        extra: Default::default(),
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

/// QQ“我喜欢”专用集合。QQ 将它作为 dirid=201 的特殊歌单处理。
/// 这是读取接口；写入需要 QQ 当前版本的动态签名接口，统一留给 SyncProvider。
pub async fn get_favorites() -> Result<(Playlist, Vec<SongInfo>), FetchError> {
    let body = serde_json::json!({
        "comm": { "ct": 20, "cv": 1859, "uin": 0, "format": "json" },
        "music.srfDissInfo.DissInfo": {
            "method": "CgiGetDiss",
            "module": "music.srfDissInfo.DissInfo",
            "param": {
                "disstid": 0,
                "dirid": 201,
                "song_begin": 0,
                "song_num": 1000,
                "enc_host_uin": ""
            }
        }
    });
    let json: Value =
        super::with_cookie(http::client().post("https://u.y.qq.com/cgi-bin/musicu.fcg"))
            .header("Referer", "https://y.qq.com/")
            .json(&body)
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
            .map_err(|error| FetchError::Network(error.to_string()))?
            .json()
            .await
            .map_err(|error| FetchError::Parse(error.to_string()))?;
    let data = &json["music.srfDissInfo.DissInfo"]["data"];
    let items = data["songlist"]
        .as_array()
        .ok_or_else(|| FetchError::Parse("QQ 我喜欢歌曲列表为空".into()))?;
    let songs = items
        .iter()
        .filter_map(super::search::parse_song)
        .collect::<Vec<_>>();
    let mut playlist = Playlist::new("profile:favorites", "我喜欢", SourceId::Tx);
    playlist.song_count = data["total_song_num"]
        .as_u64()
        .unwrap_or(songs.len() as u64) as u32;
    playlist.link = Some("https://y.qq.com/n/ryqq/songDetail/".into());
    Ok((playlist, songs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_collected_playlist_shape() {
        // fcg_get_profile_order_asset.fcg 的 cdlist：dissid 是数字，字段无下划线。
        let item = json!({
            "dissid": 6800944944_i64,
            "dissname": "劲爆嗨曲中文dj舞曲",
            "songnum": 71,
            "listennum": 12345,
            "logo": "http://img.example/logo.jpg",
            "nickname": "某位用户",
        });
        let playlist = parse_user_playlist(&item).expect("should parse");
        assert_eq!(playlist.id, "6800944944");
        assert_eq!(playlist.name, "劲爆嗨曲中文dj舞曲");
        assert_eq!(playlist.song_count, 71);
        assert_eq!(playlist.play_count, Some(12345));
        assert_eq!(playlist.creator.as_deref(), Some("某位用户"));
        assert_eq!(
            playlist.cover_url.as_deref(),
            Some("https://img.example/logo.jpg")
        );
    }

    #[test]
    fn parses_created_playlist_shape() {
        // fcg_user_created_diss 的 disslist：id 在 tid，字段带下划线。
        let item = json!({
            "dirid": 3,
            "tid": 7758023113_i64,
            "diss_name": "我的自建歌单",
            "diss_cover": "http://img.example/cover.jpg",
            "song_cnt": 12,
            "listen_num": 0,
        });
        let playlist = parse_user_playlist(&item).expect("should parse");
        assert_eq!(playlist.id, "7758023113");
        assert_eq!(playlist.name, "我的自建歌单");
        assert_eq!(playlist.song_count, 12);
        assert_eq!(
            playlist.cover_url.as_deref(),
            Some("https://img.example/cover.jpg")
        );
    }

    #[test]
    fn created_playlists_skip_favorites_and_system_dirs() {
        // dirid=201 是「我喜欢」（红心单独走 get_favorites）；tid=0 的是
        // QZone 背景音乐 / 本地上传这类没有全局 id 的系统目录。
        let created = json!({
            "code": 0,
            "data": {
                "disslist": [
                    { "dirid": 205, "tid": 0, "diss_name": "QZone背景音乐", "song_cnt": 0 },
                    { "dirid": 201, "tid": 7758023113_i64, "diss_name": "我喜欢", "song_cnt": 267 },
                    { "dirid": 206, "tid": 0, "diss_name": "本地上传", "song_cnt": 0 },
                    { "dirid": 3, "tid": 6800944944_i64, "diss_name": "自建歌单", "song_cnt": 5 },
                ]
            }
        });
        let playlists = created_playlists(&created);
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].id, "6800944944");
        assert_eq!(playlists[0].name, "自建歌单");
    }
}
