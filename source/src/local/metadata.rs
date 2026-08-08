//! 音频元数据读取（使用 lofty）

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::Picture;
use lofty::tag::{Accessor, ItemKey, Tag};

use lx_core::model::song::SongInfo;
use lx_core::model::source::{AudioProperties, Quality};

static COVER_TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
const COVER_TEMP_INFIX: &str = ".part.";

/// 可写入音频文件的标签字段。
///
/// `None` 表示保留原值；设置为 `Some` 时会更新对应字段。封面数据必须是
/// `lofty` 支持的 JPEG/PNG 等格式。
#[derive(Debug, Clone, Default)]
pub struct MetadataEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub cover: Option<Vec<u8>>,
}

/// 将标签和可选封面写回本地音频文件。
pub fn write_metadata(path: &Path, edit: &MetadataEdit) -> Result<(), String> {
    let mut tagged =
        lofty::read_from_path(path).map_err(|error| format!("读取标签失败: {error}"))?;

    if tagged.primary_tag_mut().is_none() {
        let tag_type = tagged.primary_tag_type();
        if !tagged.supports_tag_type(tag_type) {
            return Err(format!("{} 不支持写入标签", path.display()));
        }
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or_else(|| format!("{} 没有可写入的标签", path.display()))?;
    if let Some(title) = edit.title.as_ref() {
        tag.set_title(title.clone());
    }
    if let Some(artist) = edit.artist.as_ref() {
        tag.set_artist(artist.clone());
    }
    if let Some(album) = edit.album.as_ref() {
        tag.set_album(album.clone());
    }
    if let Some(cover) = edit.cover.as_ref() {
        let picture = Picture::from_reader(&mut Cursor::new(cover))
            .map_err(|error| format!("封面格式无效: {error}"))?;
        tag.set_picture(0, picture);
    }

    tagged
        .save_to_path(path, WriteOptions::default())
        .map_err(|error| format!("写入标签失败: {error}"))
}

/// 读取音频文件的元数据
pub fn read_metadata(path: &Path) -> Result<SongInfo, String> {
    let tagged = lofty::read_from_path(path).map_err(|e| format!("lofty error: {}", e))?;

    let properties = tagged.properties();
    let duration = properties.duration();
    // lofty 未解析出的字段取值为 0
    let audio = AudioProperties {
        bitrate: properties.audio_bitrate().filter(|bitrate| *bitrate != 0),
        sample_rate: properties.sample_rate().filter(|rate| *rate != 0),
        bit_depth: properties.bit_depth().filter(|depth| *depth != 0),
        // 位深字段的存在与否是 MP4 的编码判据，取值 0 同样代表存在
        lossless: is_lossless(tagged.file_type(), properties.bit_depth()),
    };

    // 提取主标签
    let (title, artist, album) = if let Some(tag) = tagged.first_tag() {
        let title = tag
            .title()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| file_stem(path));

        let artist = tag
            .artist()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未知艺术家".to_string());

        let album = tag
            .album()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "未知专辑".to_string());

        (title, artist, album)
    } else {
        (
            file_stem(path),
            "未知艺术家".to_string(),
            "未知专辑".to_string(),
        )
    };

    // 提取封面并缓存
    let cover_path = extract_cover(&tagged, path);

    Ok(SongInfo {
        id: path.to_string_lossy().to_string(),
        name: title,
        singer: artist,
        album_name: album,
        album_id: String::new(),
        duration,
        source: lx_core::model::source::SourceId::Local,
        qualities: classify(&audio).into_iter().collect(),
        audio: Some(audio),
        cover_url: cover_path,
        extra: std::collections::HashMap::new(),
        toggle_source: None,
        file_path: None,
        file_ext: path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string()),
    })
}

/// 判断编码是否无损
///
/// 按容器判断的近似结果。WavPack 的 hybrid 模式、格式标记非 PCM 与 IEEE_FLOAT
/// 的 WAV、带压缩类型的 AIFC 三者实为有损，此处判为无损。三者的准确标识在
/// `FileProperties` 的转换过程被丢弃，取值需要按具体类型重新解析文件。但实
/// 际应用场景极其有限因此不做处理。
fn is_lossless(file_type: FileType, bit_depth: Option<u8>) -> bool {
    match file_type {
        FileType::Flac | FileType::Ape | FileType::WavPack | FileType::Wav | FileType::Aiff => true,
        // AAC、ALAC 与 FLAC 共用 MP4 容器，lofty 只在后两者的解析分支写入位深
        FileType::Mp4 => bit_depth.is_some(),
        _ => false,
    }
}

