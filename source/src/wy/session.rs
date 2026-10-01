//! 网易云登录态。
//!
//! 会话统一存在通用 `SessionStore` 里（`~/.config/voicefox/session/wy.json`），
//! 这里只提供进程内单例与「要不要带 cookie」的判断，避免每个请求点都去
//! 读一次文件。

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use lx_core::model::source::SourceId;

use crate::session::{SessionStore, SourceSession};

/// 登录凭据 cookie：网易云靠 `MUSIC_U` 判定登录。
const LOGIN_COOKIE: &str = "MUSIC_U";

/// 会话失效标记存在 `extra` 里，随会话文件持久化：过期 cookie 重启后依然是过期的，
/// 设置页与同步预检都能直接看到，而不必等下一次接口失败。
const EXPIRED_FLAG: &str = "login_expired";

static STORE: OnceLock<SessionStore> = OnceLock::new();
/// 「已失效」是否已递交给 UI（避免每次接口失败都重复提醒）。
static EXPIRED_NOTICED: AtomicBool = AtomicBool::new(false);

/// 登录健康状态：`is_logged_in` 只看本地有没有凭据，无法区分「过期」，
/// 界面展示与会话判定应使用这里的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginHealth {
    /// 有 `MUSIC_U` 且未被接口判定失效。
    LoggedIn,
    /// 有 `MUSIC_U`，但接口已返回「需要登录」——凭据过期，需要重新扫码。
    Expired,
    /// 没有 `MUSIC_U`，从未登录。
    NotLoggedIn,
}

pub(super) fn store() -> &'static SessionStore {
    STORE.get_or_init(|| SessionStore::load(SourceId::Wy))
}

pub(super) fn snapshot() -> SourceSession {
    store().snapshot()
}

/// 请求头里的 Cookie；未登录或没有 cookie 时返回 `None`。
pub(super) fn cookie_header() -> Option<String> {
    let session = snapshot();
    session
        .cookie_header_of(&[
            LOGIN_COOKIE,
            "MUSIC_A",
            "__csrf",
            "NMTID",
            "JSESSIONID-WYYY",
        ])
        .or_else(|| {
            (!session.cookies.is_empty()).then(|| {
                session
                    .cookies
                    .iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
        })
}

/// 本地是否有登录凭据（不校验服务端有效性；有效性看 [`login_health`]）。
pub fn is_logged_in() -> bool {
    snapshot().has_cookie(LOGIN_COOKIE)
}

/// 界面展示与会话判定用的登录健康状态。
pub fn login_health() -> LoginHealth {
    let session = snapshot();
    if !session.has_cookie(LOGIN_COOKIE) {
        return LoginHealth::NotLoggedIn;
    }
    if session.extra(EXPIRED_FLAG).is_some() {
        return LoginHealth::Expired;
    }
    LoginHealth::LoggedIn
}

/// 账号显示名（登录成功或会话验证通过时保存的昵称）。
pub fn account_display() -> Option<String> {
    snapshot()
        .user_name
        .filter(|name| !name.trim().is_empty())
}

/// 接口返回「需要登录」时记录会话失效。
///
/// 未登录（无 `MUSIC_U`）时匿名请求也会被拒，这不叫「失效」，不记标记。
/// 返回值表示是否刚刚从「有效」翻转为「失效」——同一失效周期里重复的
/// 301/512 不再重复提醒；重新登录或会话恢复后标记被清除，下次失效才会
/// 再次提醒。
pub fn mark_login_expired() -> bool {
    let session = snapshot();
    if !session.has_cookie(LOGIN_COOKIE) {
        return false;
    }
    let already_expired = session.extra(EXPIRED_FLAG).is_some();
    let _ = store().update(|session| session.set_extra(EXPIRED_FLAG, "1"));
    if already_expired {
        return false;
    }
    !EXPIRED_NOTICED.swap(true, Ordering::SeqCst)
}

/// UI 侧消费「登录已失效」提醒：失效后第一次调用返回 `true`，之后返回 `false`，
/// 直到重新判定失效或恢复登录。
pub fn take_expired_notice() -> bool {
    EXPIRED_NOTICED.swap(false, Ordering::SeqCst)
}

/// 会话验证通过（重新登录或账号接口恢复正常）后清除失效标记。
fn clear_expired() {
    let _ = store().update(|session| session.set_extra(EXPIRED_FLAG, ""));
    EXPIRED_NOTICED.store(false, Ordering::SeqCst);
}

/// 保存账号显示信息（昵称 / uid）。只在会话验证通过的路径上调用，
/// 所以顺手清除失效标记。
pub(super) fn save_account(user_name: Option<String>, user_id: Option<String>) {
    let _ = store().update(|session| {
        if let Some(name) = user_name.filter(|name| !name.trim().is_empty()) {
            session.user_name = Some(name);
        }
        if let Some(id) = user_id.filter(|id| !id.trim().is_empty()) {
            session.user_id = Some(id);
        }
    });
    clear_expired();
}

/// 扫码成功后写入 cookie。
pub(super) fn save_cookies(
    cookies: &std::collections::BTreeMap<String, String>,
) -> Result<(), String> {
    store().update(|session| {
        for (name, value) in cookies {
            session.set_cookie(name, value);
        }
    })
}

pub(super) fn logout() -> Result<(), String> {
    EXPIRED_NOTICED.store(false, Ordering::SeqCst);
    store().clear()
}
