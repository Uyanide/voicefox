//! 网易云音乐播放 URL 获取
//!
//! 主路径：官方 eapi 接口 `/api/song/enhance/player/url/v1`
//! （加密方式与搜索、歌词一致，见 `crypto::eapi`），一次请求即可拿到
//! 播放地址 + `size` + `md5`，下载侧因此能做完整性校验。
//!
//! 兜底：接口无版权/风控拿不到地址时，退回公开重定向 URL
//! `https://music.163.com/song/media/outer/url?id={id}.mp3`，代价是拿不到 size。

use serde_json::Value;

use lx_core::model::song::SongInfo;
use lx_core::model::source::Quality;
use lx_core::traits::source::{FetchError, SongUrl};

use crate::http::SendWithRetry;

use super::super::http;
use super::crypto;

/// eapi 签名用的路径，请求也发到同一路径。
const PLAYER_URL_API: &str = "/api/song/enhance/player/url/v1";
const PLAYER_URL_ENDPOINT: &str = "https://music.163.com/api/song/enhance/player/url/v1";

/// 校验兜底地址可播放性的单次请求上限。
///
/// 实测该接口有效 id 0.07s、无效 id 0.15s，3s 已是二十倍以上余量；这里刻意
/// 不放宽到 HTTP client 默认的 15s —— 校验的意义就是"拿不准就快点失败"，
/// 让 `resolve_playable_song` 及时转入跨平台换源，而不是先交给 mpv 播一次坏地址。
const OUTER_URL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 请求音质对应的降级阶梯：高音质拿不到可播地址时逐级下调，
/// 避免「要 FLAC 但账号只有 320k」直接判定整首不可播。
fn level_ladder(quality: Quality) -> &'static [(&'static str, Quality)] {
    match quality {
        Quality::Flac24 => &[
            ("hires", Quality::Flac24),
            ("lossless", Quality::Flac),
            ("exhigh", Quality::High320),
            ("standard", Quality::Low128),
        ],
        Quality::Flac => &[
            ("lossless", Quality::Flac),
            ("exhigh", Quality::High320),
            ("standard", Quality::Low128),
        ],
        Quality::High320 => &[("exhigh", Quality::High320), ("standard", Quality::Low128)],
        Quality::Low128 => &[("standard", Quality::Low128)],
    }
}

/// 无损档位用 flac 容器，其余用 mp3。
fn encode_type(quality: Quality) -> &'static str {
    match quality {
        Quality::Flac | Quality::Flac24 => "flac",
        Quality::Low128 | Quality::High320 => "mp3",
    }
}

/// 官方接口返回的可播地址及其校验信息。
struct OfficialUrl {
    url: String,
    quality: Quality,
    size: Option<u64>,
    md5: Option<String>,
}

fn json_u64(value: &Value) -> Option<u64> {
    value.as_u64().filter(|&size| size > 0)
}

fn json_non_empty_str(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 调官方接口拿地址。返回 `Ok(None)` 表示该档位不可播（应继续降级或走兜底）。
async fn fetch_official_url(
    song: &SongInfo,
    quality: Quality,
) -> Result<Option<OfficialUrl>, FetchError> {
    let client = http::client();
    let mut last_error: Option<FetchError> = None;

    for (level, achieved) in level_ladder(quality) {
        let data = serde_json::json!({
            "ids": format!("[{}]", song.id),
            "level": level,
            "encodeType": encode_type(*achieved),
        });
        let encrypted = crypto::eapi(PLAYER_URL_API, &data);

        let resp = match super::with_cookie(client.post(PLAYER_URL_ENDPOINT))
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .header("origin", "https://music.163.com")
            .header("Referer", "https://music.163.com/")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("params={encrypted}"))
            .send_with_retry(crate::http::RETRY_ATTEMPTS)
            .await
        {
            Ok(resp) => resp,
            Err(error) => {
                last_error = Some(FetchError::Network(error.to_string()));
                continue;
            }
        };

        if !resp.status().is_success() {
            last_error = Some(FetchError::Network(format!("HTTP {}", resp.status())));
            continue;
        }

        // 接口偶发返回非 JSON（例如风控会返回拼接的 {"msg":"参数错误","code":400}），
        // 这种情况不当作整首失败，继续降级/走兜底。
        let text = match resp.text().await {
            Ok(text) => text,
            Err(error) => {
                last_error = Some(FetchError::Network(error.to_string()));
                continue;
            }
        };
        let json: Value = match serde_json::from_str(&text) {
            Ok(json) => json,
            Err(error) => {
                tracing::debug!("网易云 URL 接口返回非 JSON 响应（{level} 档位）: {error}");
                last_error = Some(FetchError::Parse(error.to_string()));
                continue;
            }
        };

        let Some(item) = json["data"].as_array().and_then(|items| items.first()) else {
            continue;
        };

        // 试听片段（freeTrialInfo 非空）不是完整音频，直接判为该档位不可用，
        // 否则会把 30 秒片段当成整首歌落盘。
        if !item["freeTrialInfo"].is_null() {
            tracing::debug!("网易云 {level} 档位只返回试听片段，跳过: {}", song.name);
            continue;
        }

        let Some(url) = json_non_empty_str(&item["url"]) else {
            continue;
        };

        return Ok(Some(OfficialUrl {
            url,
            quality: *achieved,
            size: json_u64(&item["size"]),
            md5: json_non_empty_str(&item["md5"]).map(|md5| md5.to_ascii_lowercase()),
        }));
    }

    if let Some(error) = last_error {
        tracing::warn!("网易云官方 URL 接口不可用，回退公开重定向地址: {error}");
    }
    Ok(None)
}

