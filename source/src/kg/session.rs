//! 酷狗登录态。
//!
//! 登录后拿到的是 `token` + `userid` 两个 cookie（外加一组设备标识），
//! 统一存进通用 `SessionStore`；请求时把全部 cookie 拼进头部。

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use lx_core::model::source::SourceId;

use crate::session::{SessionStore, SourceSession};

/// 登录凭据 cookie。
const LOGIN_COOKIE: &str = "token";

static STORE: OnceLock<SessionStore> = OnceLock::new();
static PENDING_DEVICE: OnceLock<Mutex<BTreeMap<String, BTreeMap<String, String>>>> =
    OnceLock::new();

fn pending_device() -> &'static Mutex<BTreeMap<String, BTreeMap<String, String>>> {
    PENDING_DEVICE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn save_pending(key: &str, device: &BTreeMap<String, String>) {
    let mut guard = pending_device()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    guard.insert(key.to_string(), device.clone());
}

pub(super) fn pending(key: &str) -> Option<BTreeMap<String, String>> {
    let guard = pending_device()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    guard.get(key).cloned()
}

pub(super) fn clear_pending(key: &str) {
    if let Ok(mut guard) = pending_device().lock() {
        guard.remove(key);
    }
}

pub(super) fn store() -> &'static SessionStore {
    STORE.get_or_init(|| SessionStore::load(SourceId::Kg))
}

pub(super) fn snapshot() -> SourceSession {
    store().snapshot()
}

/// 请求头里的 Cookie：全部键值按名字排序，保证请求稳定可复现。
pub(super) fn cookie_header() -> Option<String> {
    let session = snapshot();
    if session.cookies.is_empty() {
        return None;
    }
    let mut pairs = session
        .cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    pairs.sort();
    Some(pairs.join("; "))
}

pub(super) fn is_logged_in() -> bool {
    let session = snapshot();
    session.has_cookie(LOGIN_COOKIE) && session.user_id.is_some()
}

pub(super) fn user_id() -> Option<String> {
    snapshot().user_id
}

/// 保存扫码结果：设备标识、登录票据与账号 ID 一并写入。
pub(super) fn save_login(
    device: &std::collections::BTreeMap<String, String>,
    token: &str,
    user_id: &str,
) -> Result<(), String> {
    store().update(|session| {
        for (name, value) in device {
            session.set_cookie(name, value);
        }
        session.set_cookie("token", token);
        session.set_cookie("userid", user_id);
        session.user_id = Some(user_id.to_string());
    })
}

pub(super) fn logout() -> Result<(), String> {
    store().clear()
}
