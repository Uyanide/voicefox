//! 网易云扫码登录。
//!
//! - 生成：POST `/api/login/qrcode/unikey`（表单 `type=3`）拿到 `unikey`；
//! - 轮询：POST `/api/login/qrcode/client/login`（表单 `key` + `type=3`），
//!   状态码 800 过期 / 801 等待 / 802 已扫码 / 803 成功，成功后响应体与
//!   `Set-Cookie` 里都带登录票据，两者都收下来。

use std::collections::BTreeMap;

use lx_core::model::login::{QrLoginResult, QrLoginSession, QrLoginStatus};
use lx_core::model::source::SourceId;
use lx_core::traits::source::FetchError;
use serde_json::Value;

use crate::http;
use crate::http::SendWithRetry;

use super::crypto;
use super::session;

const QR_KEY_API: &str = "https://interface3.music.163.com/eapi/login/qrcode/unikey";
const QR_CHECK_API: &str = "https://interface3.music.163.com/eapi/login/qrcode/client/login";
const QR_KEY_PATH: &str = "/api/login/qrcode/unikey";
const QR_CHECK_PATH: &str = "/api/login/qrcode/client/login";
const EAPI_USER_AGENT: &str = "NeteaseMusic 9.0.90/5038 (iPhone; iOS 16.2; zh_CN)";
const REFERER: &str = "https://music.163.com/";

fn eapi_header() -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    serde_json::json!({
        "osver": "16.2",
        "deviceId": "voicefox-linux",
        "os": "iphone",
        "appver": "9.0.90",
        "versioncode": "140",
        "mobilename": "",
        "buildver": (now / 1000).to_string(),
        "resolution": "1920x1080",
        "__csrf": "",
        "channel": "distribution",
        "requestId": format!("{now}_0000"),
    })
}

fn eapi_body(path: &str, mut data: Value) -> String {
    data["header"] = eapi_header();
    crypto::eapi(path, &data)
}

pub async fn create() -> Result<QrLoginSession, FetchError> {
    let body = eapi_body(QR_KEY_PATH, serde_json::json!({ "type": 3 }));
    let mut request = http::client()
        .post(QR_KEY_API)
        .header("User-Agent", EAPI_USER_AGENT)
        .header("Accept", "*/*")
        .header("Accept-Language", "zh-CN,zh;q=0.8")
        .header("Origin", "https://music.163.com")
        .header("Referer", REFERER)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("params={body}"));
    if let Some(cookie) = session::cookie_header() {
        request = request.header("Cookie", cookie);
    }
    let response = request
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let headers = response.headers().clone();
    let json: Value = response
        .json()
        .await
        .map_err(|error| FetchError::Parse(error.to_string()))?;
    if json["code"].as_i64() != Some(200) {
        let message = json["message"].as_str().unwrap_or("网易云二维码生成失败");
        return Err(FetchError::Other(message.to_string()));
    }
    save_set_cookies(&headers);
    let key = json["unikey"]
        .as_str()
        .or_else(|| json["data"]["unikey"].as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| FetchError::Parse("网易云二维码 key 为空".to_string()))?;
    Ok(QrLoginSession {
        source: SourceId::Wy,
        key: key.to_string(),
        url: format!("https://music.163.com/login?codekey={key}"),
        image_png: None,
        expires_in: 300,
    })
}