/// 将实际编码参数归入 `Quality` 档位
///
/// 无损文件的码率随音乐内容浮动，规格取决于位深与采样率。有损文件的码率即规格
fn classify(audio: &AudioProperties) -> Option<Quality> {
    if audio.lossless {
        let hi_res = audio.bit_depth.is_some_and(|depth| depth > 16)
            || audio.sample_rate.is_some_and(|rate| rate > 48_000);
        return Some(if hi_res {
            Quality::Flac24
        } else {
            Quality::Flac
        });
    }
    // 码率缺失则无从判断档位
    let bitrate = audio.bitrate?;
    // 224 是 128 与 320 的中点
    Some(if bitrate >= 224 {
        Quality::High320
    } else {
        Quality::Low128
    })
}

/// 读取音频文件中嵌入的歌词。
///
/// Lofty 会将 ID3 USLT、Vorbis `LYRICS`、MP4 lyrics 等格式统一映射到
/// `ItemKey::Lyrics`，因此这里不需要针对容器格式分别解析。
pub fn read_embedded_lyric(path: &Path) -> Result<Option<String>, String> {
    let tagged = lofty::read_from_path(path).map_err(|e| format!("lofty error: {}", e))?;
    Ok(tagged.tags().iter().find_map(embedded_lyric_from_tag))
}

fn embedded_lyric_from_tag(tag: &Tag) -> Option<String> {
    tag.get_strings(&ItemKey::Lyrics)
        .map(str::trim)
        .find(|lyric| !lyric.is_empty())
        .map(ToOwned::to_owned)
}

/// 从文件名提取歌曲名（不含扩展名）
fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("未知歌曲")
        .to_string()
}

/// 提取嵌入的封面图并缓存到磁盘
fn extract_cover(tagged: &lofty::file::TaggedFile, audio_path: &Path) -> Option<String> {
    let tag = tagged.first_tag()?;

    // 尝试读取封面
    let picture = tag.pictures().first()?;

    // 缓存目录
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("voicefox")
        .join("covers");

    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(&cache_dir);
    }

    // 路径相同的文件可能被重新嵌入了封面。将图片内容纳入缓存键，避免标签编辑后
    // 继续显示旧封面；未变化的文件仍会命中缓存。
    let mut cache_key = audio_path.to_string_lossy().as_bytes().to_vec();
    cache_key.extend_from_slice(picture.data());
    let hash = simple_hash(&cache_key);
    let cover_path = cache_dir.join(format!("{}.jpg", hash));

    if cover_path.exists() {
        if validate_cover(&cover_path) {
            return Some(cover_path.to_string_lossy().to_string());
        }
        tracing::debug!("local cover cache {cover_path:?} is corrupt, rebuilding");
        let _ = std::fs::remove_file(&cover_path);
    }

    let data = picture.data();
    if write_cover_cache(&cover_path, data).is_ok() {
        Some(cover_path.to_string_lossy().to_string())
    } else {
        None
    }
}

fn validate_cover(path: &Path) -> bool {
    image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
        .is_ok_and(|image| image.width() > 0 && image.height() > 0)
}

fn cover_temp_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        "{COVER_TEMP_INFIX}{}.{}",
        std::process::id(),
        COVER_TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    target.with_file_name(name)
}

