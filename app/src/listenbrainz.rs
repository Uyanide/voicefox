//! ListenBrainz 听歌记录上报。
//!
//! 在 `[integration] listenbrainz_token` 里填入用户 token 后，开始播放满
//! [`SUBMIT_DELAY`] 秒即向 ListenBrainz 提交一条 listen（submit-listens API）。
//! token 留空时整条链路不启动，不产生任何网络请求。
//!
//! 提交是尽力而为的：失败只记日志，不打扰播放，也不做本地排队重发——
//! 听歌记录允许偶尔漏一条，不值得为它维护一个持久化队列。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;
use std::time::Duration;

use lx_core::model::song::SongInfo;
use lx_core::model::source::PlayerState;

/// 开始播放多少秒后才上报：太短会把「切歌试听」也记成完整收听，
/// 30 秒与播放时自动缓存（`download.auto_cache_after_secs` 默认值）同口径。
const SUBMIT_DELAY: Duration = Duration::from_secs(30);

/// 调度一次上报：[`SUBMIT_DELAY`] 秒后仍是同一次播放请求、且确实在播放，
/// 才提交这条 listen。切歌 / 暂停 / 停止都会让这次上报静默作废。
/// token 留空时直接不调度（配置为空 = 功能关闭）。
pub fn schedule(
    rt: &tokio::runtime::Runtime,
    ctx: &crate::context::AppContext,
    play_request_id: u64,
    song: SongInfo,
) {
    let token = {
        let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
        config.integration.listenbrainz_token.trim().to_string()
    };
    if token.is_empty() {
        return;
    }
    let player_state = ctx.player_state.clone();
    let current_request = ctx.play_request_id.clone();
    rt.spawn(async move {
        tokio::time::sleep(SUBMIT_DELAY).await;
        if current_request.load(Ordering::SeqCst) != play_request_id {
            return;
        }
        if *player_state.borrow() != PlayerState::Playing {
            return;
        }
        match submit(&token, &song).await {
            Ok(()) => tracing::debug!("ListenBrainz 已上报: {} - {}", song.name, song.singer),
            Err(error) => tracing::warn!(
                "ListenBrainz 上报失败（{} - {}）: {error}",
                song.name,
                song.singer
            ),
        }
    });
}

/// 提交一条 single listen。元数据缺失（无歌名 / 无歌手）时直接跳过：
/// ListenBrainz 会拒绝这样的记录，重试也没有意义。
async fn submit(token: &str, song: &SongInfo) -> Result<(), String> {
    if song.name.trim().is_empty() || song.singer.trim().is_empty() {
        return Ok(());
    }
    let listened_at = chrono::Utc::now().timestamp();
    let body = serde_json::json!({
        "listen_type": "single",
        "payload": [{
            "listened_at": listened_at,
            "track_metadata": {
                "artist_name": song.singer,
                "track_name": song.name,
                "release_name": (!song.album_name.trim().is_empty()).then(|| song.album_name.clone()),
                "additional_info": {
                    "media_player": "voicefox",
                    "submission_client": "voicefox",
                    "duration_ms": song.duration.as_millis().min(u128::from(u64::MAX)) as u64,
                },
            },
        }],
    });
    let response = http_client()
        .post("https://api.listenbrainz.org/1/submit-listens")
        .header("Authorization", format!("Token {token}"))
        .timeout(Duration::from_secs(10))
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("请求失败: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        // 响应体里有官方的 error 描述，截一段放进日志方便排查（token 写错等）。
        let detail = response.text().await.unwrap_or_default();
        return Err(format!(
            "HTTP {}: {}",
            status.as_u16(),
            detail.chars().take(200).collect::<String>()
        ));
    }
    Ok(())
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(name: &str, singer: &str) -> SongInfo {
        SongInfo::new(
            "id".to_string(),
            lx_core::model::source::SourceId::Kw,
            name.to_string(),
            singer.to_string(),
        )
    }

    #[test]
    fn submissions_without_metadata_are_skipped() {
        // 不发请求、不报错：缺元数据的记录在本地就被丢掉。
        let missing_name = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(submit("token", &song("  ", "歌手")));
        assert_eq!(missing_name, Ok(()));
        let missing_artist = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(submit("token", &song("歌名", "")));
        assert_eq!(missing_artist, Ok(()));
    }
}
