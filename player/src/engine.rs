use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::Context;
use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv, mpv_end_file_reason};
use lx_core::model::source::PlayerState;
use lx_core::traits::player::{Player, PlayerEvent};
use tokio::sync::{mpsc, watch};
use tracing::warn;

const TIME_POS_OBSERVER: u64 = 1;
const AUDIO_PTS_OBSERVER: u64 = 2;
const DURATION_OBSERVER: u64 = 3;
const PAUSE_OBSERVER: u64 = 4;
const BUFFERING_OBSERVER: u64 = 5;

struct EventLoopContext {
    state_tx: watch::Sender<PlayerState>,
    position_tx: watch::Sender<Duration>,
    audible_position_tx: watch::Sender<Duration>,
    duration_tx: watch::Sender<Duration>,
    event_tx: mpsc::UnboundedSender<PlayerEvent>,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    pending_seek: Arc<Mutex<Option<(u64, Duration)>>>,
    shutdown: Arc<AtomicBool>,
}

/// 基于 libmpv 的进程内播放引擎。
pub struct MpvEngine {
    mpv: Arc<Mpv>,
    play_lock: Mutex<()>,
    state_tx: watch::Sender<PlayerState>,
    state_rx: watch::Receiver<PlayerState>,
    position_tx: watch::Sender<Duration>,
    position_rx: watch::Receiver<Duration>,
    audible_position_tx: watch::Sender<Duration>,
    audible_position_rx: watch::Receiver<Duration>,
    duration_tx: watch::Sender<Duration>,
    duration_rx: watch::Receiver<Duration>,
    event_tx: mpsc::UnboundedSender<PlayerEvent>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<PlayerEvent>>>,
    volume: AtomicU32,
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    pending_seek: Arc<Mutex<Option<(u64, Duration)>>>,
    shutdown: Arc<AtomicBool>,
    event_thread: Mutex<Option<JoinHandle<()>>>,
}

impl MpvEngine {
    pub fn new() -> anyhow::Result<Self> {
        let mpv = Arc::new(
            Mpv::with_initializer(|init| {
                init.set_option("vo", "null")?;
                init.set_option("cache", "yes")?;
                init.set_option("audio-client-name", "voicefox")?;
                Ok(())
            })
            .context("初始化 libmpv 失败")?,
        );
        mpv.set_property("volume", 80.0_f64)
            .context("设置 libmpv 初始音量失败")?;

        mpv.disable_deprecated_events()
            .context("配置 libmpv 事件失败")?;
        mpv.observe_property("time-pos", Format::Double, TIME_POS_OBSERVER)
            .context("监听 libmpv 播放进度失败")?;
        mpv.observe_property("audio-pts", Format::Double, AUDIO_PTS_OBSERVER)
            .context("监听 libmpv 音频进度失败")?;
        mpv.observe_property("duration", Format::Double, DURATION_OBSERVER)
            .context("监听 libmpv 音频时长失败")?;
        mpv.observe_property("pause", Format::Flag, PAUSE_OBSERVER)
            .context("监听 libmpv 暂停状态失败")?;
        mpv.observe_property("cache-buffering-state", Format::Double, BUFFERING_OBSERVER)
            .context("监听 libmpv 缓冲状态失败")?;

        let (state_tx, state_rx) = watch::channel(PlayerState::Idle);
        let (position_tx, position_rx) = watch::channel(Duration::ZERO);
        let (audible_position_tx, audible_position_rx) = watch::channel(Duration::ZERO);
        let (duration_tx, duration_rx) = watch::channel(Duration::ZERO);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let generation = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let pending_seek = Arc::new(Mutex::new(None));
        let shutdown = Arc::new(AtomicBool::new(false));

        let event_context = EventLoopContext {
            state_tx: state_tx.clone(),
            position_tx: position_tx.clone(),
            audible_position_tx: audible_position_tx.clone(),
            duration_tx: duration_tx.clone(),
            event_tx: event_tx.clone(),
            generation: Arc::clone(&generation),
            paused: Arc::clone(&paused),
            pending_seek: Arc::clone(&pending_seek),
            shutdown: Arc::clone(&shutdown),
        };
        let event_mpv = Arc::clone(&mpv);
        let event_thread = thread::Builder::new()
            .name("voicefox-libmpv-events".to_string())
            .spawn(move || run_event_loop(event_mpv, event_context))
            .context("启动 libmpv 事件线程失败")?;

        Ok(Self {
            mpv,
            play_lock: Mutex::new(()),
            state_tx,
            state_rx,
            position_tx,
            position_rx,
            audible_position_tx,
            audible_position_rx,
            duration_tx,
            duration_rx,
            event_tx,
            event_rx: Mutex::new(Some(event_rx)),
            volume: AtomicU32::new(80),
            generation,
            paused,
            pending_seek,
            shutdown,
            event_thread: Mutex::new(Some(event_thread)),
        })
    }