fn write_cover_cache(target: &Path, data: &[u8]) -> std::io::Result<()> {
    let temp_path = cover_temp_path(target);
    let result = (|| {
        std::fs::write(&temp_path, data)?;
        if !validate_cover(&temp_path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "embedded cover image is corrupt",
            ));
        }
        std::fs::rename(&temp_path, target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// 简单的字符串哈希
fn simple_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        classify, embedded_lyric_from_tag, is_lossless, validate_cover, write_cover_cache,
    };
    use lofty::file::FileType;
    use lofty::tag::{ItemKey, Tag, TagType};
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::{AudioProperties, Quality};

    fn audio(
        lossless: bool,
        bitrate: u32,
        sample_rate: u32,
        bit_depth: Option<u8>,
    ) -> AudioProperties {
        AudioProperties {
            bitrate: Some(bitrate),
            sample_rate: Some(sample_rate),
            bit_depth,
            lossless,
        }
    }

    #[test]
    fn mp4_is_lossless_only_when_a_bit_depth_was_parsed() {
        assert!(is_lossless(FileType::Mp4, Some(24)));
        assert!(is_lossless(FileType::Mp4, Some(0)));
        assert!(!is_lossless(FileType::Mp4, None));
    }

    #[test]
    fn container_decides_losslessness_outside_mp4() {
        assert!(is_lossless(FileType::Flac, None));
        assert!(is_lossless(FileType::Wav, Some(16)));
        assert!(!is_lossless(FileType::Mpeg, Some(16)));
        assert!(!is_lossless(FileType::Opus, None));
    }

    #[test]
    fn lossless_files_are_classified_by_depth_and_sample_rate() {
        let cases = [
            (audio(true, 5475, 192_000, Some(24)), Quality::Flac24),
            (audio(true, 900, 44_100, Some(24)), Quality::Flac24),
            (audio(true, 3000, 96_000, Some(16)), Quality::Flac24),
            (audio(true, 551, 44_100, Some(16)), Quality::Flac),
        ];
        for (audio, expected) in cases {
            assert_eq!(classify(&audio), Some(expected));
        }
    }

    #[test]
    fn a_high_bitrate_lossless_file_never_lands_in_a_lossy_tier() {
        let quality = classify(&audio(true, 5475, 192_000, Some(24)));
        assert!(matches!(quality, Some(Quality::Flac | Quality::Flac24)));
    }

    #[test]
    fn lossy_files_are_classified_by_a_midpoint_threshold() {
        let cases = [
            (audio(false, 321, 44_100, None), Quality::High320),
            (audio(false, 224, 44_100, None), Quality::High320),
            (audio(false, 223, 44_100, None), Quality::Low128),
            (audio(false, 96, 44_100, None), Quality::Low128),
        ];
        for (audio, expected) in cases {
            assert_eq!(classify(&audio), Some(expected));
        }
    }

    #[test]
    fn a_lossless_file_is_labelled_by_depth_and_sample_rate() {
        let cases = [
            (audio(true, 5475, 192_000, Some(24)), "24/192"),
            (audio(true, 2478, 96_000, Some(24)), "24/96"),
            (audio(true, 551, 44_100, Some(16)), "16/44.1"),
            (audio(true, 4000, 176_400, Some(24)), "24/176.4"),
        ];
        for (audio, expected) in cases {
            assert_eq!(audio.label().as_deref(), Some(expected));
        }
    }

    #[test]
    fn a_lossy_file_is_labelled_by_its_measured_bitrate() {
        let cases = [
            (audio(false, 321, 44_100, None), "321K"),
            (audio(false, 320, 44_100, None), "320K"),
            (audio(false, 220, 44_100, None), "220K"),
            (audio(false, 107, 44_100, None), "107K"),
        ];
        for (audio, expected) in cases {
            assert_eq!(audio.label().as_deref(), Some(expected));
        }
    }

    #[test]
    fn a_lossless_file_missing_a_bit_depth_falls_back_to_the_sample_rate() {
        let no_depth = AudioProperties {
            bitrate: Some(900),
            sample_rate: Some(44_100),
            bit_depth: None,
            lossless: true,
        };

        assert_eq!(no_depth.label().as_deref(), Some("44.1kHz"));
    }

    #[test]
    fn a_lossy_file_without_a_parsed_bitrate_gets_no_tier_at_all() {
        let unknown = AudioProperties {
            bitrate: None,
            sample_rate: Some(44_100),
            bit_depth: None,
            lossless: false,
        };

        assert_eq!(classify(&unknown), None);

        let mut song = SongInfo::new(
            "1".to_string(),
            lx_core::model::source::SourceId::Local,
            "歌曲".to_string(),
            "歌手".to_string(),
        );
        song.qualities = classify(&unknown).into_iter().collect();
        song.audio = Some(unknown);

        assert_eq!(song.quality_label(), "-");
    }

    #[test]
    fn reads_and_trims_embedded_lyric() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.insert_text(ItemKey::Lyrics, " \n[00:01.00]歌词\n ".to_string());

        assert_eq!(
            embedded_lyric_from_tag(&tag).as_deref(),
            Some("[00:01.00]歌词")
        );
    }

    fn png_bytes() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(8, 4));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn embedded_cover_cache_is_validated_before_replacement() {
        let dir = std::env::temp_dir().join("voicefox-local-cover-valid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("cover.jpg");
        std::fs::write(&target, b"corrupt").unwrap();

        write_cover_cache(&target, &png_bytes()).unwrap();

        assert!(validate_cover(&target));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn corrupt_embedded_cover_data_is_not_cached() {
        let dir = std::env::temp_dir().join("voicefox-local-cover-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("cover.jpg");
        let mut bytes = png_bytes();
        bytes.truncate(33);

        assert!(write_cover_cache(&target, &bytes).is_err());
        assert!(!target.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }
}
