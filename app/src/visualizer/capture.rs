//! PCM 采集：读取系统默认输出的监视流。
//!
//! 监视流（monitor）混录的是本机正在播放的全部声音 —— cava 等终端可视化
//! 也是这个口径。采集用现成的命令行工具子进程，不引入音频后端依赖：
//!
//! 1. `pw-record`（PipeWire 自带）：`--target @DEFAULT_MONITOR@` 直接表示
//!    默认输出设备的监视流，无需查询设备名；
//! 2. `parec`（pulseaudio-utils）：需要先用 `pactl get-default-sink` 查出
//!    默认 sink 名再拼 `<sink>.monitor`。
//!
//! 两者都输出 s16le / 44.1kHz / 双声道的裸 PCM。

use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::spectrum::{Analyzer, RingBuffer, WINDOW_SIZE};

/// 采集参数（pw-record 与 parec 共用的口径）。
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u32 = 2;
/// 计算节拍：20fps 的可视化足够顺滑，也把 FFT 开销压在主循环之外。
const COMPUTE_INTERVAL: Duration = Duration::from_millis(50);
/// 单次读取的原始字节数（s16le 立体声 → 512 帧）。
const READ_CHUNK_BYTES: usize = 512 * CHANNELS as usize * 2;

/// 采集子进程的包装：Drop 时强制终止，让阻塞在 stdout 上的读立刻结束。
#[derive(Clone)]
pub(super) struct CaptureProcess {
    child: Arc<Mutex<Child>>,
}

impl CaptureProcess {
    pub(super) fn spawn(mut command: Command) -> std::io::Result<Self> {
        let child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
        })
    }

    fn take_stdout(&self) -> Option<ChildStdout> {
        self.child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stdout
            .take()
    }

    pub(super) fn kill(&self) {
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// 组装采集命令。返回 `None` 表示系统里没有可用的采集工具。
pub fn capture_command() -> Option<Command> {
    if which("pw-record") {
        let mut command = Command::new("pw-record");
        command
            .arg("--raw")
            .arg("--format")
            .arg("s16")
            .arg("--rate")
            .arg(SAMPLE_RATE.to_string())
            .arg("--channels")
            .arg(CHANNELS.to_string())
            .arg("--target")
            .arg("@DEFAULT_MONITOR@")
            .arg("-");
        return Some(command);
    }
    if which("parec") && which("pactl") {
        let sink = Command::new("pactl")
            .arg("get-default-sink")
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|name| !name.is_empty())?;
        let mut command = Command::new("parec");
        command
            .arg("-d")
            .arg(format!("{sink}.monitor"))
            .arg("--format=s16le")
            .arg(format!("--rate={SAMPLE_RATE}"))
            .arg(format!("--channels={CHANNELS}"));
        return Some(command);
    }
    None
}

fn which(program: &str) -> bool {
    // 采集命令用 PATH 解析即可；不缓存，可视化是低频开启操作。
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// 采集线程主体：读 PCM → 写环形缓冲 → 按 COMPUTE_INTERVAL 出帧。
///
/// 断流（子进程退出 / 读取失败）时会清掉旧帧并自动重启采集进程：设备切换、
/// PipeWire 重启都会让旧的监视流结束；如果就此收工，用户看到的就是一屏永远
/// 不动的柱子（"卡死"）。重启后槽位里始终是当前那枚进程，drop 杀掉它即可。
pub fn run_capture(
    current: Arc<Mutex<Option<CaptureProcess>>>,
    shutdown: Arc<AtomicBool>,
    snapshot: Arc<Mutex<Option<super::Snapshot>>>,
) {
    let mut ring = RingBuffer::new(WINDOW_SIZE * 2);
    let mut analyzer = Analyzer::new(SAMPLE_RATE);
    let mut raw = vec![0u8; READ_CHUNK_BYTES];
    let mut mono = Vec::with_capacity(READ_CHUNK_BYTES / 2);

    loop {
        if shutdown.load(Ordering::Acquire) {
            return;
        }

        let process = current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(process) = process else {
            if !spawn_capture(&current, &shutdown) {
                sleep_briefly(&shutdown);
            }
            continue;
        };
        let Some(mut stdout) = process.take_stdout() else {
            process.kill();
            *current
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            continue;
        };
        let mut last_compute = Instant::now() - COMPUTE_INTERVAL;

        loop {
            if shutdown.load(Ordering::Acquire) {
                return;
            }
            use std::io::Read;
            let read = match stdout.read(&mut raw) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) => {
                    tracing::warn!("频谱采集读取失败: {error}");
                    break;
                }
            };
            // s16le 交错立体声 → 单声道 f32（忽略落单的最后一个字节）。
            mono.clear();
            for frame in raw[..read.saturating_sub(read % 4)].chunks_exact(4) {
                let left = i16::from_le_bytes([frame[0], frame[1]]);
                let right = i16::from_le_bytes([frame[2], frame[3]]);
                mono.push((f32::from(left) + f32::from(right)) / (2.0 * f32::from(i16::MAX)));
            }
            ring.push(&mono);
            if last_compute.elapsed() >= COMPUTE_INTERVAL
                && let Some(samples) = ring.latest(WINDOW_SIZE)
            {
                let data = analyzer.analyze_data(&samples);
                let snapshot_value = super::Snapshot {
                    data,
                    peaks: analyzer.peaks().to_vec(),
                    updated_at: Instant::now(),
                };
                if let Ok(mut guard) = snapshot.try_lock() {
                    *guard = Some(snapshot_value);
                }
                last_compute = Instant::now();
            }
        }

        // 流断了：旧帧不能再当实时数据画；杀掉旧进程后重连。
        drop(stdout);
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        tracing::info!("频谱采集流结束，尝试重连");
        *snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        process.kill();
        *current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        sleep_briefly(&shutdown);
    }
}

