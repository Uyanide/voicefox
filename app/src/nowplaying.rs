//! macOS Now Playing（MPRemoteCommandCenter / MPNowPlayingInfoCenter）服务。
//!
//! 通过 souvlaki 接入：触摸栏、耳机媒体键、控制中心与 Now Playing 浮层都能
//! 控制 voicefox。快照 / 命令的数据结构在 [`crate::media_session`]，与
//! Linux MPRIS / Windows SMTC 共用；这里只做协议翻译 —— 在专属线程上构造
//! `MediaControls` 并循环应用最新快照。
//!
//! 注意：souvlaki 的 macOS 后端要求进程存在 AppKit 运行环境。终端直跑
//! （非 .app bundle）时控件通常仍可用，但系统可能不显示应用名。

use std::time::Duration;

use souvlaki::{MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::warn;

use crate::media_session::{MediaCommand, MediaSnapshot};

/// 快照轮询间隔：与 SMTC 同口径，决定媒体键事件的响应延迟与空转开销的平衡。
const PUMP_INTERVAL: Duration = Duration::from_millis(50);

pub fn start() -> anyhow::Result<(
    crate::media_session::MediaHandle,
    UnboundedReceiver<MediaCommand>,
)> {
    let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (handle, update_rx) = crate::media_session::channel();

    // 控件与创建它的线程绑定，线程退出即控件失效，因此存活到进程结束；
    // 失败只记日志由用户在通知里看到。
    std::thread::Builder::new()
        .name("voicefox-nowplaying".to_string())
        .spawn(move || {
            if let Err(error) = run_worker(command_tx, update_rx) {
                warn!("Now Playing worker exited: {error:#}");
            }
        })?;

    Ok((handle, command_rx))
}

fn run_worker(
    command_tx: tokio::sync::mpsc::UnboundedSender<MediaCommand>,
    mut update_rx: UnboundedReceiver<MediaSnapshot>,
) -> anyhow::Result<()> {
    let mut controls = MediaControls::new(PlatformConfig {
        dbus_name: "voicefox",
        display_name: "voicefox",
        hwnd: None,
    })
    .map_err(|error| anyhow::anyhow!("create media controls: {error}"))?;
    controls
        .attach(move |event| {
            if let Some(command) = crate::media_session::translate_event(event) {
                let _ = command_tx.send(command);
            }
        })
        .map_err(|error| anyhow::anyhow!("attach media controls: {error}"))?;

    // 只保留最新快照：消费不及时的时候，中间状态没有回放价值。
    let mut applied: Option<MediaSnapshot> = None;
    loop {
        let mut latest = None;
        while let Ok(snapshot) = update_rx.try_recv() {
            latest = Some(snapshot);
        }
        if let Some(snapshot) = latest {
            let metadata_changed = applied
                .as_ref()
                .is_none_or(|previous| previous.metadata_changed(&snapshot));
            if metadata_changed {
                apply_metadata(&mut controls, &snapshot);
            }
            if applied.as_ref() != Some(&snapshot) {
                apply_playback(&mut controls, &snapshot);
            }
            applied = Some(snapshot);
        }
        std::thread::sleep(PUMP_INTERVAL);
    }
}

fn apply_metadata(controls: &mut MediaControls, snapshot: &MediaSnapshot) {
    let duration = Duration::from_micros(snapshot.duration_micros.max(0) as u64);
    let metadata = MediaMetadata {
        title: Some(snapshot.title.as_str()),
        artist: Some(snapshot.artist.as_str()),
        album: Some(snapshot.album.as_str()),
        cover_url: Some(snapshot.art_url.as_str()).filter(|url| !url.is_empty()),
        duration: (snapshot.duration_micros > 0).then_some(duration),
    };
    if controls.set_metadata(metadata).is_err() {
        warn!("Now Playing set_metadata failed");
    }
}

fn apply_playback(controls: &mut MediaControls, snapshot: &MediaSnapshot) {
    let playback = match snapshot.playback_status {
        "Playing" => MediaPlayback::Playing {
            progress: Some(MediaPosition(position(snapshot))),
        },
        "Paused" => MediaPlayback::Paused {
            progress: Some(MediaPosition(position(snapshot))),
        },
        _ => MediaPlayback::Stopped,
    };
    if controls.set_playback(playback).is_err() {
        warn!("Now Playing set_playback failed");
    }
}

fn position(snapshot: &MediaSnapshot) -> Duration {
    Duration::from_micros(snapshot.position_micros.max(0) as u64)
}
