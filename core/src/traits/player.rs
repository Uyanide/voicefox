use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::model::source::PlayerState;

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
}