/// 断流重连间隔：给音频服务切换留时间，也避免失败时空转烧 CPU。
const RESTART_INTERVAL: Duration = Duration::from_millis(500);

/// 尝试把一枚新的采集进程放进槽位；返回 false 表示这次没成功（下轮再试）。
fn spawn_capture(current: &Arc<Mutex<Option<CaptureProcess>>>, shutdown: &AtomicBool) -> bool {
    if shutdown.load(Ordering::Acquire) {
        return true;
    }
    let Some(command) = capture_command() else {
        return false;
    };
    match CaptureProcess::spawn(command) {
        Ok(process) => {
            *current
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(process);
            true
        }
        Err(error) => {
            tracing::warn!("频谱采集重启失败: {error}");
            false
        }
    }
}

/// 以 50ms 为粒度小睡到下一次重连；期间响应 shutdown，避免关掉可视化时
/// drop 还要等满一个 RESTART_INTERVAL。
fn sleep_briefly(shutdown: &AtomicBool) {
    let mut waited = Duration::ZERO;
    while waited < RESTART_INTERVAL {
        if shutdown.load(Ordering::Acquire) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
        waited += Duration::from_millis(50);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_command_exists_on_pipewire_systems() {
        // 本测试只断言命令构造的形状；没有 pw-record/parec 的环境允许 None。
        if let Some(command) = capture_command() {
            let program = command.get_program().to_string_lossy().to_string();
            assert!(program.ends_with("pw-record") || program.ends_with("parec"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn killing_capture_process_unblocks_stdout_read() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 60"]);
        let process = CaptureProcess::spawn(command).unwrap();
        let mut stdout = process.take_stdout().unwrap();
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut byte = [0u8; 1];
            stdout.read(&mut byte)
        });

        process.kill();

        assert_eq!(reader.join().unwrap().unwrap(), 0);
    }

    #[test]
    fn read_chunk_covers_at_least_one_window_per_second() {
        // 44.1kHz × 512 帧/读 → 每秒约 86 次读，50ms 节拍下窗口填充绰绰有余。
        let chunks_per_second = SAMPLE_RATE as f32 / 512.0;
        assert!(chunks_per_second > 1.0 / COMPUTE_INTERVAL.as_secs_f32());
    }

    /// 端到端冒烟：真实起 pw-record 采 2 秒系统音频，断言产出了一帧合法频谱。
    /// 静音环境下柱子全 0 也算通过 —— 验证的是"链路通"，不是"有声"。
    #[test]
    #[ignore = "需要系统音频栈（pipewire/pulse），CI 无设备"]
    fn capture_stream_produces_valid_spectrum_frame() {
        use std::sync::Mutex;

        let Some(command) = capture_command() else {
            panic!("本机应提供 pw-record 或 parec");
        };
        let shutdown = Arc::new(AtomicBool::new(false));
        let snapshot = Arc::new(Mutex::new(None));
        let current = Arc::new(Mutex::new(Some(CaptureProcess::spawn(command).unwrap())));
        let worker_current = Arc::clone(&current);
        let worker = {
            let shutdown = Arc::clone(&shutdown);
            let snapshot = Arc::clone(&snapshot);
            std::thread::spawn(move || run_capture(worker_current, shutdown, snapshot))
        };
        std::thread::sleep(Duration::from_secs(2));
        shutdown.store(true, Ordering::Release);
        if let Some(process) = current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            process.kill();
        }
        let _ = worker.join();

        let frame = snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("capture should publish a frame within 2s");
        assert_eq!(
            frame.data.spectrum.len(),
            super::super::spectrum::BAND_COUNT
        );
        assert_eq!(frame.peaks.len(), super::super::spectrum::BAND_COUNT);
        assert!(
            frame
                .data
                .spectrum
                .iter()
                .chain(frame.peaks.iter())
                .all(|level| (0.0..=1.0).contains(level))
        );
    }
}
