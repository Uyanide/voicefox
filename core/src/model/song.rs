use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use super::source::{AudioProperties, Quality, SourceId};

pub const EXTRA_FILE_MODIFIED_UNIX_NANOS: &str = "file_modified_unix_nanos";

/// 统一歌曲模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongInfo {
    pub id: String,
    pub source: SourceId,
    pub name: String,
    /// 歌手，格式：歌手A、歌手B
    pub singer: String,
    pub album_name: String,
    pub album_id: String,
    pub duration: Duration,
    pub cover_url: Option<String>,
    pub qualities: BTreeSet<Quality>,
    /// 实际编码参数，仅本地文件可获取
    #[serde(default)]
    pub audio: Option<AudioProperties>,

    /// 音源特有数据（按 key 存取）
    pub extra: HashMap<String, String>,

    /// 换源后匹配的目标歌曲
    pub toggle_source: Option<Box<SongInfo>>,

    // 本地文件特有
    pub file_path: Option<PathBuf>,
    pub file_ext: Option<String>,
}

impl SongInfo {
    pub fn new(id: String, source: SourceId, name: String, singer: String) -> Self {
        Self {
            id,
            source,
            name,
            singer,
            album_name: String::new(),
            album_id: String::new(),
            duration: Duration::ZERO,
            cover_url: None,
            qualities: BTreeSet::new(),
            audio: None,
            extra: HashMap::new(),
            toggle_source: None,
            file_path: None,
            file_ext: None,
        }
    }

    /// 界面显示的音质文本
    ///
    /// - 本地文件 - 实测规格
    /// - 在线音源 - 回落到档位
    pub fn quality_label(&self) -> String {
        if let Some(label) = self.audio.as_ref().and_then(AudioProperties::label) {
            return label;
        }
        self.qualities
            .iter()
            .next_back()
            .map_or_else(|| "-".to_string(), |quality| quality.label().to_string())
    }
}
