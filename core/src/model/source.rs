use serde::{Deserialize, Serialize};

/// 单个音源的最近一次连通性检测结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceHealth {
    pub id: SourceId,
    pub name: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub result_count: u32,
    pub detail: String,
}

/// 音源标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceId {
    #[serde(rename = "kw")]
    Kw,
    #[serde(rename = "kg")]
    Kg,
    #[serde(rename = "tx")]
    Tx,
    #[serde(rename = "wy")]
    Wy,
    #[serde(rename = "mg")]
    Mg,
    #[serde(rename = "bili")]
    Bili,
    #[serde(rename = "local")]
    Local,
}

impl SourceId {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceId::Kw => "kw",
            SourceId::Kg => "kg",
            SourceId::Tx => "tx",
            SourceId::Wy => "wy",
            SourceId::Mg => "mg",
            SourceId::Bili => "bili",
            SourceId::Local => "local",
        }
    }

    pub fn all_online() -> &'static [SourceId] {
        &[
            SourceId::Kw,
            SourceId::Kg,
            SourceId::Tx,
            SourceId::Wy,
            SourceId::Mg,
            SourceId::Bili,
        ]
    }
}

/// 音质
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "128k")]
    Low128,
    #[serde(rename = "320k")]
    High320,
    #[serde(rename = "flac")]
    Flac,
    #[serde(rename = "flac24bit")]
    Flac24,
}

impl Quality {
    /// 档位标签
    pub fn label(self) -> &'static str {
        match self {
            Quality::Low128 => "128K",
            Quality::High320 => "320K",
            Quality::Flac => "FLAC",
            Quality::Flac24 => "Hi-Res",
        }
    }
}

/// 音质尝试顺序（高→低）
pub const QUALITY_ORDER: &[Quality] = &[
    Quality::Flac24,
    Quality::Flac,
    Quality::High320,
    Quality::Low128,
];

/// 音频文件的实际编码参数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioProperties {
    /// 音频流码率（kb/s）
    pub bitrate: Option<u32>,
    /// 采样率（Hz）
    pub sample_rate: Option<u32>,
    /// 位深（bit）
    pub bit_depth: Option<u8>,
    /// 是否为无损编码
    pub lossless: bool,
}

impl AudioProperties {
    /// 实测规格标签
    ///
    /// 无损编码的码率不表示规格，取位深与采样率。有损编码的码率即规格
    pub fn label(&self) -> Option<String> {
        if self.lossless {
            return match (self.bit_depth, self.sample_rate) {
                (Some(depth), Some(rate)) => Some(format!("{}/{}", depth, format_khz(rate))),
                (None, Some(rate)) => Some(format!("{}kHz", format_khz(rate))),
                _ => None,
            };
        }
        self.bitrate.map(|bitrate| format!("{}K", bitrate))
    }
}

/// 采样率转 kHz 显示，整千值省去小数部分
fn format_khz(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        (hz / 1000).to_string()
    } else {
        format!("{:.1}", hz as f64 / 1000.0)
    }
}

/// 播放器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Idle,
    Loading,
    Playing,
    Paused,
    Stopped,
}