pub async fn check(key: &str) -> Result<QrLoginResult, FetchError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(FetchError::Other("网易云二维码 key 为空".to_string()));
    }
    let body = eapi_body(QR_CHECK_PATH, serde_json::json!({ "key": key, "type": 3 }));
    let mut request = http::client()
        .post(QR_CHECK_API)
        .header("User-Agent", EAPI_USER_AGENT)
        .header("Accept", "*/*")
        .header("Accept-Language", "zh-CN,zh;q=0.8")
        .header("Origin", "https://music.163.com")
        .header("Referer", REFERER)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!("params={body}"));
    if let Some(cookie) = session::cookie_header() {
        request = request.header("Cookie", cookie);
    }
    let response = request
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let headers = response.headers().clone();
    let json: Value = response
        .json()
        .await
        .map_err(|error| FetchError::Parse(error.to_string()))?;
    let code = json["code"].as_i64().unwrap_or_default();
    let message = json["message"].as_str().unwrap_or_default();
    let status =
        if code != 800 && code != 801 && code != 802 && code != 803 && message.contains("验证") {
            // 网易云风控偶尔会在轮询接口返回“请完成验证操作”。这不是二维码
            // 已失效，不能把二维码页面直接切成 Error；保留二维码继续等待。
            QrLoginStatus::RiskControl
        } else {
            status_from_code(code)
        };

    // 成功时收集 Set-Cookie 与响应体里的 cookie 串，两者合并后写入存储。
    let mut cookies = BTreeMap::new();
    if status == QrLoginStatus::Success {
        for value in headers.get_all(reqwest::header::SET_COOKIE) {
            if let Ok(value) = value.to_str()
                && let Some(pair) = value.split(';').next()
                && let Some((name, value)) = pair.split_once('=')
            {
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
        if let Some(raw) = json["cookie"].as_str() {
            for pair in raw.split(';') {
                if let Some((name, value)) = pair.split_once('=') {
                    cookies.insert(name.trim().to_string(), value.trim().to_string());
                }
            }
        }
        if !cookies.is_empty() {
            session::save_cookies(&cookies).map_err(FetchError::Other)?;
        }
    }

    let mut result = QrLoginResult::new(status, message_for(status, &json));
    result.cookies = cookies;
    Ok(result)
}

fn save_set_cookies(headers: &reqwest::header::HeaderMap) {
    let mut cookies = BTreeMap::new();
    for value in headers.get_all(reqwest::header::SET_COOKIE) {
        if let Ok(value) = value.to_str()
            && let Some(pair) = value.split(';').next()
            && let Some((name, value)) = pair.split_once('=')
        {
            cookies.insert(name.trim().to_string(), value.trim().to_string());
        }
    }
    if !cookies.is_empty() {
        if let Err(error) = session::save_cookies(&cookies) {
            tracing::debug!("保存网易云匿名登录 cookie 失败: {error}");
        }
    }
}

fn status_from_code(code: i64) -> QrLoginStatus {
    match code {
        800 => QrLoginStatus::Expired,
        801 => QrLoginStatus::Waiting,
        802 => QrLoginStatus::Scanned,
        803 => QrLoginStatus::Success,
        _ => QrLoginStatus::Failed,
    }
}

fn message_for(status: QrLoginStatus, json: &Value) -> String {
    let message = json["message"].as_str().unwrap_or_default().trim();
    if !message.is_empty() {
        return message.to_string();
    }
    match status {
        QrLoginStatus::Waiting => "等待扫码",
        QrLoginStatus::Scanned => "已扫码，请在手机上确认",
        QrLoginStatus::Success => "登录成功",
        QrLoginStatus::Expired => "二维码已过期",
        QrLoginStatus::Failed => "登录失败",
        QrLoginStatus::NetworkError => "网络错误，正在重试",
        QrLoginStatus::RiskControl => "触发验证/风控，正在重试",
        QrLoginStatus::ServerError => "服务端暂时异常，正在重试",
        QrLoginStatus::InvalidSession => "登录会话已失效",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_netease_qr_codes() {
        assert_eq!(status_from_code(800), QrLoginStatus::Expired);
        assert_eq!(status_from_code(801), QrLoginStatus::Waiting);
        assert_eq!(status_from_code(802), QrLoginStatus::Scanned);
        assert_eq!(status_from_code(803), QrLoginStatus::Success);
        assert_eq!(status_from_code(404), QrLoginStatus::Failed);
    }

    #[test]
    fn prefers_the_api_message() {
        let json = serde_json::json!({ "message": "二维码已失效" });
        assert_eq!(message_for(QrLoginStatus::Expired, &json), "二维码已失效");
        let empty = serde_json::json!({});
        assert_eq!(message_for(QrLoginStatus::Waiting, &empty), "等待扫码");
    }
}
