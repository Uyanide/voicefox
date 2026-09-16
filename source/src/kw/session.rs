//! 酷我 www 接口的会话与签名。
//!
//! 对齐 MusicBot-Go `plugins/kuwo/session.go`：每个 www API 请求都要带
//! `Secret: build_secret(cookie, nonce)` 头与 `reqId` 查询参数，还要带一个
//! `Hm_Iuvt_...` cookie，否则服务端返回
//! `{"success":false,"message":"The request is illegal!"}`。
//!
//! 实测（2026-09）这个 cookie 不需要是首页下发的真实值：服务端只校验
//! 「Secret 解出来的值 == 请求里带的 cookie」，本地随机生成的 32 位 token
//! 一样能通过（`playUrl` / `musicInfo` 都返回 200）。因此这里不再访问首页，
//! 进程内生成并缓存一个 token 就够了，也避开了站点 WAF cookie 的干扰。
//!
//! 酷我本身不需要登录：签名只是防抓取。

use std::sync::{Mutex, OnceLock};

use lx_core::traits::source::FetchError;

/// 站点约定的会话 cookie 名。
pub(super) const SESSION_COOKIE: &str = "Hm_Iuvt_cdb524f42f23cer9b268564v7y735ewrq2324";
pub(super) const REFERER: &str = "https://www.kuwo.cn/";
/// 音频直链要求这个 UA（`okhttp` 客户端标识），否则部分 CDN 会 403。
pub(super) const MEDIA_UA: &str = "okhttp/3.10.0";
pub(super) const WEB_UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";

/// 签名用的线性同余参数（与参考实现一致）。
const SECRET_SEED: i64 = 59_910_100;
const SECRET_MULTIPLIER: i64 = 9253;
const SECRET_INCREMENT: i64 = 23;
const SECRET_MODULUS: i64 = 2_147_483_647;
/// 站点下发的 token 长度。
const TOKEN_LEN: usize = 32;

fn token_store() -> &'static Mutex<Option<String>> {
    static TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    TOKEN.get_or_init(|| Mutex::new(None))
}

fn random_token() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..TOKEN_LEN)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// 取（必要时生成）会话 token。
pub(super) fn session_token() -> String {
    let mut guard = token_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    guard.get_or_insert_with(random_token).clone()
}

/// 会话被判非法时丢弃当前 token，下次重新生成。
pub(super) fn invalidate() {
    let mut guard = token_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *guard = None;
}

/// `Secret` 头：逐字节用线性同余序列混淆 cookie，再拼上 8 位 nonce。
pub(super) fn build_secret(cookie: &str, nonce: u32) -> String {
    let mut state = SECRET_SEED;
    let mut encoded = Vec::with_capacity(cookie.len());
    for byte in cookie.as_bytes() {
        state = (state * SECRET_MULTIPLIER + SECRET_INCREMENT) % SECRET_MODULUS;
        let mask = (state * 255 / SECRET_MODULUS) as u8;
        encoded.push(byte ^ mask);
    }
    format!("{}{:08x}", hex::encode(encoded), nonce)
}

/// 8 位随机 nonce。
pub(super) fn random_nonce() -> u32 {
    use rand::Rng;
    rand::thread_rng().gen_range(10_000_000..100_000_000)
}

/// 请求标识，酷我要求每个请求带不同的 `reqId`（UUIDv4 形态）。
pub(super) fn request_id() -> String {
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let value = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0))
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(chunk.as_ptr() as u64);
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    // 版本 4 + variant 10xx。
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let as_hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &as_hex[0..8],
        &as_hex[8..12],
        &as_hex[12..16],
        &as_hex[16..20],
        &as_hex[20..32]
    )
}

/// 给 www API 的 URL 补上 `reqId`。
pub(super) fn with_request_id(url: &str, request_id: &str) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}reqId={request_id}")
}

/// 给 www API 的请求带上签名所需的标准头。
pub(super) fn sign(
    request: reqwest::RequestBuilder,
    cookie: &str,
    nonce: u32,
) -> reqwest::RequestBuilder {
    request
        .header("User-Agent", WEB_UA)
        .header("Referer", REFERER)
        .header("Secret", build_secret(cookie, nonce))
        .header("Cookie", format!("{SESSION_COOKIE}={cookie}"))
}

/// 会话非法（风控/过期）的响应特征。
pub(super) fn is_illegal_session(body: &str) -> bool {
    body.contains("request is illegal") || body.contains("The request is illegal")
}

/// 具名错误：会话不可用时给出可读原因（当前不会发生，保留给未来的真实会话模式）。
#[allow(dead_code)]
pub(super) fn session_unavailable() -> FetchError {
    FetchError::Other("酷我会话不可用".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_is_stable_and_length_prefixed() {
        // 同一 cookie + nonce 必须得到同样的签名（对齐参考实现）。
        let secret = build_secret("abc123", 12345678);
        assert_eq!(secret, build_secret("abc123", 12345678));
        assert_ne!(secret, build_secret("abc123", 12345679));
        // 长度 = cookie 字节数 * 2（hex） + 8 位 nonce。
        assert_eq!(secret.len(), 6 * 2 + 8);
        assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn secret_matches_reference_algorithm() {
        // 期望值由参考实现（MusicBot-Go 的 buildSecret）独立算出，
        // 用来锁住线性同余参数与拼接格式。
        assert_eq!(build_secret("abc123", 12345678), "4232e893231400bc614e");
        assert_eq!(build_secret("Hm_X", 1), "6b3dd4fa00000001");
    }

    #[test]
    fn request_id_looks_like_uuid_v4() {
        let id = request_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(
            parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_ne!(id, request_id());
    }

    #[test]
    fn request_id_is_appended_to_query() {
        assert_eq!(
            with_request_id("https://www.kuwo.cn/api?mid=1", "abc"),
            "https://www.kuwo.cn/api?mid=1&reqId=abc"
        );
        assert_eq!(
            with_request_id("https://www.kuwo.cn/api", "abc"),
            "https://www.kuwo.cn/api?reqId=abc"
        );
    }

    #[test]
    fn session_token_is_cached_and_regenerated_on_invalidate() {
        let first = session_token();
        assert_eq!(first.len(), TOKEN_LEN);
        assert_eq!(session_token(), first);
        invalidate();
        let second = session_token();
        assert_eq!(second.len(), TOKEN_LEN);
        assert_ne!(second, first);
    }
}
