//! kw 播放 URL 获取
//!
//! 流程（封面与播放地址并发请求）:
//! 1. 调用 musicInfo API 获取封面
//! 2. 调用 url API 获取播放地址，并校验响应为 http(s) 直链

use std::time::{SystemTime, UNIX_EPOCH};

use lx_core::model::song::SongInfo;
use lx_core::model::source::Quality;
use lx_core::traits::source::{FetchError, SongUrl};

use super::super::http;

/// 根据音质选择对应的 bitrate 参数值
fn quality_to_bitrate(quality: Quality) -> u32 {
    match quality {
        Quality::Flac24 => 4000,
        Quality::Flac => 2000,
        Quality::High320 => 320,
        Quality::Low128 => 128,
    }
}

/// 解析封面图片直链。
///
/// 优先走 musicInfo API；失败时退回 artistpicserver。后者不是图片地址，
/// 而是个跳转服务，响应体是一行纯文本形式的真实图片地址。
pub(super) async fn resolve_cover_url(client: &reqwest::Client, song_id: &str) -> Option<String> {
    match fetch_cover_url(client, song_id).await {
        Some(url) => Some(url),
        None => fetch_artist_pic_url(client, song_id).await,
    }
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// 通过 musicInfo API 获取封面图片 URL
async fn fetch_cover_url(client: &reqwest::Client, song_id: &str) -> Option<String> {
    let url = format!("http://www.kuwo.cn/api/www/music/musicInfo?mid={}", song_id);

    let resp = client
        .get(&url)
        .header("Referer", "http://www.kuwo.cn/")
        .header("csrf", song_id)
        .header("Cookie", format!("kw_token={}", song_id))
        .send()
        .await
        .ok()?;

    let text = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;

    if json["code"].as_i64() != Some(200) {
        return None;
    }

    json["data"]["pic"]
        .as_str()
        .map(str::trim)
        .filter(|pic| is_http_url(pic))
        .map(str::to_string)
}

/// 向 artistpicserver 要真实图片地址
async fn fetch_artist_pic_url(client: &reqwest::Client, song_id: &str) -> Option<String> {
    let url = format!(
        "http://artistpicserver.kuwo.cn/pic.web?corp=kuwo&type=rid_pic&pictype=500&size=500&rid={}",
        song_id
    );

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let pic = resp.text().await.ok()?.trim().to_string();
    is_http_url(&pic).then_some(pic)
}

pub async fn get_song_url(song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError> {
    let client = http::client();
    let song_id = &song.id;

    // Step 1: 构造播放 URL 请求
    let bitrate = quality_to_bitrate(quality);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| FetchError::Other(e.to_string()))?
        .as_millis();

    // 随机 10 位 reqId
    use rand::Rng;
    let req_id: u64 = rand::thread_rng().gen_range(1_000_000_000..10_000_000_000);

    let url = format!(
        "http://www.kuwo.cn/url?format=mp3&rid={}&response=url&type=convert_url3&br={}k&from=web&t={}&reqId={}",
        song_id, bitrate, timestamp, req_id
    );

    // 封面与播放地址并发请求，避免封面串行等待拖慢出歌
    let (cover_url, url_text) = tokio::join!(
        resolve_cover_url(&client, song_id),
        fetch_play_url(&client, &url)
    );
    let url_text = url_text?;

    // VIP 失效等场景接口会返回错误 JSON/文本，直接交给播放器会整体 loadfile 失败，
    // 这里校验必须是 http(s) 直链
    if !is_http_url(&url_text) {
        // 只记录前 200 字符，避免错误响应是大段 HTML 时刷日志
        tracing::warn!(
            "酷我返回的播放地址无效，疑似鉴权失败: {}",
            &url_text[..url_text.len().min(200)]
        );
        return Err(FetchError::NotFound);
    }

    // 转换 BTreeSet → Vec
    let qualities: Vec<Quality> = song.qualities.iter().copied().collect();

    Ok(SongUrl {
        url: url_text,
        quality,
        duration: song.duration,
        cover_url,
        qualities,
        headers: vec![],
    })
}

/// 请求播放地址响应文本（不校验内容，由调用方判断是否为直链）
async fn fetch_play_url(client: &reqwest::Client, url: &str) -> Result<String, FetchError> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(FetchError::NotFound);
    }

    resp.text()
        .await
        .map(|text| text.trim().to_string())
        .map_err(|e| FetchError::Network(e.to_string()))
}
