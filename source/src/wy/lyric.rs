//! 网易云音乐歌词获取
//!
//! POST https://interface3.music.163.com/eapi/song/lyric/v1
//! 使用 eapi 加密

use std::sync::OnceLock;

use lx_core::model::lyric::LyricData;
use lx_core::model::song::SongInfo;
use lx_core::traits::source::FetchError;
use serde_json::Value;

use super::session;

/// 从响应中提取歌词字段（.lyric）
fn extract_lyric(root: &Value, path: &str) -> Option<String> {
    // path like "lrc.lyric" → root["lrc"]["lyric"]
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;
    for part in &parts {
        current = current.get(part)?;
    }
    current.as_str().map(|s| s.to_string())
}

/// 修正 YRC 时间标签：[mm:ss:ms] → [mm:ss.ms]
fn fix_yrc_timestamps(yrc: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"\[(\d+):(\d+):(\d+)\]").expect("valid YRC timestamp regex")
    });
    re.replace_all(yrc, "[$1:$2.$3]").to_string()
}

pub async fn get_lyric(song: &SongInfo) -> Result<LyricData, FetchError> {
    let url = "/api/song/lyric/v1";
    let data = serde_json::json!({
        "id": song.id,
        "cp": false,
        "tv": 0,
        "lv": 0,
        "rv": 0,
        "kv": 0,
        "yv": 0,
        "ytv": 0,
        "yrv": 0,
    });

    let json = super::eapi_post(
        "https://interface3.music.163.com/eapi/song/lyric/v1",
        url,
        &data,
    )
    .await?;

    // 检查响应码
    let code = json["code"].as_i64().unwrap_or(0);
    if code != 200 {
        if super::response_requires_login(&json) {
            session::mark_login_expired();
        }
        // 歌词获取失败不报错，返回空；但要留下日志，否则分不清
        // 「真的没歌词」和「接口被风控/会话失效」。
        tracing::debug!(
            "网易云歌词接口返回 code={code}，按无歌词处理: {}",
            song.name
        );
        return Ok(LyricData::default());
    }

    let lrc = extract_lyric(&json, "lrc.lyric").unwrap_or_default();
    let tlyric = extract_lyric(&json, "tlyric.lyric");
    let rlyric = extract_lyric(&json, "romalrc.lyric");
    let lxlyric = extract_lyric(&json, "yrc.lyric").map(|y| fix_yrc_timestamps(&y));

    Ok(LyricData {
        lyric: lrc.clone(),
        tlyric,
        rlyric,
        lxlyric,
        raw_lrc: Some(lrc),
    })
}
