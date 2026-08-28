//! kg 播放 URL 获取
//!
//! 流程:
//! 1. POST http://gateway.kugou.com/v3/album_audio/audio 获取歌曲详情
//! 2. 从响应中提取 play_url

use std::time::{SystemTime, UNIX_EPOCH};

use lx_core::model::song::SongInfo;
use lx_core::model::source::Quality;
use lx_core::traits::source::{FetchError, SongUrl};

use super::super::http;

/// 根据请求音质选择对应的 hash 字段；请求字段缺失时按
/// SQFileHash > HQFileHash > FileHash 回退，并返回实际选中的音质，
/// 保证 SongUrl.quality 与真实下发的流一致。
fn select_hash(song: &SongInfo, quality: Quality) -> Option<(String, Quality)> {
    let field = match quality {
        Quality::Low128 => "FileHash",
        Quality::High320 => "HQFileHash",
        Quality::Flac => "SQFileHash",
        Quality::Flac24 => "ResFileHash",
    };
    if let Some(hash) = song.extra.get(field).filter(|hash| !hash.is_empty()) {
        return Some((hash.clone(), quality));
    }
    // 回退链（沿用旧逻辑）：SQ > HQ > File
    const FALLBACK: [(&str, Quality); 3] = [
        ("SQFileHash", Quality::Flac),
        ("HQFileHash", Quality::High320),
        ("FileHash", Quality::Low128),
    ];
    FALLBACK
        .iter()
        .find_map(|(key, fallback_quality)| {
            song.extra
                .get(*key)
                .filter(|hash| !hash.is_empty())
                .map(|hash| (hash.clone(), *fallback_quality))
        })
}

pub async fn get_song_url(song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError> {
    let client = http::client();

    let (hash, actual_quality) = select_hash(song, quality).ok_or(FetchError::NotFound)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| FetchError::Other(e.to_string()))?
        .as_millis();

    // 构造请求体
    let body = serde_json::json!({
        "area_code": "1",
        "data": [{"hash": hash}],
        "key": "OIlwieks28dk2k092lksi2UIkp",
        "appid": 1005,
        "clientver": 11451,
        "mid": "1",
        "dfid": "-",
        "clienttime": now
    });

    let resp = client
        .post("http://gateway.kugou.com/v3/album_audio/audio")
        .header("KG-THash", "13a3164")
        .header("KG-RC", "1")
        .header("KG-Fake", "0")
        .header("KG-RF", "00869891")
        .header(
            "User-Agent",
            "Android712-AndroidPhone-11451-376-0-FeeCacheUpdate-wifi",
        )
        .header("x-router", "kmr.service.kugou.com")
        .json(&body)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(FetchError::NotFound);
    }

    let text = resp
        .text()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| FetchError::Parse(e.to_string()))?;

    let data = match &json["data"] {
        serde_json::Value::Array(arr) if !arr.is_empty() => &arr[0],
        _ => return Err(FetchError::NotFound),
    };

    let play_url = data["play_url"].as_str().unwrap_or("").to_string();

    if play_url.is_empty() {
        return Err(FetchError::NotFound);
    }

    let qualities: Vec<Quality> = song.qualities.iter().copied().collect();

    Ok(SongUrl {
        url: play_url,
        quality: actual_quality,
        duration: song.duration,
        cover_url: None,
        qualities,
        headers: vec![],
    })
}
