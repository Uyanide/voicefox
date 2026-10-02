//! 频谱计算：环形缓冲 → 加窗 FFT → 对数频带 → dB 归一化 → 平滑与峰值。
//!
//! 视觉口径参考 cava：
//! - 50Hz–12kHz 对数频带（能量集中在可听主体，柱子不会挤在左边）；
//! - monstercat 式相邻频带平滑（柱子连成"山脊"而不是杂乱尖刺）；
//! - 攻击快 / 衰减慢的时间平滑 + 慢速下落的峰值标记（peak caps）；
//! - 慢释放的自动增益，让普通音量下柱子也能撑起大部分高度。

use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex};

/// Hann 窗系数（周期形式）。
pub fn hann_window(len: usize) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * i as f32 / len as f32;
            0.5 * (1.0 - x.cos())
        })
        .collect()
}

/// FFT 窗口长度（样本数）。48kHz 下约 85ms，频率分辨率 ~11.7Hz。
pub const WINDOW_SIZE: usize = 4096;
/// 固定输出的频带数；渲染层按终端宽度重采样。
pub const BAND_COUNT: usize = 96;
/// 分析频带范围（Hz）：cava 默认口径附近，可听主体最密集的区段。
const BAND_MIN_HZ: f32 = 50.0;
const BAND_MAX_HZ: f32 = 12_000.0;
/// dB 显示下限：低于此电平视为静音。
const DB_FLOOR: f32 = -55.0;
/// 时间平滑：上升系数（跟手）与下降系数（余韵）。
const ATTACK: f32 = 0.60;
const DECAY: f32 = 0.72;
/// 相邻频带平滑系数与作用距离（monstercat 滤波）。
const NEIGHBOR_FACTOR: f32 = 0.55;
const NEIGHBOR_REACH: usize = 4;
/// 自动增益：目标满高电平、运行峰值的衰减率（每帧）与增益上限。
const AUTO_GAIN_TARGET: f32 = 0.85;
const AUTO_GAIN_PEAK_DECAY: f32 = 0.995;
const AUTO_GAIN_MIN_PEAK: f32 = 0.40;
const AUTO_GAIN_MAX: f32 = 2.2;
/// 峰值标记每帧下落的高度（0..1 柱高）。
const PEAK_FALL: f32 = 0.035;

/// 单声道 f32 环形缓冲（写入来自采集线程，读取在同一线程内完成）。
pub struct RingBuffer {
    buf: Vec<f32>,
    write: usize,
    filled: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity],
            write: 0,
            filled: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for sample in samples {
            self.buf[self.write] = *sample;
            self.write = (self.write + 1) % self.buf.len();
        }
        self.filled = (self.filled + samples.len()).min(self.buf.len());
    }

    /// 拷出最新的 `count` 个样本（时间正序）。样本不足时返回 `None`。
    pub fn latest(&self, count: usize) -> Option<Vec<f32>> {
        if self.filled < count {
            return None;
        }
        let mut out = vec![0.0f32; count];
        let len = self.buf.len();
        for (offset, slot) in out.iter_mut().enumerate() {
            let index = (self.write + len - count + offset) % len;
            *slot = self.buf[index];
        }
        Some(out)
    }
}

/// 对外提供给任意 UI 的音频分析结果；UI 不需要知道 FFT 如何实现。
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct VisualizerData {
    pub spectrum: Vec<f32>,
    pub waveform: Vec<f32>,
    pub rms: f32,
    pub peak: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
}

/// 一次频带分析的可复用中间量；FFT planner 与工作缓冲均长期复用。
pub struct Analyzer {
    window: Vec<f32>,
    fft: std::sync::Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    /// 每个频带对应的 FFT bin 区间（半开区间）。
    bands: Vec<(usize, usize)>,
    /// 上一帧的平滑结果（供衰减）。
    smoothed: Vec<f32>,
    /// 上一帧的峰值标记（渲染成柱顶的下落小帽）。
    peaks: Vec<f32>,
    /// 运行峰值（慢衰减）：自动增益的依据。
    running_peak: f32,
}