/// 校验网易公开重定向地址是否**确实是可播放音频**。
///
/// 实测（2026-09，公网）该接口的两种响应：
///
/// | id | 状态 | 最终 host | content-type |
/// |---|---|---|---|
/// | 有效 | **200** | `m701.music.126.net`（已 302 到 CDN） | `audio/mpeg` |
/// | 伪造 | **200** | `music.163.com`（未跳转，107KB 错误页） | `text/html;charset=utf8` |
///
/// 由此确定三条策略：
///
/// 1. **不能只看状态码** —— 有效与无效都是 200；
/// 2. **不用 `Range`** —— 实测该接口对无效 id 忽略 Range、照样返回整个 HTML
///    错误页（107KB），既不省流量也不省时间；
/// 3. **优先用 HEAD** —— 实测可用且最快（0.07s / 0.15s），只读响应头、不碰 body。
///    仅当服务端明确说这个方法不支持（405/501）时才退回 GET，且同样只看响应头、
///    拿到头部就丢弃 body，不下载音频内容。
///
/// 判定口径是保守的：只有 `audio/*` 或 `application/octet-stream` 才算确认可播；
/// 明确的 `text/*`、`application/json` 等一律判失败；连 `content-type` 都没有时
/// 也判失败（无法确认 → 交给跨平台换源，而不是塞给 mpv 一个坏地址）。
///
/// 这里刻意**不做重试**：校验要的是快速失败，不是最终成功。
async fn verify_outer_url_playable(url: &str) -> Result<(), FetchError> {
    let client = http::client();
    let head = client
        .head(url)
        .timeout(OUTER_URL_PROBE_TIMEOUT)
        .send()
        .await;

    // HEAD 不可用（个别服务端/网关不支持）时才换 GET；网络类失败直接失败，
    // 不再试第二次 —— 那只会把"快速失败"拖成两倍耗时。
    let response = match head {
        Ok(response)
            if matches!(
                response.status().as_u16(),
                405 | 501 // Method Not Allowed / Not Implemented
            ) =>
        {
            tracing::debug!(
                "网易兜底地址不支持 HEAD（HTTP {}），改用 GET 只读响应头",
                response.status().as_u16()
            );
            client
                .get(url)
                .timeout(OUTER_URL_PROBE_TIMEOUT)
                .send()
                .await
                .map_err(|error| {
                    FetchError::Network(format!("网易兜底地址校验请求失败: {error}"))
                })?
        }
        Ok(response) => response,
        Err(error) => {
            return Err(FetchError::Network(format!(
                "网易兜底地址校验请求失败: {error}"
            )));
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::Other(format!(
            "网易兜底地址返回 HTTP {}",
            status.as_u16()
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .to_ascii_lowercase()
        });

    match content_type.as_deref() {
        Some(kind) if kind.starts_with("audio/") || kind == "application/octet-stream" => Ok(()),
        Some(kind) => Err(FetchError::Other(format!(
            "网易兜底地址的内容不是音频（content-type: {kind}），判定为不可播放"
        ))),
        None => Err(FetchError::Other(
            "网易兜底地址未声明 content-type，无法确认可播放".to_string(),
        )),
    }
}

pub async fn get_song_url(song: &SongInfo, quality: Quality) -> Result<SongUrl, FetchError> {
    let qualities: Vec<Quality> = song.qualities.iter().copied().collect();

    if let Some(official) = fetch_official_url(song, quality).await? {
        return Ok(SongUrl {
            url: official.url,
            quality: official.quality,
            duration: song.duration,
            cover_url: song.cover_url.clone(),
            qualities,
            headers: vec![],
            size: official.size,
            size_is_advisory: false,
            md5: official.md5,
            // 备用 CDN 由下载引擎按 m8/m801/m804/m704 → m7/m701 改写补全。
            candidate_urls: vec![],
            max_chunk_size: 0,
        });
    }

    // 兜底：公开重定向 URL（reqwest 自动跟随 302），拿不到 size/md5。
    let url = format!(
        "https://music.163.com/song/media/outer/url?id={}.mp3",
        song.id
    );
    // 这个 fallback **不校验 id**：不存在的 id 同样返回 200，只是内容是网易的
    // HTML 错误页。不先确认就返回，会让解析层"假成功"，只能等 mpv 播失败后
    // 再走一遍重试（实测多花约 6.65s）。所以这里必须先确认拿到的是音频。
    verify_outer_url_playable(&url).await?;
    Ok(SongUrl {
        url,
        quality,
        duration: song.duration,
        cover_url: song.cover_url.clone(),
        qualities,
        headers: vec![],
        size: None,
        size_is_advisory: false,
        md5: None,
        candidate_urls: vec![],
        max_chunk_size: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ladder_downgrades_towards_standard() {
        let ladder = level_ladder(Quality::Flac24);
        assert_eq!(ladder.first().unwrap().0, "hires");
        assert_eq!(ladder.last().unwrap().0, "standard");
        // 逐级下降，不会跳到更高档位。
        let levels: Vec<&str> = ladder.iter().map(|(level, _)| *level).collect();
        assert_eq!(levels, vec!["hires", "lossless", "exhigh", "standard"]);

        assert_eq!(level_ladder(Quality::Low128).len(), 1);
    }

    #[test]
    fn encode_type_follows_the_achieved_quality() {
        assert_eq!(encode_type(Quality::Flac), "flac");
        assert_eq!(encode_type(Quality::Flac24), "flac");
        assert_eq!(encode_type(Quality::High320), "mp3");
    }

    #[test]
    fn size_and_md5_parsing_ignores_empty_values() {
        assert_eq!(json_u64(&serde_json::json!(4276601)), Some(4276601));
        assert_eq!(json_u64(&serde_json::json!(0)), None);
        assert_eq!(json_u64(&serde_json::json!(null)), None);

        assert_eq!(
            json_non_empty_str(&serde_json::json!("A0634034446F904929E37DC2686BA91B")),
            Some("A0634034446F904929E37DC2686BA91B".to_string())
        );
        assert_eq!(json_non_empty_str(&serde_json::json!("  ")), None);
        assert_eq!(json_non_empty_str(&serde_json::json!(null)), None);
    }

    // ── 兜底地址可用性校验（本地 mock，不碰公网）──

    /// mock 响应：状态码 + content-type。
    #[derive(Clone, Copy)]
    struct Reply {
        code: u16,
        reason: &'static str,
        content_type: Option<&'static str>,
    }

    /// 实测的有效响应形态：200 + audio/mpeg。
    const AUDIO: Reply = Reply {
        code: 200,
        reason: "OK",
        content_type: Some("audio/mpeg; charset=UTF-8"),
    };
    /// 实测的无效 id 响应形态：200 + 网易 HTML 错误页。
    const HTML_ERROR_PAGE: Reply = Reply {
        code: 200,
        reason: "OK",
        content_type: Some("text/html;charset=utf8"),
    };

    enum MockReply {
        Answer(Reply),
        /// 建立连接后不返回任何内容，用于触发客户端超时。
        Hang,
    }

    /// 起一个极简 mock HTTP 服务器，按请求方法分别返回指定响应。
    ///
    /// 沿用仓库已有的 `TcpListener` 手写 HTTP 方式（见 `source/tests/js_user_api.rs`），
    /// 不引入新的测试依赖，也不让单测依赖网易公网。
    /// 线程带截止时间，最多服务若干次请求，避免测试结束时挂着不退出。
    fn spawn_mock(head: MockReply, get: MockReply) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("绑定 mock 端口");
        let address = listener.local_addr().expect("读取 mock 地址");
        listener.set_nonblocking(true).expect("mock 设为非阻塞");
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            while std::time::Instant::now() < deadline {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => return,
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .ok();
                let mut buffer = [0_u8; 1024];
                let read = std::io::Read::read(&mut stream, &mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let is_head = request.starts_with("HEAD");
                let reply = if is_head { &head } else { &get };
                match reply {
                    MockReply::Hang => {
                        // 不响应，保持连接直到客户端自己超时。
                        std::thread::sleep(std::time::Duration::from_secs(4));
                    }
                    MockReply::Answer(reply) => {
                        let body = "voicefox";
                        let mut response = format!(
                            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                            reply.code,
                            reply.reason,
                            body.len()
                        );
                        if let Some(content_type) = reply.content_type {
                            response.push_str(&format!("Content-Type: {content_type}\r\n"));
                        }
                        response.push_str("\r\n");
                        response.push_str(body);
                        let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
                        let _ = std::io::Write::flush(&mut stream);
                    }
                }
            }
        });
        format!("http://{address}/song/media/outer/url?id=probe.mp3")
    }

    #[tokio::test]
    async fn test_netease_fallback_valid() {
        let url = spawn_mock(MockReply::Answer(AUDIO), MockReply::Answer(AUDIO));
        verify_outer_url_playable(&url)
            .await
            .expect("200 + audio/mpeg 必须判定为可播放");
    }

    /// 实测形态：不存在的 id 返回 **200 + text/html**（网易 HTML 错误页）。
    /// 这正是"假成功"的来源，必须被判定为不可播放。
    #[tokio::test]
    async fn test_netease_fallback_invalid_id() {
        let url = spawn_mock(
            MockReply::Answer(HTML_ERROR_PAGE),
            MockReply::Answer(HTML_ERROR_PAGE),
        );
        let error = verify_outer_url_playable(&url)
            .await
            .expect_err("HTML 错误页必须判定为不可播放");
        assert!(
            error.to_string().contains("text/html"),
            "失败原因必须保留 content-type，实际: {error}"
        );
    }

    /// 4xx/5xx、以及 200 但内容是 JSON，都应判失败。
    #[tokio::test]
    async fn test_netease_fallback_non_audio_response() {
        let cases = [
            Reply {
                code: 404,
                reason: "Not Found",
                content_type: Some("application/json"),
            },
            Reply {
                code: 500,
                reason: "Internal Server Error",
                content_type: None,
            },
            Reply {
                code: 200,
                reason: "OK",
                content_type: Some("application/json"),
            },
            // 没有 content-type 时保守判失败
            Reply {
                code: 200,
                reason: "OK",
                content_type: None,
            },
        ];
        for reply in cases {
            let url = spawn_mock(MockReply::Answer(reply), MockReply::Answer(reply));
            assert!(
                verify_outer_url_playable(&url).await.is_err(),
                "HTTP {} / content-type {:?} 必须判定为不可播放",
                reply.code,
                reply.content_type
            );
        }
    }

    /// 服务端不响应时必须在探测上限附近快速失败，而不是长时间挂住。
    #[tokio::test]
    async fn test_netease_fallback_timeout() {
        let url = spawn_mock(MockReply::Hang, MockReply::Hang);
        let started = std::time::Instant::now();
        let error = verify_outer_url_playable(&url)
            .await
            .expect_err("不响应必须判定为失败");
        let elapsed = started.elapsed();
        assert!(
            elapsed < OUTER_URL_PROBE_TIMEOUT + std::time::Duration::from_secs(2),
            "必须在探测上限附近返回，实际 {elapsed:?}"
        );
        assert!(
            error.to_string().contains("校验请求失败"),
            "超时要保留原因，实际: {error}"
        );
    }

    /// HEAD 被明确拒绝（405/501）时改用 GET 只看响应头。
    #[tokio::test]
    async fn test_netease_fallback_falls_back_to_get_when_head_unsupported() {
        let url = spawn_mock(
            MockReply::Answer(Reply {
                code: 405,
                reason: "Method Not Allowed",
                content_type: Some("text/plain"),
            }),
            MockReply::Answer(AUDIO),
        );
        verify_outer_url_playable(&url)
            .await
            .expect("HEAD 不支持时应由 GET 的 content-type 判定");

        // 反向：GET 也拿不到音频时仍然失败
        let url = spawn_mock(
            MockReply::Answer(Reply {
                code: 501,
                reason: "Not Implemented",
                content_type: None,
            }),
            MockReply::Answer(HTML_ERROR_PAGE),
        );
        assert!(verify_outer_url_playable(&url).await.is_err());
    }

    // ── 真网 smoke（默认 ignore，CI 不依赖公网）──

    /// 对真实网易接口的冒烟测试：
    /// - 真实存在的歌必须通过校验（不能因为加了校验就把正常歌判死）；
    /// - 不存在的 id 必须被判定为不可播放（本次修复的核心）。
    ///
    /// 运行：`cargo test -p lx-source --lib wy::url -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn smoke_outer_url_verification_against_netease() {
        let valid = "https://music.163.com/song/media/outer/url?id=2652820720.mp3";
        let started = std::time::Instant::now();
        verify_outer_url_playable(valid)
            .await
            .expect("真实存在的网易歌曲必须通过校验");
        println!(
            "[smoke] 有效 id 校验通过，耗时 {:.2}s",
            started.elapsed().as_secs_f32()
        );

        let missing =
            "https://music.163.com/song/media/outer/url?id=voicefox-probe-missing-228908.mp3";
        let error = verify_outer_url_playable(missing)
            .await
            .expect_err("不存在的网易 id 必须判定为不可播放");
        println!("[smoke] 无效 id 判定失败（符合预期）: {error}");
    }
}
