//! QQ音乐搜索 API
//!
//! GET https://c.y.qq.com/soso/fcgi-bin/client_search_cp

use crate::http::SendWithRetry;
use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use lx_core::model::song::SongInfo;
use lx_core::model::source::{Quality, SourceId};
use lx_core::traits::source::{SearchError, SearchResult};
use serde_json::Value;

use super::super::http;

/// 从 JSON Value 提取 i64
fn json_i64(value: &Value) -> i64 {
    value.as_i64().unwrap_or(0)
}

/// 将文件大小值映射为音质
fn add_quality_if_positive(qualities: &mut BTreeSet<Quality>, size: i64, quality: Quality) {
    if size > 0 {
        qualities.insert(quality);
    }
}

/// 校验搜索响应是否值得送进 JSON 解析。
///
/// QQ 音乐在风控或异常时会返回 **HTTP 500 + 空 body**（实测 `content-length: 0`）。
/// 空 body 直接交给 `serde_json::from_str` 只会得到
/// `parse error: EOF while parsing a value at line 1 column 0` —— 看起来像解析器
/// 出了问题，实际原因是服务端 500。这里先把状态码与空响应挡下来，保留真实原因。
///
/// 注意：它**不改变**这一路的耗时。实测同一接口从 0.04s 到 5.1s 都出现过，
/// 那是 QQ 服务端返回 500 的延迟，客户端无法缩短；本函数只保证失败原因准确。
///
/// 只做判定，不改变重试与超时策略：状态码非 2xx 本来就不会被 `send_with_retry`
/// 重试（它只重试连接失败/超时），这里只是不再把响应体当 JSON 解析。
fn validate_search_response(status: reqwest::StatusCode, body: &str) -> Result<(), SearchError> {
    if !status.is_success() {
        return Err(SearchError::Network(format!(
            "QQ音乐搜索返回 HTTP {}",
            status.as_u16()
        )));
    }
    if body.trim().is_empty() {
        return Err(SearchError::Parse(format!(
            "QQ音乐搜索返回空响应（HTTP {}）",
            status.as_u16()
        )));
    }
    Ok(())
}

pub async fn search(keyword: &str, page: u32, limit: u32) -> Result<SearchResult, SearchError> {
    let url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?p={page}&n={limit}&w={}&format=json&new_json=1&cr=1&aggr=1&lossless=1",
        urlencoding::encode(keyword)
    );

    let client = http::client();
    let resp = super::with_cookie(client.get(&url))
        .header("Referer", "https://y.qq.com/")
        .header("User-Agent", "Mozilla/5.0")
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|e| SearchError::Network(e.to_string()))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| SearchError::Network(e.to_string()))?;

    validate_search_response(status, &text)?;

    let json: Value = serde_json::from_str(&text).map_err(|e| SearchError::Parse(e.to_string()))?;

    // 检查响应
    let code = json["code"].as_i64().unwrap_or(-1);
    if code != 0 {
        return Err(SearchError::Api(format!("tx search error: code={code}")));
    }

    let data = match &json["data"]["song"] {
        Value::Null => {
            return Ok(SearchResult {
                items: vec![],
                total: 0,
                has_more: false,
            });
        }
        d => d,
    };

    let total = data["totalnum"].as_u64().unwrap_or(0) as u32;
    let item_song = match &data["list"] {
        Value::Array(items) => items,
        _ => {
            return Ok(SearchResult {
                items: vec![],
                total,
                has_more: false,
            });
        }
    };

    let mut items = Vec::with_capacity(item_song.len());

    for item in item_song {
        if let Some(song) = parse_song(item) {
            items.push(song);
        }
    }

    let has_more = (page * limit) < total;

    Ok(SearchResult {
        items,
        total,
        has_more,
    })
}