impl Analyzer {
    pub fn new(sample_rate: u32) -> Self {
        let n = WINDOW_SIZE;
        let half = n / 2;
        let bin_hz = sample_rate as f32 / n as f32;
        // 对数频带：30Hz..16kHz 均分 BAND_COUNT 段，换算成 bin 区间。
        let log_lo = BAND_MIN_HZ.ln();
        let log_hi = BAND_MAX_HZ.ln();
        let mut bands = Vec::with_capacity(BAND_COUNT);
        for band in 0..BAND_COUNT {
            let f_lo = (log_lo + (log_hi - log_lo) * band as f32 / BAND_COUNT as f32).exp();
            let f_hi = (log_lo + (log_hi - log_lo) * (band + 1) as f32 / BAND_COUNT as f32).exp();
            let start = ((f_lo / bin_hz).floor() as usize).clamp(1, half - 1);
            let end = ((f_hi / bin_hz).ceil() as usize).clamp(start + 1, half);
            bands.push((start, end));
        }
        Self {
            window: hann_window(n),
            fft: {
                let mut planner = RealFftPlanner::<f32>::new();
                planner.plan_fft_forward(n)
            },
            input: vec![0.0; n],
            spectrum: vec![Complex::default(); n / 2 + 1],
            bands,
            smoothed: vec![0.0; BAND_COUNT],
            peaks: vec![0.0; BAND_COUNT],
            running_peak: 0.0,
        }
    }

    /// 计算完整的可视化数据，UI 与 FFT 实现完全解耦。
    pub fn analyze_data(&mut self, samples: &[f32]) -> VisualizerData {
        let n = WINDOW_SIZE;
        for index in 0..n {
            self.input[index] = samples.get(index).copied().unwrap_or(0.0) * self.window[index];
        }
        if let Err(error) = self.fft.process(&mut self.input, &mut self.spectrum) {
            tracing::warn!("realfft processing failed: {error}");
            return VisualizerData::default();
        }

        let mut raw = Vec::with_capacity(BAND_COUNT);
        for (start, end) in &self.bands {
            // 频带内取最大 bin 幅度：窄频带在低频段只覆盖 1~2 个 bin，
            // 取平均会让鼓点峰值被静音稀释。
            let magnitude = (*start..*end)
                .map(|bin| self.spectrum[bin].norm())
                .fold(0.0f32, |acc, value| acc.max(value));
            // 归一化：窗口能量 + dB 显示范围。1e-9 防止 log(0)。
            let db = 20.0 * (magnitude / (n as f32 / 4.0) + 1e-9).log10();
            raw.push(((db - DB_FLOOR) / -DB_FLOOR).clamp(0.0, 1.0));
        }

        // 自动增益：按慢衰减的运行峰值抬升整体电平，普通音量下柱子也
        // 能撑起大部分高度（cava 的 auto-sensitivity 口径）。
        let frame_peak = raw.iter().fold(0.0f32, |acc, level| acc.max(*level));
        self.running_peak = (self.running_peak * AUTO_GAIN_PEAK_DECAY).max(frame_peak);
        let gain =
            (AUTO_GAIN_TARGET / self.running_peak.max(AUTO_GAIN_MIN_PEAK)).min(AUTO_GAIN_MAX);
        let output: Vec<f32> = raw
            .iter()
            .map(|level| (level * gain).clamp(0.0, 1.0))
            .collect();

        // monstercat 式相邻平滑：让相邻柱子互相牵引，连成山脊。
        let mut blended = output.clone();
        for (index, slot) in blended.iter_mut().enumerate() {
            for distance in 1..=NEIGHBOR_REACH {
                let weight = NEIGHBOR_FACTOR.powi(distance as i32);
                if let Some(left) = index.checked_sub(distance) {
                    *slot = (*slot).max(output[left] * weight);
                }
                if let Some(right) = index
                    .checked_add(distance)
                    .filter(|candidate| *candidate < BAND_COUNT)
                {
                    *slot = (*slot).max(output[right] * weight);
                }
            }
        }

        // 时间平滑：上升快（跟手），下降慢（余韵）；同时推进峰值标记。
        for (index, level) in blended.iter().enumerate() {
            let previous = self.smoothed[index];
            let factor = if *level > previous { ATTACK } else { DECAY };
            self.smoothed[index] = previous + (*level - previous) * factor;
            let peak = &mut self.peaks[index];
            if self.smoothed[index] > *peak {
                *peak = self.smoothed[index];
            } else {
                *peak = (*peak - PEAK_FALL).max(self.smoothed[index]);
            }
        }
        let spectrum = self.smoothed.clone();
        let waveform = waveform_samples(samples);
        let rms = rms(samples);
        let peak = samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0f32, f32::max)
            .clamp(0.0, 1.0);
        let bass = band_average(&spectrum, 0, BAND_COUNT / 3);
        let mid = band_average(&spectrum, BAND_COUNT / 3, BAND_COUNT * 2 / 3);
        let treble = band_average(&spectrum, BAND_COUNT * 2 / 3, BAND_COUNT);
        VisualizerData {
            spectrum,
            waveform,
            rms,
            peak,
            bass,
            mid,
            treble,
        }
    }

    /// 当前帧的峰值标记（与最近一次 [`Self::analyze`] 的返回值对齐）。
    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
        .sqrt()
        .clamp(0.0, 1.0)
}

