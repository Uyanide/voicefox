//! HTTP 客户端封装
//!
//! 职责：统一 UA、超时（连接 + 整体）、瞬时错误自动重试、代理支持
use std::future::Future;
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
        let mut options = options().write().unwrap_or_else(|e| e.into_inner());
        options.proxy_url = proxy_url.trim().to_string();
        options.timeout = Duration::from_secs(timeout_secs.clamp(1, 300));
    }
    // 代理或超时变化后重建全局客户端。
    *client_store().write().unwrap_or_else(|e| e.into_inner()) = Arc::new(build_client(&options().read().unwrap_or_else(|e| e.into_inner()).clone()));
}

/// 全局复用的 HTTP 客户端。
///
/// 每次请求都新建 `reqwest::Client` 会重复创建 TLS 会话缓存与连接池，
/// 聚合搜索并发多个音源时内存和 CPU 都会被顶高；这里全局共享一个
/// 客户端，代理/超时变化时由 `configure` 重建。
pub fn client() -> reqwest::Client {
    (**client_store().read().unwrap_or_else(|e| e.into_inner())).clone()
}

fn client_store() -> &'static RwLock<Arc<reqwest::Client>> {
    static CLIENT: OnceLock<RwLock<Arc<reqwest::Client>>> = OnceLock::new();
    CLIENT.get_or_init(|| RwLock::new(Arc::new(build_client(&options().read().unwrap_or_else(|e| e.into_inner()).clone()))))
}

fn build_client(options: &NetworkOptions) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(options.timeout)
        // TCP 连接挂起时快速失败，而不是吃满整个整体超时
        .connect_timeout(Duration::from_secs(8))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent("Mozilla/5.0 (compatible; voicefox/0.1)");
    if !options.proxy_url.is_empty() {
        match reqwest::Proxy::all(&options.proxy_url) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(error) => tracing::warn!("invalid proxy URL: {error}"),
        }
    }
    builder.build().expect("failed to build HTTP client")
}

/// 默认重试次数：仅连接/超时类瞬时错误，最多再试 1 次。
pub(crate) const RETRY_ATTEMPTS: usize = 1;

/// [`reqwest::RequestBuilder`] 的重试扩展。
///
/// 仅对连接失败/超时这类瞬时网络错误做有限次重试，4xx/5xx 响应不重试，
/// 由调用方自行判断业务语义。
pub(crate) trait SendWithRetry {
    fn send_with_retry(self, retries: usize) -> impl Future<Output = Result<reqwest::Response, reqwest::Error>>;
}

impl SendWithRetry for reqwest::RequestBuilder {
    async fn send_with_retry(self, retries: usize) -> Result<reqwest::Response, reqwest::Error> {
        let mut attempt = 0;
        loop {
            let request = self
                .try_clone()
                .expect("request builder must be cloneable (GET/JSON/form bodies are)");
            match request.send().await {
                Ok(resp) => return Ok(resp),
                Err(error) if attempt < retries && (error.is_connect() || error.is_timeout()) => {
                    attempt += 1;
                    let backoff = std::time::Duration::from_millis(300 * (1 << attempt.min(3)));
                    tracing::warn!("request failed (attempt {attempt}), retrying in {backoff:?}: {error}");
                    tokio::time::sleep(backoff).await;
                }
                Err(error) => return Err(error),
            }
        }
    }
}
