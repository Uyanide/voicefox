#[allow(dead_code)]
#[path = "cover_src/source.rs"]
mod source;

use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use source::CoverImage;

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
        let mut last = "封面请求失败".to_string();
        for attempt in 0..3 {
            if self.request_id.load(Ordering::SeqCst) != id {
                return Ok(());
            }
            match source::download_and_cache(&self.client, &url).await {
                Ok(image) => {
                    *self.image.write().unwrap_or_else(|e| e.into_inner()) = Some(image);
                    *self.state.write().unwrap_or_else(|e| e.into_inner()) = CoverState::Ready;
                    return Ok(());
                }
                Err(e) => {
                    last = e;
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(150 * (attempt + 1))).await;
                    }
                }
            }
        }
        if self.request_id.load(Ordering::SeqCst) == id {
            *self.state.write().unwrap_or_else(|e| e.into_inner()) =
                CoverState::Unavailable(last.clone());
        }
        Err(last)
    }
    pub fn image_path(&self) -> Option<String> {
        self.image
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|i| i.path.clone())
    }
}
