#[allow(dead_code)]
#[path = "cover_src/source.rs"]
mod source;

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use source::CoverImage;
pub use source::{is_usable_remote_url, sweep_temp_files};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverState {
    Empty,
    Loading,
    Ready,
    Unavailable(String),
}

pub struct CoverService {
    client: reqwest::Client,
    image: RwLock<Option<CoverImage>>,
    state: RwLock<CoverState>,
    request_id: AtomicU64,
}

impl CoverService {
    pub fn new(proxy_url: &str, timeout_secs: u64) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs.clamp(1, 300)))
            .user_agent("voicefox/0.3");
        if !proxy_url.trim().is_empty()
            && let Ok(proxy) = reqwest::Proxy::all(proxy_url.trim())
        {
            builder = builder.proxy(proxy);
        }
        Self {
            client: builder.build().unwrap_or_default(),
            image: RwLock::new(None),
            state: RwLock::new(CoverState::Empty),
            request_id: AtomicU64::new(0),
        }
    }
    pub fn state(&self) -> CoverState {
        self.state.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn clear(&self) {
        self.request_id.fetch_add(1, Ordering::SeqCst);
        *self.image.write().unwrap_or_else(|e| e.into_inner()) = None;
        *self.state.write().unwrap_or_else(|e| e.into_inner()) = CoverState::Empty;
    }
    pub async fn cache_path(&self, url: Option<String>) -> Result<Option<String>, String> {
        let Some(url) = url
            .map(|u| source::normalize_url(&u))
            .filter(|u| !u.trim().is_empty())
        else {
            return Ok(None);
        };
        source::download_and_cache(&self.client, &url)
            .await
            .map(|i| Some(i.path))
    }
    pub async fn load(&self, url: Option<String>) -> Result<(), String> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst) + 1;
        *self.image.write().unwrap_or_else(|e| e.into_inner()) = None;
        let Some(url) = url
            .map(|u| source::normalize_url(&u))
            .filter(|u| !u.trim().is_empty())
        else {
            *self.state.write().unwrap_or_else(|e| e.into_inner()) =
                CoverState::Unavailable("当前音源没有返回封面".into());
            return Ok(());
        };
        *self.state.write().unwrap_or_else(|e| e.into_inner()) = CoverState::Loading;
        // 重试只在下载内部针对"发送阶段的瞬时网络错误"发生（沿用 lx-source 的
        // `SendWithRetry` 策略）。这里不再整体重试：以前三层循环会把 4xx 和
        // 损坏图片也各重试三次，白白多花 450ms 才把失败显示出来。
        if self.request_id.load(Ordering::SeqCst) != id {
            return Ok(());
        }
        let result = source::download_and_cache(&self.client, &url).await;
        if self.request_id.load(Ordering::SeqCst) != id {
            return Ok(());
        }
        match result {
            Ok(image) => {
                *self.image.write().unwrap_or_else(|e| e.into_inner()) = Some(image);
                *self.state.write().unwrap_or_else(|e| e.into_inner()) = CoverState::Ready;
                Ok(())
            }
            Err(error) => {
                *self.state.write().unwrap_or_else(|e| e.into_inner()) =
                    CoverState::Unavailable(error.clone());
                Err(error)
            }
        }
    }
    pub fn image_path(&self) -> Option<String> {
        self.image
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|i| i.path.clone())
    }

    pub fn image_aspect(&self) -> f32 {
        self.image
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(1.0, |image| image.aspect)
    }
}

#[cfg(test)]
mod cover_pipeline_probe {
    //! 用**真实**的本地音乐文件验证整条链路：
    //! `lx_source::local::metadata::read_metadata` → `cover_url` → `CoverService::load`
    //! → `image_path()` 有值。默认 `#[ignore]`，需要真实文件。
    //!
    //! 跑法：
    //! `VOICEFOX_LOCAL_MUSIC_DIR=/path/to/music cargo test -p voicefox-runtime \
    //!    -- --ignored local_cover_reaches_the_cover_service --nocapture`

    use super::CoverService;
    use super::source;

    #[tokio::test]
    #[ignore = "需要真实的本地音乐目录"]
    async fn local_cover_reaches_the_cover_service() {
        let dir = std::env::var("VOICEFOX_LOCAL_MUSIC_DIR")
            .expect("请设置 VOICEFOX_LOCAL_MUSIC_DIR 指向含内嵌封面的音乐目录");
        let mut song = None;
        for entry in walkdir::WalkDir::new(&dir).max_depth(2) {
            let entry = entry.unwrap();
            if !entry.file_type().is_file() {
                continue;
            }
            if let Ok(candidate) = lx_source::local::metadata::read_metadata(entry.path())
                && candidate.cover_url.is_some()
            {
                song = Some(candidate);
                break;
            }
        }
        let song = song.expect("目录里没有带内嵌封面的音频文件");
        println!("song      = {} - {}", song.name, song.singer);
        println!("cover_url = {:?}", song.cover_url);

        // 1) 地址体检：本地绝对路径会被判为"不可用远程地址"，这是**预期**的
        let cover = song.cover_url.clone().expect("应有内嵌封面");
        println!(
            "is_usable_remote_url({cover}) = {}",
            source::is_usable_remote_url(&cover)
        );

        // 2) 真正走播放时的入口：CoverService::load
        let service = CoverService::new("", 10);
        let result = service.load(Some(cover)).await;
        println!("load result = {result:?}");
        println!("state       = {:?}", service.state());
        println!("image_path  = {:?}", service.image_path());
        println!("aspect      = {}", service.image_aspect());

        assert!(result.is_ok(), "load 失败：{result:?}");
        assert!(service.image_path().is_some(), "封面路径为空");
    }
}
