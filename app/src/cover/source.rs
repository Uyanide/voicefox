//! 封面的获取与本地缓存

use reqwest::header::{ACCEPT, REFERER};

use super::layout::DEFAULT_IMAGE_ASPECT;

/// 已就绪的封面
#[derive(Debug, Clone)]
pub struct CoverImage {
    pub path: String,
    /// 像素 宽/高
    pub aspect: f32,
}

/// 下载封面到本地缓存，返回缓存路径与像素宽高比
pub async fn download_and_cache(client: &reqwest::Client, url: &str) -> Result<CoverImage, String> {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("voicefox")
        .join("covers");

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

    if cache_path.exists() {
        return Ok(CoverImage {
            path: cache_path.to_string_lossy().to_string(),
            aspect: probe_aspect(&cache_path)
                .await
                .unwrap_or(DEFAULT_IMAGE_ASPECT),
        });
    }

    // HTTP 下载
    let mut request = client
        .get(url)
        .header(ACCEPT, "image/avif,image/webp,image/apng,image/*,*/*;q=0.8");
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

    let cache_path_clone = cache_path.clone();
    tokio::task::spawn_blocking(move || {
        std::fs::write(&cache_path_clone, &bytes).ok();
    })
    .await
    .ok();

    Ok(CoverImage {
        path: cache_path.to_string_lossy().to_string(),
        aspect: probe_aspect(&cache_path)
            .await
            .unwrap_or(DEFAULT_IMAGE_ASPECT),
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
    use super::probe_aspect_blocking;

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
