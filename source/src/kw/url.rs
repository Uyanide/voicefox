//! kw 播放 URL 获取
//!
//! 对齐 MusicBot-Go `plugins/kuwo/media.go` 的解析顺序：
//! 1. 移动端接口 `mobi.kuwo.cn/mobi.s?type=convert_url_with_sign`，按音质传 `br`
//!    （`128kmp3` / `320kmp3`），匿名可用；
//! 2. 网页版接口 `www.kuwo.cn/api/v1/www/music/playUrl`，需要会话签名
//!    （见 [`super::session`]），作为备用 CDN 与兜底；
//! 3. 封面从 musicInfo（签名）取，失败退回 artistpicserver。
//!
//! 注意：付费曲目在移动端接口上**不报错**，而是返回一个 11 秒占位曲目
//! （rid 与请求不符），因此必须校验返回的 rid 与时长，否则会把占位音频当成成品。

use serde_json::Value;

use crate::http::SendWithRetry;
use lx_core::model::song::SongInfo;
use lx_core::model::source::Quality;
use lx_core::traits::source::{FetchError, SongUrl};

use super::super::http;
use super::session;

const MOBILE_PLAY_API: &str = "https://mobi.kuwo.cn/mobi.s";
const WEB_PLAY_API: &str = "https://www.kuwo.cn/api/v1/www/music/playUrl";
const MUSIC_INFO_API: &str = "https://www.kuwo.cn/api/www/music/musicInfo";
/// 移动端接口要求的客户端标识（匿名可用）。
const MOBILE_SOURCE: &str = "kwplayer_ar_5.1.0.0_B_jiakong_vh.apk";
const MOBILE_USER: &str = "359307055300426";

/// 移动端可用的码率档位。无损档位该接口拿不到（会回占位曲目），
/// 因此 hi-res / 无损请求按 320k → 128k 降级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MobileTier {
    br: &'static str,
    format: &'static str,
    quality: Quality,
}

const TIER_128: MobileTier = MobileTier {
    br: "128kmp3",
    format: "mp3",
    quality: Quality::Low128,
};
const TIER_320: MobileTier = MobileTier {
    br: "320kmp3",
    format: "mp3",
    quality: Quality::High320,
};

fn mobile_tiers(quality: Quality) -> &'static [MobileTier] {
    match quality {
        Quality::Low128 => &[TIER_128],
        Quality::High320 => &[TIER_320, TIER_128],
        // 无损需要 legacy `convert_url2` + DES 查询串，暂未实现，先降级。
        Quality::Flac | Quality::Flac24 => &[TIER_320, TIER_128],
    }
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// 酷我的 JSON 字段时而是字符串时而是数字（`rid` 实测为数字），统一取文本比较。
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn scalar_u64(value: &Value) -> u64 {
    match value {
        Value::Number(number) => number.as_u64().unwrap_or(0),
        Value::String(text) => text.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

/// 解析封面图片直链。
///
/// 优先走 musicInfo API（签名后可正常返回）；失败时退回 artistpicserver。
/// 后者不是图片地址，而是个跳转服务，响应体是一行纯文本形式的真实图片地址。
pub(super) async fn resolve_cover_url(client: &reqwest::Client, song_id: &str) -> Option<String> {
    match fetch_cover_url(client, song_id).await {
        Some(url) => Some(url),
        None => fetch_artist_pic_url(client, song_id).await,
    }
}

/// 通过 musicInfo API 获取封面图片 URL（需要会话签名）。
async fn fetch_cover_url(client: &reqwest::Client, song_id: &str) -> Option<String> {
    let cookie = session::session_token();
    let url = session::with_request_id(
        &format!("{MUSIC_INFO_API}?mid={song_id}&httpsStatus=1"),
        &session::request_id(),
    );
    let resp = session::sign(client.get(&url), &cookie, session::random_nonce())
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .ok()?;

    let text = resp.text().await.ok()?;
    if session::is_illegal_session(&text) {
        session::invalidate();
        return None;
    }
    let json: Value = serde_json::from_str(&text).ok()?;
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

    let resp = client
        .get(&url)
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let pic = resp.text().await.ok()?.trim().to_string();
    is_http_url(&pic).then_some(pic)
}

/// 移动端接口：一次请求拿到指定码率的直链。
async fn fetch_mobile_url(
    client: &reqwest::Client,
    song_id: &str,
    tier: MobileTier,
) -> Result<String, FetchError> {
    let url = format!(
        "{MOBILE_PLAY_API}?f=web&source={MOBILE_SOURCE}&type=convert_url_with_sign&sig=0&network=WIFI&br={}&format={}&rid={}&user={}",
        tier.br, tier.format, song_id, MOBILE_USER
    );
    let resp = client
        .get(&url)
        .header("User-Agent", session::WEB_UA)
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError::Network(format!("HTTP {}", resp.status())));
    }
    let text = resp
        .text()
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let json: Value =
        serde_json::from_str(&text).map_err(|error| FetchError::Parse(error.to_string()))?;

    if json["code"].as_i64() != Some(200) {
        return Err(FetchError::NotFound);
    }
    let data = &json["data"];
    let play_url = data["url"]
        .as_str()
        .map(str::trim)
        .filter(|url| is_http_url(url))
        .ok_or(FetchError::NotFound)?;

    // 付费曲目会返回 rid 与时长都不匹配的占位音频，必须挡掉。
    let returned_rid = scalar_text(&data["rid"]);
    let duration = scalar_u64(&data["duration"]);
    if returned_rid != song_id || duration == 0 {
        tracing::debug!(
            "酷我移动端返回占位曲目（请求 rid={song_id}，返回 rid={returned_rid}，时长={duration}s）"
        );
        return Err(FetchError::NotFound);
    }
    Ok(play_url.to_string())
}

