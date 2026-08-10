//! HTTP 客户端封装 — TODO: Phase 2
//!
//! 职责：统一 UA、超时、重试逻辑、代理支持
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

#[derive(Debug, Clone)]
struct NetworkOptions {
    proxy_url: String,
    timeout: Duration,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            proxy_url: String::new(),
            timeout: Duration::from_secs(15),
        }
    }
}

fn options() -> &'static RwLock<NetworkOptions> {
    static OPTIONS: OnceLock<RwLock<NetworkOptions>> = OnceLock::new();
    OPTIONS.get_or_init(|| RwLock::new(NetworkOptions::default()))
}

pub(crate) fn configure(proxy_url: &str, timeout_secs: u64) {
    {
        let mut options = options().write().unwrap();
        options.proxy_url = proxy_url.trim().to_string();
        options.timeout = Duration::from_secs(timeout_secs.clamp(1, 300));
    }
    // 代理或超时变化后重建全局客户端。
    *client_store().write().unwrap() = Arc::new(build_client(&options().read().unwrap().clone()));
}

/// 全局复用的 HTTP 客户端。
///
/// 每次请求都新建 `reqwest::Client` 会重复创建 TLS 会话缓存与连接池，
/// 聚合搜索并发多个音源时内存和 CPU 都会被顶高；这里全局共享一个
/// 客户端，代理/超时变化时由 `configure` 重建。
pub fn client() -> reqwest::Client {
    (**client_store().read().unwrap()).clone()
}

fn client_store() -> &'static RwLock<Arc<reqwest::Client>> {
    static CLIENT: OnceLock<RwLock<Arc<reqwest::Client>>> = OnceLock::new();
    CLIENT.get_or_init(|| {
        RwLock::new(Arc::new(build_client(&options().read().unwrap().clone())))
    })
}

fn build_client(options: &NetworkOptions) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(options.timeout)
        .user_agent("Mozilla/5.0 (compatible; voicefox/0.1)");
    if !options.proxy_url.is_empty() {
        match reqwest::Proxy::all(&options.proxy_url) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(error) => tracing::warn!("invalid proxy URL: {error}"),
        }
    }
    builder.build().expect("failed to build HTTP client")
}
