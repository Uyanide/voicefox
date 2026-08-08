use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

use crate::model::source::PlayerState;

/// ReplayGain 应用模式。
///
/// `Album` 会在专辑增益缺失时由 mpv 回退到曲目增益。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReplayGainMode {
    /// 不应用文件中的 ReplayGain 标签。
    #[default]
    Off,
    /// 应用曲目增益。
    Track,
    /// 优先应用专辑增益，缺失时回退到曲目增益。
    Album,
}

/// 输出声道布局的常用预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelMode {
    /// 由音频输出和源文件协商布局（使用 mpv 的安全自动模式）。
    #[default]
    Auto,
    Stereo,
    Mono,
    /// 只输出左声道。
    Left,
    /// 只输出右声道。
    Right,
}

/// A-B 循环的两个时间点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbLoop {
    pub start: Duration,
    pub end: Duration,
}

impl AbLoop {
    /// 创建一个有效的循环区间。终点必须严格晚于起点。
    pub fn new(start: Duration, end: Duration) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }
}

/// 一个均衡器频段（中心频率和增益）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqualizerBand {
    pub frequency_hz: f64,
    pub gain_db: f64,
}

impl EqualizerBand {
    pub const fn new(frequency_hz: f64, gain_db: f64) -> Self {
        Self {
            frequency_hz,
            gain_db,
        }
    }
}

/// 向后兼容的别名，便于调用方使用更明确的名称。
pub type AbLoopPoints = AbLoop;

/// 播放器离散事件
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    Playing { generation: u64 },
    Ended { generation: u64 },
    Error { generation: u64, message: String },
    Buffering { generation: u64, percent: f64 },
}

/// 播放器统一接口
pub trait Player: Send + Sync {
    /// 标记播放器即将加载新媒体，并返回本次播放的代次令牌。
    fn prepare(&self) -> u64;
    /// 仅当代次令牌仍有效时开始播放，返回是否接受了本次请求。
    fn play(&self, url: &str, generation: u64) -> bool;
    fn play_with_headers(&self, url: &str, generation: u64, _headers: &[(String, String)]) -> bool {
        self.play(url, generation)
    }
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn toggle(&self);
    fn seek(&self, position: Duration);

    /// 状态观察者（watch: 取最新值，高频不丢帧）
    fn state_watcher(&self) -> watch::Receiver<PlayerState>;
    /// 进度观察者
    fn position_watcher(&self) -> watch::Receiver<Duration>;
    /// 实际可听到的音频进度。默认与媒体进度一致，支持音频输出延迟的播放器可覆盖。
    fn audible_position_watcher(&self) -> watch::Receiver<Duration> {
        self.position_watcher()
    }
    /// 总时长观察者
    fn duration_watcher(&self) -> watch::Receiver<Duration>;

    /// 离散事件（Ended, Error, Buffering）— 调用后 receiver 被消耗
    fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<PlayerEvent>>;

    /// 音量 0-100
    fn volume(&self) -> u32;
    fn set_volume(&self, vol: u32);
    fn volume_up(&self, delta: u32);
    fn volume_down(&self, delta: u32);

    /// 播放速度倍率，正常速度为 `1.0`。
    fn playback_speed(&self) -> f64 {
        1.0
    }
    fn set_playback_speed(&self, _speed: f64) {}

    /// 音频输出设备名称。名称格式由 mpv 音频输出后端定义，`auto` 表示系统默认设备。
    fn audio_output_device(&self) -> String {
        "auto".to_string()
    }
    fn set_audio_output_device(&self, _device: &str) {}

    /// 便捷别名，保持控制层命名简洁。
    fn speed(&self) -> f64 {
        self.playback_speed()
    }
    fn set_speed(&self, speed: f64) {
        self.set_playback_speed(speed);
    }
    fn audio_device(&self) -> String {
        self.audio_output_device()
    }
    fn set_audio_device(&self, device: &str) {
        self.set_audio_output_device(device);
    }

    /// ReplayGain 模式及预放大增益（分贝）。
    fn replaygain_mode(&self) -> ReplayGainMode {
        ReplayGainMode::Off
    }
    fn set_replaygain_mode(&self, _mode: ReplayGainMode) {}
    fn replaygain_preamp(&self) -> f64 {
        0.0
    }
    fn set_replaygain_preamp(&self, _db: f64) {}
    fn replaygain_clip(&self) -> bool {
        false
    }
    fn set_replaygain_clip(&self, _clip: bool) {}

    /// 输出声道布局预设。
    fn channel_mode(&self) -> ChannelMode {
        ChannelMode::Auto
    }
    fn set_channel_mode(&self, _mode: ChannelMode) {}

    /// 左右声道平衡，范围为 `-1.0`（全左）到 `1.0`（全右）。
    fn balance(&self) -> f64 {
        0.0
    }
    fn set_balance(&self, _balance: f64) {}

    /// 当前 A-B 循环区间；未设置时返回 `None`。
    fn ab_loop(&self) -> Option<AbLoop> {
        None
    }
    fn set_ab_loop(&self, _loop_points: Option<AbLoop>) {}
    fn clear_ab_loop(&self) {
        self.set_ab_loop(None);
    }

    /// 设置均衡器频段。空切片清除由 Voicefox 设置的频段。
    fn equalizer_bands(&self) -> Vec<EqualizerBand> {
        Vec::new()
    }
    fn set_equalizer_bands(&self, _bands: &[EqualizerBand]) {}
    fn equalizer(&self) -> Vec<EqualizerBand> {
        self.equalizer_bands()
    }
    fn set_equalizer(&self, bands: &[EqualizerBand]) {
        self.set_equalizer_bands(bands);
    }
    fn clear_equalizer(&self) {
        self.set_equalizer_bands(&[]);
    }

    /// Asynchronously ramp the output from silence to the configured volume.
    /// Implementations that do not support fades may leave the default no-op.
    fn fade_in(&self, _duration: Duration) {}

    /// Asynchronously ramp the output to silence while retaining the logical
    /// volume setting for the next track.
    fn fade_out(&self, _duration: Duration) {}

    /// Cancel a pending fade operation.
    fn cancel_fade(&self) {}
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{AbLoop, ChannelMode, EqualizerBand, ReplayGainMode};

    #[test]
    fn ab_loop_requires_a_strictly_positive_range() {
        assert!(AbLoop::new(Duration::from_secs(1), Duration::from_secs(2)).is_some());
        assert!(AbLoop::new(Duration::from_secs(2), Duration::from_secs(2)).is_none());
        assert!(AbLoop::new(Duration::from_secs(3), Duration::from_secs(2)).is_none());
    }

    #[test]
    fn playback_control_types_have_stable_defaults() {
        assert_eq!(ReplayGainMode::default(), ReplayGainMode::Off);
        assert_eq!(ChannelMode::default(), ChannelMode::Auto);
        assert_eq!(EqualizerBand::new(100.0, 2.0).frequency_hz, 100.0);
    }
}
