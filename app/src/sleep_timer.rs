//! 睡眠定时器：倒计时到点后淡出并暂停播放。
//!
//! 状态机只有两相（`Armed` 等待到点 → `Fading` 淡出收尾），由主循环的
//! 周期渲染路径驱动 [`SleepTimerState::poll`]，不额外开线程 —— 复用
//! libmpv 引擎自带的音量淡出，暂停与音量恢复都在主循环里收口。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use lx_core::model::source::PlayerState;

use crate::context::AppContext;
use crate::fmt::format_duration;

/// 到点淡出的时长上限。这里只是"入睡"体验的一部分，不沿用曲末淡出的完整
/// 配置 —— 用户可能配了很长的曲末淡出，到点后等太久才暂停反而不像定时器。
const MAX_SLEEP_FADE: Duration = Duration::from_secs(10);
/// 淡出的时长下限：低于这个值几乎等于瞬断，失去"渐弱入睡"的意义。
const MIN_SLEEP_FADE: Duration = Duration::from_millis(400);
/// 淡出结束后再补一点缓冲才暂停，避免淡出线程还没走完就先停了声音。
const FADE_GRACE: Duration = Duration::from_millis(200);

/// 菜单里可选的预设时长（分钟）。
pub const PRESET_MINUTES: [u64; 6] = [15, 30, 45, 60, 90, 120];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SleepPhase {
    /// 已设定，等待到点。`total_minutes` 记录用户选的档位，供菜单打勾。
    Armed {
        ends_at: Instant,
        total_minutes: u64,
    },
    /// 已触发淡出，等淡出走完再暂停。`generation` 记录触发时正在播放的
    /// 歌曲代次：淡出期间用户切了歌就不再补暂停。
    Fading { deadline: Instant, generation: u64 },
}

/// 睡眠定时器的共享状态（挂在 [`AppContext`] 上）。
pub struct SleepTimerState {
    phase: Mutex<Option<SleepPhase>>,
}

impl SleepTimerState {
    pub fn new() -> Self {
        Self {
            phase: Mutex::new(None),
        }
    }

    fn phase(&self) -> Option<SleepPhase> {
        *self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_phase(&self, phase: Option<SleepPhase>) {
        *self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = phase;
    }

    pub fn is_active(&self) -> bool {
        self.phase().is_some()
    }

    /// 当前设定的档位（分钟），未启用时为 `None`。菜单用它标记"当前在哪档"。
    pub fn armed_minutes(&self) -> Option<u64> {
        match self.phase() {
            Some(SleepPhase::Armed { total_minutes, .. }) => Some(total_minutes),
            _ => None,
        }
    }

    /// 状态栏展示文本（含倒计时），未启用时返回 `None`。
    pub fn status_label(&self) -> Option<String> {
        match self.phase() {
            Some(SleepPhase::Armed { ends_at, .. }) => {
                let remaining = ends_at.saturating_duration_since(Instant::now());
                // 向上取整到秒：刚设定的 30 分钟应显示 30:00 而不是 29:59。
                let secs = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
                // 分钟粒度展示剩余时间；不足 1 分钟时切换到秒粒度收尾。
                let text = if secs > 60 {
                    format_duration(Duration::from_secs(secs.div_ceil(60) * 60))
                } else {
                    format_duration(Duration::from_secs(secs))
                };
                Some(format!("💤 {text}"))
            }
            Some(SleepPhase::Fading { .. }) => Some("💤 淡出中".to_string()),
            None => None,
        }
    }

    /// 设定定时器（分钟）。返回给用户的通知文案。
    pub fn arm(&self, minutes: u64) -> String {
        self.set_phase(Some(SleepPhase::Armed {
            ends_at: Instant::now() + Duration::from_secs(minutes.saturating_mul(60)),
            total_minutes: minutes,
        }));
        format!("睡眠定时器: {minutes} 分钟后暂停播放")
    }

    /// 取消定时器。返回给用户的通知文案。
    pub fn cancel(&self) -> String {
        let was_active = self.phase().is_some();
        self.set_phase(None);
        if was_active {
            "睡眠定时器已取消".to_string()
        } else {
            "睡眠定时器未启用".to_string()
        }
    }

    /// 主循环周期调用：推进状态机。
    ///
    /// 返回 `Some(文案)` 表示发生了一次用户可感知的变化，调用方负责弹通知。
    pub fn poll(&self, ctx: &AppContext) -> Option<String> {
        match self.phase()? {
            SleepPhase::Armed { ends_at, .. } => {
                if Instant::now() < ends_at {
                    return None;
                }
                // 到点时本来就没在播放（用户自己暂停了）：静默解除即可。
                if *ctx.player_state.borrow() != PlayerState::Playing {
                    self.set_phase(None);
                    return None;
                }
                let fade = {
                    let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
                    Duration::from_millis(config.player.fade_out_ms)
                        .clamp(MIN_SLEEP_FADE, MAX_SLEEP_FADE)
                };
                let generation = ctx
                    .active_player_generation
                    .load(std::sync::atomic::Ordering::Acquire);
                ctx.player.fade_out(fade);
                self.set_phase(Some(SleepPhase::Fading {
                    deadline: Instant::now() + fade + FADE_GRACE,
                    generation,
                }));
                None
            }
            SleepPhase::Fading {
                deadline,
                generation,
            } => {
                if Instant::now() < deadline {
                    return None;
                }
                self.set_phase(None);
                // 淡出期间换了歌（切歌会取消淡出并恢复音量）或已暂停：不再补刀。
                let current = ctx
                    .active_player_generation
                    .load(std::sync::atomic::Ordering::Acquire);
                if generation != current || *ctx.player_state.borrow() != PlayerState::Playing {
                    return None;
                }
                ctx.player.pause();
                // 暂停后立刻恢复逻辑音量：淡出把 mpv 实际音量停在了 0 附近，
                // 不恢复的话下次手动恢复播放会没有声音。
                ctx.player.cancel_fade();
                Some("💤 睡眠定时器到点，已暂停播放".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // is_active / arm / cancel 的纯状态部分不需要 mpv，直接测。
    #[test]
    fn arm_then_cancel_round_trip() {
        let timer = SleepTimerState::new();
        assert!(!timer.is_active());
        assert_eq!(timer.status_label(), None);

        let message = timer.arm(30);
        assert_eq!(message, "睡眠定时器: 30 分钟后暂停播放");
        assert!(timer.is_active());
        // 倒计时刚设定，剩余时间显示整点档位。
        assert_eq!(timer.status_label().unwrap(), "💤 30:00");
        assert_eq!(timer.armed_minutes(), Some(30));

        assert_eq!(timer.cancel(), "睡眠定时器已取消");
        assert!(!timer.is_active());
        // 再取消一次是幂等的，只是文案不同。
        assert_eq!(timer.cancel(), "睡眠定时器未启用");
    }

    #[test]
    fn arm_overwrite_keeps_latest_preset() {
        let timer = SleepTimerState::new();
        timer.arm(15);
        timer.arm(90);
        assert_eq!(timer.armed_minutes(), Some(90));
    }

    #[test]
    fn status_label_renders_sub_minute_countdown() {
        let timer = SleepTimerState::new();
        timer.set_phase(Some(SleepPhase::Armed {
            ends_at: Instant::now() + Duration::from_secs(45),
            total_minutes: 1,
        }));
        assert_eq!(timer.status_label().unwrap(), "💤 00:45");
    }
}