fn waveform_samples(samples: &[f32]) -> Vec<f32> {
    const POINTS: usize = 96;
    if samples.is_empty() {
        return vec![0.0; POINTS];
    }
    (0..POINTS)
        .map(|index| {
            let start = index * samples.len() / POINTS;
            let end = ((index + 1) * samples.len() / POINTS).max(start + 1);
            samples[start..end.min(samples.len())]
                .iter()
                .copied()
                .fold(0.0f32, |best, sample| {
                    if sample.abs() > best.abs() {
                        sample
                    } else {
                        best
                    }
                })
        })
        .collect()
}

fn band_average(values: &[f32], start: usize, end: usize) -> f32 {
    if start >= end || start >= values.len() {
        return 0.0;
    }
    let end = end.min(values.len());
    values[start..end].iter().sum::<f32>() / (end - start) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_returns_latest_samples_in_order() {
        let mut ring = RingBuffer::new(8);
        ring.push(&[1.0, 2.0, 3.0]);
        ring.push(&[4.0, 5.0, 6.0, 7.0, 8.0]);
        // 缓冲容量 8：3 已被覆盖，最新 5 个是 4..8。
        assert_eq!(ring.latest(5), Some(vec![4.0, 5.0, 6.0, 7.0, 8.0]));
        assert_eq!(ring.latest(9), None);
    }

    #[test]
    fn analyzer_maps_tone_into_upper_bands() {
        // 1kHz 正弦在 48kHz 采样下应落在中频段，且明显高于低频段。
        let sample_rate = 48_000u32;
        let mut analyzer = Analyzer::new(sample_rate);
        let samples: Vec<f32> = (0..WINDOW_SIZE)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sample_rate as f32).sin())
            .collect();
        let bars = analyzer.analyze_data(&samples).spectrum;
        assert_eq!(bars.len(), BAND_COUNT);
        let peak = bars
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(index, _)| index)
            .unwrap();
        // 1kHz 在 30Hz..16kHz 对数轴上约位于 63% 处。
        assert!((50..BAND_COUNT).contains(&peak), "1kHz peak at band {peak}");
        assert!(bars[peak] > 0.5, "tone should register well above floor");
    }

    #[test]
    fn silence_stays_silent() {
        let mut analyzer = Analyzer::new(48_000);
        let samples = vec![0.0f32; WINDOW_SIZE];
        let bars = analyzer.analyze_data(&samples).spectrum;
        assert!(bars.iter().all(|level| *level < 0.01));
    }
}