/// 网页版接口：会话签名后拿直链，付费曲目返回 `code=-1`。
async fn fetch_web_url(client: &reqwest::Client, song_id: &str) -> Result<String, FetchError> {
    let cookie = session::session_token();
    let url = session::with_request_id(
        &format!("{WEB_PLAY_API}?mid={song_id}&type=music&httpsStatus=1"),
        &session::request_id(),
    );
    let resp = session::sign(client.get(&url), &cookie, session::random_nonce())
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !resp.status().is_success() {
        return Err(FetchError::Network(format!("HTTP {}", resp.status())));
    }
    let text = resp
        .text()
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let json: Value =
        serde_json::from_str(&text).map_err(|error| FetchError::Parse(error.to_string()))?;

    let code = json["code"].as_i64().unwrap_or(-1);
    if code != 200 {
        if session::is_illegal_session(&text) {
            session::invalidate();
        }
        // 常见：code=-1 付费内容；其它码多为版权/地区限制。
        tracing::debug!(
            "酷我网页接口不可用: code={code} msg={}",
            json["msg"].as_str().unwrap_or_default()
        );
        return Err(FetchError::NotFound);
    }
    json["data"]["url"]
        .as_str()
        .map(str::trim)
        .filter(|url| is_http_url(url))
        .map(str::to_string)
        .ok_or(FetchError::NotFound)
}

pub async fn get_song_url(song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError> {
    let client = http::client();
    let song_id = &song.id;

    // 封面、移动端直链、网页直链并发：网页直链既做候选地址，也在移动端失败时兜底。
    let (cover_url, mobile, web) = tokio::join!(
        resolve_cover_url(&client, song_id),
        resolve_mobile_url(&client, song_id, quality),
        fetch_web_url(&client, song_id)
    );

    let web_url = web.ok();
    let (play_url, achieved) = match mobile {
        Ok(found) => found,
        Err(error) => match web_url.clone() {
            Some(url) => {
                tracing::debug!("酷我移动端不可用（{error}），改用网页直链");
                // 网页接口不声明档位，按 mp3 处理。
                (url, Quality::High320)
            }
            None => return Err(error),
        },
    };

    let mut candidate_urls = Vec::new();
    if let Some(url) = web_url.filter(|url| *url != play_url) {
        candidate_urls.push(url);
    }

    let qualities: Vec<Quality> = song.qualities.iter().copied().collect();

    Ok(SongUrl {
        url: play_url,
        quality: achieved,
        duration: song.duration,
        cover_url,
        qualities,
        // 直链要求 `okhttp` 客户端 UA，否则部分 CDN 会 403。
        headers: vec![("User-Agent".to_string(), session::MEDIA_UA.to_string())],
        // 酷我没有回传文件体积的接口：`musicInfo` 的字段里没有 size
        // （只有 duration/hasLossless 等），移动端接口也只有 bitrate/duration。
        // 文件大小由下载引擎的 HEAD/Range 探测得到，不在这里假装声明。
        size: None,
        size_is_advisory: false,
        md5: None,
        candidate_urls,
        max_chunk_size: 0,
    })
}

/// 依次尝试各档位，返回第一个可用的直链与对应音质。
async fn resolve_mobile_url(
    client: &reqwest::Client,
    song_id: &str,
    quality: Quality,
) -> Result<(String, Quality), FetchError> {
    let mut last_error = FetchError::NotFound;
    for tier in mobile_tiers(quality) {
        match fetch_mobile_url(client, song_id, *tier).await {
            Ok(url) => {
                if tier.quality != quality {
                    tracing::debug!(
                        "酷我 {quality:?} 档位不可用，已降级到 {:?}: {song_id}",
                        tier.quality
                    );
                }
                return Ok((url, tier.quality));
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_tiers_downgrade_from_the_requested_quality() {
        assert_eq!(mobile_tiers(Quality::Low128), &[TIER_128]);
        assert_eq!(mobile_tiers(Quality::High320), &[TIER_320, TIER_128]);
        // 无损当前只能降级到 mp3 档位。
        assert_eq!(mobile_tiers(Quality::Flac), &[TIER_320, TIER_128]);
        assert_eq!(mobile_tiers(Quality::Flac24), &[TIER_320, TIER_128]);
    }

    #[test]
    fn placeholder_tracks_are_rejected() {
        // 付费曲目：rid 与时长都对不上（实测返回 rid=260839262 / 11s）。
        let placeholder = serde_json::json!({
            "code": 200,
            "data": {"rid": 260839262, "duration": 11, "bitrate": 1, "url": "http://kw-er.kuwo.cn/x.mp3"}
        });
        assert_ne!(scalar_text(&placeholder["data"]["rid"]), "228908");
        assert!(scalar_u64(&placeholder["data"]["duration"]) > 0);

        // 正常曲目：rid 是数字类型，也必须能被正确识别。
        let good = serde_json::json!({
            "code": 200,
            "data": {"rid": 6550104, "duration": 232, "bitrate": 320, "url": "http://kw-bj.kuwo.cn/x.mp3"}
        });
        assert_eq!(scalar_text(&good["data"]["rid"]), "6550104");
        assert_eq!(scalar_u64(&good["data"]["duration"]), 232);
        // 字符串形态的老响应同样兼容。
        assert_eq!(scalar_text(&serde_json::json!("6550104")), "6550104");
    }

    #[test]
    fn only_http_urls_are_accepted() {
        assert!(is_http_url("http://kw-bj.kuwo.cn/a.mp3"));
        assert!(is_http_url("https://kw-bj.kuwo.cn/a.mp3"));
        assert!(!is_http_url("该歌曲为付费内容"));
        assert!(!is_http_url(""));
    }
}
