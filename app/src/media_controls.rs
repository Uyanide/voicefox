//! 平台媒体控件的胶水层：快照构建、后端启动、桌面命令执行。
//!
//! 从 main.rs 拆出的第一块：这一组函数只依赖 `crate::execute_action` 与
//! `AppContext`，与 TUI 渲染无关，Linux（MPRIS）与 Windows（SMTC）共用。

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use lx_core::events::AppAction;
use lx_core::model::song::SongInfo;
use lx_core::model::source::PlayerState;

use crate::context::AppContext;
use crate::media_session;
use crate::pages;
use lx_core::events::Notification;

use tokio::sync::mpsc;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn current_media_snapshot(ctx: &AppContext) -> media_session::MediaSnapshot {
    let song = ctx.current_song.read().unwrap_or_else(|e| e.into_inner());
    media_session::MediaSnapshot::new(
        *ctx.player_state.borrow(),
        song.as_ref(),
        *ctx.position.borrow(),
        *ctx.duration.borrow(),
        ctx.player.volume(),
        ctx.playlist.mode(),
        ctx.playlist.len(),
        ctx.position_epoch(),
    )
}

/// 启动平台媒体控件后端（Linux MPRIS / Windows SMTC）。
///
/// 返回 `(None, None)` 表示平台不支持、配置关闭或启动失败。
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn start_media_controls(
    ctx: &AppContext,
    rt: &tokio::runtime::Runtime,
) -> (
    Option<media_session::MediaHandle>,
    Option<tokio::sync::mpsc::UnboundedReceiver<media_session::MediaCommand>>,
) {
    let enabled = {
        let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
        #[cfg(target_os = "linux")]
        {
            config.integration.mpris
        }
        #[cfg(target_os = "windows")]
        {
            config.integration.smtc
        }
    };
    // SMTC 不经过 tokio（souvlaki 直接开线程），参数只为与 MPRIS 路径同构。
    #[cfg(target_os = "windows")]
    let _ = rt;
    if !enabled {
        return (None, None);
    }
    let result = {
        #[cfg(target_os = "linux")]
        {
            rt.block_on(crate::mpris::start())
        }
        #[cfg(target_os = "windows")]
        {
            crate::smtc::start()
        }
    };
    match result {
        Ok((handle, receiver)) => (Some(handle), Some(receiver)),
        Err(error) => {
            tracing::warn!("media controls unavailable: {error:#}");
            let backend = if cfg!(target_os = "linux") {
                "MPRIS"
            } else {
                "SMTC"
            };
            ctx.notify(Notification::warning(format!("{backend} 启动失败: {error:#}")).tui_only());
            (None, None)
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_media_command(
    command: media_session::MediaCommand,
    ctx: &AppContext,
    rt: &tokio::runtime::Runtime,
    action_tx: &mpsc::UnboundedSender<AppAction>,
    search_page: &Arc<std::sync::Mutex<pages::search::SearchPage>>,
    settings_page: &Arc<std::sync::Mutex<pages::settings::SettingsPage>>,
    search_seq: &Arc<AtomicU64>,
) -> bool {
    use media_session::MediaCommand;

    let play_entry = |entry: Option<(Arc<Vec<SongInfo>>, usize)>| {
        if let Some((songs, index)) = entry {
            crate::execute_action(
                AppAction::PlayFromQueue { songs, index },
                ctx,
                rt,
                action_tx,
                search_page,
                settings_page,
                search_seq,
            );
        }
    };

    match command {
        MediaCommand::Quit => return true,
        MediaCommand::Play => {
            resume_or_start_current(ctx, rt, action_tx, search_page, settings_page, search_seq)
        }
        MediaCommand::Pause => ctx.player.pause(),
        MediaCommand::Toggle => {
            toggle_or_start_current(ctx, rt, action_tx, search_page, settings_page, search_seq)
        }
        MediaCommand::Stop => ctx.stop_player(),
        MediaCommand::Next => play_entry(ctx.playlist.next_manual_entry_arc()),
        MediaCommand::Previous => play_entry(ctx.playlist.prev_manual_entry_arc()),
        MediaCommand::SeekBy(offset) => {
            let current = ctx.position.borrow().as_micros();
            let target = if offset >= 0 {
                current.saturating_add(offset as u128)
            } else {
                current.saturating_sub(offset.unsigned_abs() as u128)
            };
            ctx.seek(Duration::from_micros(target.min(u64::MAX as u128) as u64));
        }
        MediaCommand::SetPosition(position) => ctx.seek(position),
        MediaCommand::SetVolume(volume) => {
            persist_volume(ctx, (volume.clamp(0.0, 1.0) * 100.0).round() as u32);
        }
    }
    false
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
#[allow(clippy::too_many_arguments)]
fn resume_or_start_current(
    ctx: &AppContext,
    rt: &tokio::runtime::Runtime,
    action_tx: &mpsc::UnboundedSender<AppAction>,
    search_page: &Arc<std::sync::Mutex<pages::search::SearchPage>>,
    settings_page: &Arc<std::sync::Mutex<pages::settings::SettingsPage>>,
    search_seq: &Arc<AtomicU64>,
) {
    if *ctx.player_state.borrow() == PlayerState::Paused {
        ctx.player.resume();
    } else if matches!(
        *ctx.player_state.borrow(),
        PlayerState::Idle | PlayerState::Stopped
    ) {
        start_current_queue_entry(ctx, rt, action_tx, search_page, settings_page, search_seq);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn toggle_or_start_current(
    ctx: &AppContext,
    rt: &tokio::runtime::Runtime,
    action_tx: &mpsc::UnboundedSender<AppAction>,
    search_page: &Arc<std::sync::Mutex<pages::search::SearchPage>>,
    settings_page: &Arc<std::sync::Mutex<pages::settings::SettingsPage>>,
    search_seq: &Arc<AtomicU64>,
) {
    // Copy the state out of the watch channel first: pause()/resume() send
    // a new state into the same watch channel, and a live watch::Ref holds
    // the RwLock read guard that the send needs as a write lock.
    let state = *ctx.player_state.borrow();
    match state {
        PlayerState::Playing | PlayerState::Loading => ctx.player.pause(),
        PlayerState::Paused => ctx.player.resume(),
        PlayerState::Idle | PlayerState::Stopped => {
            start_current_queue_entry(ctx, rt, action_tx, search_page, settings_page, search_seq);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn start_current_queue_entry(
    ctx: &AppContext,
    rt: &tokio::runtime::Runtime,
    action_tx: &mpsc::UnboundedSender<AppAction>,
    search_page: &Arc<std::sync::Mutex<pages::search::SearchPage>>,
    settings_page: &Arc<std::sync::Mutex<pages::settings::SettingsPage>>,
    search_seq: &Arc<AtomicU64>,
) {
    let (songs, index) = ctx.playlist.snapshot_arc();
    if songs.get(index).is_some() {
        crate::execute_action(
            AppAction::PlayFromQueue { songs, index },
            ctx,
            rt,
            action_tx,
            search_page,
            settings_page,
            search_seq,
        );
    }
}

pub(crate) fn persist_volume(ctx: &AppContext, volume: u32) {
    let volume = volume.clamp(0, 100);
    ctx.player.set_volume(volume);
    {
        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
        if config.player.volume == volume {
            return;
        }
        config.player.volume = volume;
    }
    ctx.mark_config_dirty();
}
