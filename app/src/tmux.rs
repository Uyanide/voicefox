//! tmux 集成：把 client attach 通知到进程里

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// 判据对齐 ratatui-image 的 detect_tmux_and_outer_protocol_from_env
pub fn is_passthrough() -> bool {
    std::env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        || std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "tmux")
}

pub struct AttachWatcher {
    session: String,
    hook: String,
    channel: String,
    attached: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

impl AttachWatcher {
    /// 装 hook 并起监听线程。失败返回 None
    pub fn install() -> Option<Self> {
        if !is_passthrough() {
            return None;
        }
        let session = session_target()?;
        let pid = std::process::id();
        let hook = hook_name(pid);
        let channel = channel_name(pid);
        let command = format!("run-shell -b \"tmux wait-for -S {channel}\"");
        if !tmux(&["set-hook", "-t", &session, &hook, &command]) {
            tracing::warn!("install tmux {hook} failed, cover will not follow attach");
            return None;
        }

        let attached = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let (flag, stop, watched) = (
            Arc::clone(&attached),
            Arc::clone(&shutdown),
            channel.clone(),
        );
        let spawned = thread::Builder::new()
            .name("voicefox-tmux-attach".to_string())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    // 阻塞，非忙等
                    if !tmux(&["wait-for", &watched]) {
                        tracing::debug!("tmux wait-for failed, stop watching for attach");
                        break;
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    flag.store(true, Ordering::Relaxed);
                }
            });
        if spawned.is_err() {
            tracing::warn!("spawn tmux attach watcher failed");
            tmux(&["set-hook", "-u", "-t", &session, &hook]);
            return None;
        }

        tracing::info!("watching tmux {hook} on session {session}");
        Some(Self {
            session,
            hook,
            channel,
            attached,
            shutdown,
        })
    }

    /// 上次问过之后有没有 client 接上来
    pub fn take_attached(&self) -> bool {
        self.attached.swap(false, Ordering::Relaxed)
    }
}

impl Drop for AttachWatcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        tmux(&["set-hook", "-u", "-t", &self.session, &self.hook]);
        // 监听线程还阻塞在 wait-for 上，让它看见 shutdown 标志
        tmux(&["wait-for", "-S", &self.channel]);
    }
}

/// hook 数组下标取进程 pid
fn hook_name(pid: u32) -> String {
    format!("client-attached[{pid}]")
}

fn channel_name(pid: u32) -> String {
    format!("voicefox-attached-{pid}")
}

/// $TMUX 的第三段是 session id 编号，拼上 $ 就是 set-hook 的目标，省一次 fork
fn session_target() -> Option<String> {
    session_target_from(&std::env::var("TMUX").ok()?)
}

fn session_target_from(tmux_env: &str) -> Option<String> {
    let id = tmux_env.split(',').nth(2)?;
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(format!("${id}"))
}

/// 跑一条 tmux 命令，返回是否成功
fn tmux(args: &[&str]) -> bool {
    Command::new("tmux")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::session_target_from;

    #[test]
    fn the_session_target_comes_from_the_third_field_of_tmux_env() {
        assert_eq!(
            session_target_from("/tmp/tmux-1000/default,22518,0"),
            Some("$0".to_string())
        );
        assert_eq!(
            session_target_from("/tmp/tmux-1000/default,22518,17"),
            Some("$17".to_string())
        );
    }

    #[test]
    fn a_malformed_tmux_env_yields_no_target() {
        // 少字段、空字段、非数字都不能拼出目标，否则 set-hook 会打到别处
        assert_eq!(session_target_from("/tmp/socket,22518"), None);
        assert_eq!(session_target_from("/tmp/socket,22518,"), None);
        assert_eq!(session_target_from("/tmp/socket,22518,zero"), None);
        assert_eq!(session_target_from(""), None);
    }
}