    fn play_inner(&self, url: &str, generation: u64, headers: &[(String, String)]) -> bool {
        let _play_guard = self.play_lock.lock().unwrap();
        if self.generation.load(Ordering::SeqCst) != generation {
            return false;
        }

        let header_fields = format_http_header_fields(headers);
        let result = self
            .mpv
            .set_property("http-header-fields", header_fields)
            .and_then(|()| self.mpv.set_property("pause", false))
            .and_then(|()| self.mpv.command("loadfile", &[url, "replace"]));

        if let Err(error) = result {
            if self.generation.load(Ordering::SeqCst) == generation {
                let _ = self.state_tx.send(PlayerState::Stopped);
                let _ = self.event_tx.send(PlayerEvent::Error {
                    generation,
                    message: error.to_string(),
                });
            }
            return false;
        }

        true
    }
}

fn run_event_loop(event_client: Arc<Mpv>, context: EventLoopContext) {
    let mut current_file_generation = None;
    let mut last_media_position = None;
    let mut has_audio_position = false;

    while !context.shutdown.load(Ordering::SeqCst) {
        let Some(event) = event_client.wait_event(0.1) else {
            continue;
        };

        match event {
            Ok(Event::StartFile) => {
                let generation = context.generation.load(Ordering::SeqCst);
                current_file_generation = Some(generation);
                last_media_position = None;
                has_audio_position = false;
                if generation != 0 {
                    let _ = context.state_tx.send(loading_state(&context.paused));
                }
            }
            Ok(Event::FileLoaded) => {
                if let Some(generation) =
                    current_generation(current_file_generation, &context.generation)
                {
                    apply_pending_seek(&event_client, generation, &context.pending_seek);
                    let _ = context.state_tx.send(loading_state(&context.paused));
                }
            }
            Ok(Event::PlaybackRestart) => {
                if let Some(generation) =
                    current_generation(current_file_generation, &context.generation)
                {
                    apply_pending_seek(&event_client, generation, &context.pending_seek);
                    let state = if context.paused.load(Ordering::SeqCst) {
                        PlayerState::Paused
                    } else {
                        PlayerState::Playing
                    };
                    let _ = context.state_tx.send(state);
                    let _ = context.event_tx.send(PlayerEvent::Playing { generation });
                }
            }
            Ok(Event::EndFile(reason)) => {
                let Some(generation) =
                    current_generation(current_file_generation, &context.generation)
                else {
                    continue;
                };

                match reason {
                    mpv_end_file_reason::Eof => {
                        let _ = context.state_tx.send(PlayerState::Stopped);
                        let _ = context.event_tx.send(PlayerEvent::Ended { generation });
                    }
                    mpv_end_file_reason::Error => {
                        let _ = context.state_tx.send(PlayerState::Stopped);
                        let _ = context.event_tx.send(PlayerEvent::Error {
                            generation,
                            message: "libmpv 无法播放当前音频".to_string(),
                        });
                    }
                    _ => {}
                }
                current_file_generation = None;
            }
            Ok(Event::PropertyChange { name, change, .. }) => {
                if !event_is_current(current_file_generation, &context.generation) {
                    continue;
                }
                let generation = current_file_generation.unwrap_or_default();

                match (name, change) {
                    ("time-pos", PropertyData::Double(seconds)) => {
                        if let Some(position) = duration_from_mpv_seconds(seconds) {
                            last_media_position = Some(position);
                            let _ = context.position_tx.send(position);
                            if !has_audio_position {
                                let _ = context.audible_position_tx.send(position);
                            }
                        }
                    }
                    ("audio-pts", PropertyData::Double(seconds)) => {
                        if let Some(position) = duration_from_mpv_seconds(seconds) {
                            has_audio_position = true;
                            let _ = context.audible_position_tx.send(position);
                        } else if let Some(position) = last_media_position {
                            has_audio_position = false;
                            let _ = context.audible_position_tx.send(position);
                        }
                    }
                    ("duration", PropertyData::Double(seconds)) => {
                        if let Some(duration) = duration_from_mpv_seconds(seconds) {
                            let _ = context.duration_tx.send(duration);
                        }
                    }
                    ("pause", PropertyData::Flag(paused)) => {
                        context.paused.store(paused, Ordering::SeqCst);
                        if paused {
                            let _ = context.state_tx.send(PlayerState::Paused);
                        }
                    }
                    ("cache-buffering-state", PropertyData::Double(percent)) => {
                        if !percent.is_finite() {
                            continue;
                        }
                        let percent = (percent.clamp(0.0, 100.0) / 100.0).clamp(0.0, 1.0);
                        if percent < 1.0 && !context.paused.load(Ordering::SeqCst) {
                            let _ = context.state_tx.send(PlayerState::Loading);
                        }
                        let _ = context.event_tx.send(PlayerEvent::Buffering {
                            generation,
                            percent,
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::QueueOverflow) => {
                warn!("libmpv event queue overflow");
            }
            Ok(Event::Shutdown) => {
                if !context.shutdown.load(Ordering::SeqCst)
                    && let Some(generation) =
                        current_generation(current_file_generation, &context.generation)
                {
                    let _ = context.state_tx.send(PlayerState::Stopped);
                    let _ = context.event_tx.send(PlayerEvent::Error {
                        generation,
                        message: "libmpv 播放核心意外退出".to_string(),
                    });
                }
                break;
            }
            Ok(_) => {}
            Err(error) => {
                if let Some(generation) =
                    current_generation(current_file_generation, &context.generation)
                {
                    let _ = context.state_tx.send(PlayerState::Stopped);
                    let _ = context.event_tx.send(PlayerEvent::Error {
                        generation,
                        message: error.to_string(),
                    });
                } else {
                    warn!("stale libmpv event error: {error}");
                }
                current_file_generation = None;
            }
        }
    }
}

fn loading_state(paused: &AtomicBool) -> PlayerState {
    if paused.load(Ordering::SeqCst) {
        PlayerState::Paused
    } else {
        PlayerState::Loading
    }
}

fn apply_pending_seek(
    event_client: &Mpv,
    generation: u64,
    pending_seek: &Mutex<Option<(u64, Duration)>>,
) {
    let position = take_pending_seek(pending_seek, generation);

    if let Some(position) = position
        && let Err(error) = event_client.set_property("time-pos", position.as_secs_f64())
    {
        warn!("libmpv deferred seek failed: {error}");
        *pending_seek.lock().unwrap() = Some((generation, position));
    }
}

fn take_pending_seek(
    pending_seek: &Mutex<Option<(u64, Duration)>>,
    generation: u64,
) -> Option<Duration> {
    let mut pending = pending_seek.lock().unwrap();
    match *pending {
        Some((pending_generation, position)) if pending_generation == generation => {
            pending.take();
            Some(position)
        }
        Some((pending_generation, _)) if pending_generation < generation => {
            pending.take();
            None
        }
        _ => None,
    }
}

fn current_generation(event_generation: Option<u64>, active_generation: &AtomicU64) -> Option<u64> {
    let event_generation = event_generation?;
    (event_generation != 0 && event_generation == active_generation.load(Ordering::SeqCst))
        .then_some(event_generation)
}

fn event_is_current(event_generation: Option<u64>, active_generation: &AtomicU64) -> bool {
    current_generation(event_generation, active_generation).is_some()
}

fn duration_from_mpv_seconds(seconds: f64) -> Option<Duration> {
    seconds
        .is_finite()
        .then(|| Duration::from_secs_f64(seconds.max(0.0)))
}

fn format_http_header_fields(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .filter(|(name, value)| {
            !name.is_empty() && !name.contains(['\r', '\n']) && !value.contains(['\r', '\n'])
        })
        .map(|(name, value)| format!("{name}: {value}"))
        .map(|field| field.replace('\\', "\\\\").replace(',', "\\,"))
        .collect::<Vec<_>>()
        .join(",")
}

impl Player for MpvEngine {
    fn prepare(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.paused.store(false, Ordering::SeqCst);
        *self.pending_seek.lock().unwrap() = None;
        let _ = self.state_tx.send(PlayerState::Loading);
        let _ = self.position_tx.send(Duration::ZERO);
        let _ = self.audible_position_tx.send(Duration::ZERO);
        let _ = self.duration_tx.send(Duration::ZERO);
        generation
    }

    fn play(&self, url: &str, generation: u64) -> bool {
        self.play_inner(url, generation, &[])
    }

    fn play_with_headers(&self, url: &str, generation: u64, headers: &[(String, String)]) -> bool {
        self.play_inner(url, generation, headers)
    }

    fn pause(&self) {
        if !matches!(
            *self.state_rx.borrow(),
            PlayerState::Playing | PlayerState::Loading
        ) {
            return;
        }
        if let Err(error) = self.mpv.set_property("pause", true) {
            warn!("libmpv pause failed: {error}");
            return;
        }
        self.paused.store(true, Ordering::SeqCst);
        let _ = self.state_tx.send(PlayerState::Paused);
    }

    fn resume(&self) {
        if *self.state_rx.borrow() != PlayerState::Paused {
            return;
        }
        if let Err(error) = self.mpv.set_property("pause", false) {
            warn!("libmpv resume failed: {error}");
            return;
        }
        self.paused.store(false, Ordering::SeqCst);
        let _ = self.state_tx.send(PlayerState::Playing);
    }

    fn stop(&self) {
        let _play_guard = self.play_lock.lock().unwrap();
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        *self.pending_seek.lock().unwrap() = None;
        if let Err(error) = self.mpv.command("stop", &[]) {
            warn!("libmpv stop failed: {error}");
        }
        let _ = self.state_tx.send(PlayerState::Stopped);
        let _ = self.position_tx.send(Duration::ZERO);
        let _ = self.audible_position_tx.send(Duration::ZERO);
        let _ = self.duration_tx.send(Duration::ZERO);
    }

    fn toggle(&self) {
        let state = *self.state_rx.borrow();
        match state {
            PlayerState::Playing => self.pause(),
            PlayerState::Paused => self.resume(),
            _ => {}
        }
    }

    fn seek(&self, position: Duration) {
        let duration = *self.duration_rx.borrow();
        let position = if duration.is_zero() {
            position
        } else {
            position.min(duration)
        };
        let _ = self.position_tx.send(position);
        let _ = self.audible_position_tx.send(position);
        if *self.state_rx.borrow() == PlayerState::Loading {
            let generation = self.generation.load(Ordering::SeqCst);
            *self.pending_seek.lock().unwrap() = Some((generation, position));
        }
        if let Err(error) = self.mpv.set_property("time-pos", position.as_secs_f64()) {
            warn!("libmpv seek failed: {error}");
        }
    }

    fn state_watcher(&self) -> watch::Receiver<PlayerState> {
        self.state_rx.clone()
    }

    fn position_watcher(&self) -> watch::Receiver<Duration> {
        self.position_rx.clone()
    }

    fn audible_position_watcher(&self) -> watch::Receiver<Duration> {
        self.audible_position_rx.clone()
    }

    fn duration_watcher(&self) -> watch::Receiver<Duration> {
        self.duration_rx.clone()
    }

    fn take_event_receiver(&self) -> Option<mpsc::UnboundedReceiver<PlayerEvent>> {
        self.event_rx.lock().unwrap().take()
    }

    fn volume(&self) -> u32 {
        self.volume.load(Ordering::Relaxed)
    }

    fn set_volume(&self, volume: u32) {
        let volume = volume.clamp(0, 100);
        if let Err(error) = self.mpv.set_property("volume", f64::from(volume)) {
            warn!("libmpv set_volume failed: {error}");
            return;
        }
        self.volume.store(volume, Ordering::Relaxed);
    }

    fn volume_up(&self, delta: u32) {
        self.set_volume(self.volume().saturating_add(delta));
    }

    fn volume_down(&self, delta: u32) {
        self.set_volume(self.volume().saturating_sub(delta));
    }
}

impl Drop for MpvEngine {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.mpv.command("stop", &[]);
        if let Some(event_thread) = self.event_thread.lock().unwrap().take()
            && event_thread.join().is_err()
        {
            warn!("libmpv event thread panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use lx_core::model::source::PlayerState;

    use super::{
        duration_from_mpv_seconds, format_http_header_fields, loading_state, take_pending_seek,
    };

    #[test]
    fn invalid_mpv_times_are_handled_safely() {
        assert_eq!(duration_from_mpv_seconds(-0.001), Some(Duration::ZERO));
        assert_eq!(duration_from_mpv_seconds(f64::NAN), None);
        assert_eq!(duration_from_mpv_seconds(f64::INFINITY), None);
    }

    #[test]
    fn positive_mpv_times_keep_fractional_seconds() {
        assert_eq!(
            duration_from_mpv_seconds(1.25),
            Some(Duration::from_millis(1_250))
        );
    }

    #[test]
    fn headers_are_sanitized_and_escaped_for_mpv_string_lists() {
        let headers = vec![
            ("Referer".to_string(), "https://example.com/a,b".to_string()),
            ("X-Bad\nHeader".to_string(), "ignored".to_string()),
            ("Cookie".to_string(), r"a=b\c".to_string()),
        ];

        assert_eq!(
            format_http_header_fields(&headers),
            r"Referer: https://example.com/a\,b,Cookie: a=b\\c"
        );
    }

    #[test]
    fn empty_headers_clear_previous_mpv_headers() {
        assert_eq!(format_http_header_fields(&[]), "");
    }

    #[test]
    fn loading_events_preserve_a_pending_pause() {
        let paused = AtomicBool::new(false);
        assert_eq!(loading_state(&paused), PlayerState::Loading);

        paused.store(true, Ordering::SeqCst);
        assert_eq!(loading_state(&paused), PlayerState::Paused);
    }

    #[test]
    fn pending_seek_is_applied_only_to_its_generation() {
        let pending = Mutex::new(Some((4, Duration::from_secs(12))));

        assert_eq!(take_pending_seek(&pending, 3), None);
        assert_eq!(
            take_pending_seek(&pending, 4),
            Some(Duration::from_secs(12))
        );
        assert_eq!(take_pending_seek(&pending, 4), None);
    }

    #[test]
    fn stale_pending_seek_is_discarded() {
        let pending = Mutex::new(Some((4, Duration::from_secs(12))));

        assert_eq!(take_pending_seek(&pending, 5), None);
        assert!(pending.lock().unwrap().is_none());
    }
}