pub(crate) fn parse_song(item: &Value) -> Option<SongInfo> {
    let mid = item["mid"].as_str().unwrap_or("").to_string();
    if mid.is_empty() {
        return None;
    }

    let name = item["title"].as_str().unwrap_or("").to_string();

    // 歌手名：用 、 连接
    let singer = match &item["singer"] {
        Value::Array(singers) => {
            let names: Vec<&str> = singers.iter().filter_map(|s| s["name"].as_str()).collect();
            names.join("、")
        }
        _ => String::new(),
    };

    let album_name = item["album"]
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let album_mid = item["album"]
        .get("mid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 封面 URL
    let cover_url = if !album_mid.is_empty() {
        Some(format!(
            "https://y.gtimg.cn/music/photo_new/T002R500x500M000{}.jpg",
            album_mid
        ))
    } else {
        None
    };

    // 时长 (interval 单位是秒)
    let duration_secs = json_i64(&item["interval"]) as u64;
    let duration = Duration::from_secs(duration_secs);

    // 音质
    let mut qualities = BTreeSet::new();
    let file = &item["file"];
    add_quality_if_positive(
        &mut qualities,
        json_i64(&file["size_128mp3"]),
        Quality::Low128,
    );
    add_quality_if_positive(
        &mut qualities,
        json_i64(&file["size_320mp3"]),
        Quality::High320,
    );
    add_quality_if_positive(&mut qualities, json_i64(&file["size_flac"]), Quality::Flac);
    add_quality_if_positive(
        &mut qualities,
        json_i64(&file["size_hires"]),
        Quality::Flac24,
    );

    // extra
    let mut extra = HashMap::new();
    if let Some(s) = item["id"].as_i64() {
        extra.insert("songId".to_string(), s.to_string());
    }
    if let Some(s) = item["file"]["media_mid"].as_str() {
        extra.insert("strMediaMid".to_string(), s.to_string());
    }
    for (key, json_key) in [
        ("size_128mp3", "size_128mp3"),
        ("size_320mp3", "size_320mp3"),
        ("size_flac", "size_flac"),
        ("size_hires", "size_hires"),
    ] {
        let size = json_i64(&file[json_key]);
        if size > 0 {
            extra.insert(key.to_string(), size.to_string());
        }
    }

    let mut song = SongInfo::new(mid, SourceId::Tx, name, singer);
    song.album_name = album_name;
    song.album_id = album_mid;
    song.duration = duration;
    song.cover_url = cover_url;
    song.qualities = qualities;
    song.extra = extra;

    Some(song)
}

#[cfg(test)]
mod tests {
    use super::validate_search_response;
    use lx_core::traits::source::SearchError;
    use reqwest::StatusCode;

    /// 实测形态：HTTP 500 + 空 body。过去被当成 JSON 解析失败（EOF），
    /// 报出的原因误导人；现在必须直接说出是 HTTP 500。
    #[test]
    fn http_500_with_empty_body_reports_the_status() {
        let error = validate_search_response(StatusCode::INTERNAL_SERVER_ERROR, "")
            .expect_err("HTTP 500 必须判定为失败");
        let message = error.to_string();
        assert!(
            message.contains("500"),
            "错误信息必须包含真实状态码，实际为: {message}"
        );
        assert!(
            !message.contains("EOF"),
            "不应再把它报成 JSON 解析错误，实际为: {message}"
        );
    }

    #[test]
    fn empty_or_blank_body_is_rejected_before_json_parsing() {
        for body in ["", "   ", "\n\t "] {
            let error =
                validate_search_response(StatusCode::OK, body).expect_err("空 body 必须判定为失败");
            assert!(
                error.to_string().contains("空响应"),
                "空 body 应给出明确原因，实际为: {error}"
            );
        }
    }

    #[test]
    fn json_body_passes_validation() {
        assert!(validate_search_response(StatusCode::OK, "{\"code\":0}").is_ok());
        // 非法 JSON 不在这里拦截：交给后面的 serde 报解析错误。
        assert!(validate_search_response(StatusCode::OK, "not json").is_ok());
    }

    #[test]
    fn other_error_statuses_are_rejected() {
        for status in [
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let error = validate_search_response(status, "{}").expect_err("非 2xx 必须判定为失败");
            assert!(
                matches!(error, SearchError::Network(_)),
                "非 2xx 应归类为网络/服务端错误"
            );
        }
    }
}
