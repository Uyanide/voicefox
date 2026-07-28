//! 封面的获取与本地缓存

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::header::{ACCEPT, REFERER};

use super::layout::DEFAULT_IMAGE_ASPECT;

/// 临时文件名的流水号
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 临时文件名里的记号
const TEMP_INFIX: &str = ".part.";

/// 比这个新的临时文件当作别的实例正在写，不动
const TEMP_GRACE: Duration = Duration::from_secs(60);

/// 封面缓存目录
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("voicefox")
        .join("covers")
}

/// 清掉进程被强杀时留在缓存目录里的临时文件
pub async fn sweep_temp_files() {
    let Ok(mut entries) = tokio::fs::read_dir(cache_dir()).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry.file_name().to_string_lossy().contains(TEMP_INFIX) {
            continue;
        }
        if let Ok(metadata) = entry.metadata().await
            && metadata
                .modified()
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age < TEMP_GRACE))
        {
            continue;
        }
        match tokio::fs::remove_file(entry.path()).await {
            Ok(()) => tracing::debug!("removed stale cover temp file {:?}", entry.path()),
            Err(error) => tracing::debug!("remove stale cover temp file failed: {error}"),
        }
    }
}

/// 已就绪的封面
#[derive(Debug, Clone)]
pub struct CoverImage {
    pub path: String,
    /// 像素 宽/高
    pub aspect: f32,
}

/// 下载封面到本地缓存，返回缓存路径与像素宽高比
pub async fn download_and_cache(client: &reqwest::Client, url: &str) -> Result<CoverImage, String> {
    let cache_dir = cache_dir();

    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(&cache_dir);
    }

    // 本地文件直接返回路径
    if url.starts_with('/') || url.starts_with("file://") {
        let path = url.strip_prefix("file://").unwrap_or(url);
        if !std::path::Path::new(path).exists() {
            return Err("封面文件不存在".to_string());
        }
        return Ok(CoverImage {
            path: path.to_string(),
            aspect: probe_aspect(path).await.unwrap_or(DEFAULT_IMAGE_ASPECT),
        });
    }

    // 远程文件：下载到缓存
    let hash = simple_hash(url.as_bytes());
    let cache_path = cache_dir.join(format!("{}.jpg", hash));

    match probe_aspect(&cache_path).await {
        Some(aspect) => {
            return Ok(CoverImage {
                path: cache_path.to_string_lossy().to_string(),
                aspect,
            });
        }
        // 探不出宽高说明文件损坏
        // 也可能是因为实际为 image crate default-formats 不包括的格式，但反正不认识，和损坏没两样
        None if cache_path.exists() => {
            tracing::debug!("cover cache {cache_path:?} is unreadable, downloading again");
            let _ = tokio::fs::remove_file(&cache_path).await;
        }
        None => {}
    }

    // HTTP 下载
    let mut request = client
        .get(url)
        .header(ACCEPT, "image/webp,image/apng,image/*,*/*;q=0.8");
    if let Some(referer) = cover_referer(url) {
        request = request.header(REFERER, referer);
    }
    let bytes = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .bytes()
        .await
        .map_err(|error| error.to_string())?;

    let target = cache_path.clone();
    tokio::task::spawn_blocking(move || write_cache_file(&target, &bytes))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("写入封面缓存失败: {error}"))?;

    Ok(CoverImage {
        path: cache_path.to_string_lossy().to_string(),
        aspect: probe_aspect(&cache_path)
            .await
            .unwrap_or(DEFAULT_IMAGE_ASPECT),
    })
}

/// 同目录下的临时文件路径
fn temp_path_for(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(
        "{TEMP_INFIX}{}.{}",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    target.with_file_name(name)
}

/// 写入缓存文件，确保原子性
fn write_cache_file(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp_path = temp_path_for(target);
    std::fs::write(&temp_path, bytes)
        .and_then(|()| std::fs::rename(&temp_path, target))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp_path);
        })
}

/// 读图片文件头取像素宽高比，错误返回 None
fn probe_aspect_blocking(path: &std::path::Path) -> Option<f32> {
    let (width, height) = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(width as f32 / height as f32)
}

pub async fn probe_aspect(path: impl AsRef<std::path::Path>) -> Option<f32> {
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || probe_aspect_blocking(&path))
        .await
        .ok()
        .flatten()
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    }
}

fn cover_referer(url: &str) -> Option<&'static str> {
    if url.contains("kuwo.cn") {
        Some("https://www.kuwo.cn/")
    } else if url.contains("kugou.com") {
        Some("https://www.kugou.com/")
    } else if url.contains("qq.com") {
        Some("https://y.qq.com/")
    } else if url.contains("music.163.com") || url.contains("126.net") {
        Some("https://music.163.com/")
    } else {
        None
    }
}

fn simple_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use super::{probe_aspect_blocking, write_cache_file};

    /// 建一个空的临时目录，返回路径
    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("voicefox-cache-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// 目录里的文件名，排过序
    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_cache_write_replaces_the_target_instead_of_truncating_it() {
        let dir = temp_dir("replace");
        let target = dir.join("cover.jpg");
        std::fs::write(&target, b"old").unwrap();

        // 先攥住旧文件的句柄，rename 换掉的只是目录项
        let mut old_handle = std::fs::File::open(&target).unwrap();
        write_cache_file(&target, b"new").unwrap();

        let mut old = Vec::new();
        old_handle.read_to_end(&mut old).unwrap();
        assert_eq!(old, b"old", "写入前打开的句柄应仍读到旧内容");
        assert_eq!(std::fs::read(&target).unwrap(), b"new", "新内容应已就位");
    }

    #[test]
    fn a_finished_cache_write_leaves_no_temp_file() {
        let dir = temp_dir("finished");
        let target = dir.join("cover.jpg");
        write_cache_file(&target, b"payload").unwrap();
        assert_eq!(names(&dir), ["cover.jpg"], "目录里应只剩目标文件");
    }

    #[test]
    fn a_failed_cache_write_leaves_no_temp_file() {
        let dir = temp_dir("failed");
        // 目标是个目录，rename 过不去
        let target = dir.join("cover.jpg");
        std::fs::create_dir(&target).unwrap();

        assert!(write_cache_file(&target, b"payload").is_err(), "应该报错");
        assert_eq!(names(&dir), ["cover.jpg"], "临时文件应已清掉");
    }

    #[test]
    fn probe_reads_dimensions_even_when_the_extension_lies() {
        // 缓存文件名一律是 .jpg，实际内容却可能是任何格式
        let path = std::env::temp_dir().join("voicefox-cover-probe.jpg");
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(20, 10))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(probe_aspect_blocking(&path), Some(2.0));
    }
}
