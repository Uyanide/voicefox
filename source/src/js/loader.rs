//! JS 音源下载与缓存
//!
//! 从 URL 下载 JS 脚本，缓存到 `~/.config/voicefox/sources/` 目录

use crate::http::SendWithRetry;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::engine::JsEngine;
use super::js_source::JsSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IntegrityPolicy {
    Enforce,
    ApproveChanges,
}

/// JS 音源脚本下载大小上限，防止异常超大响应占用内存
const MAX_SOURCE_SIZE: u64 = 10 * 1024 * 1024;

/// 一次下载失败：`message` 给用户看，`retryable` 决定外层要不要重试。
///
/// 只有网络类瞬时错误值得重试；HTTP 状态码、大小超限、内容为空、本地文件
/// 读取失败、完整性校验失败都是确定性失败——重试只会把同一个错误放大成
/// 「已重试 3 次」，把真正的原因淹没（issue #44 的报错观感）。
struct SourceDownloadError {
    message: String,
    retryable: bool,
}

impl SourceDownloadError {
    fn fatal(message: String) -> Self {
        Self {
            message,
            retryable: false,
        }
    }

    fn transient(message: String) -> Self {
        Self {
            message,
            retryable: true,
        }
    }
}

fn local_source_path(input: &str) -> Option<PathBuf> {
    let value = input.strip_prefix("file://").unwrap_or(input);
    let path = if let Some(relative) = value.strip_prefix("~/") {
        dirs::home_dir()?.join(relative)
    } else {
        PathBuf::from(value)
    };
    path.is_file().then_some(path)
}

/// JS 音源脚本缓存目录：`<config>/voicefox/sources`。
///
/// 数据目录早已统一叫 voicefox，这里曾经还是 lx-tui 旧名——用户按文档备份
/// `voicefox` 目录时带不走音源，重装后要重新导入。首次使用时把旧目录的文件
/// 一次性搬过来。
fn sources_cache_dir() -> PathBuf {
    static MIGRATED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let new_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("voicefox")
        .join("sources");
    if MIGRATED.set(()).is_ok() {
        let legacy_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lx-tui")
            .join("sources");
        migrate_legacy_sources_cache(&legacy_dir, &new_dir);
    }
    new_dir
}

/// 把 lx-tui 旧缓存目录的脚本搬进新目录（只搬一次、绝不覆盖新目录已有内容）。
fn migrate_legacy_sources_cache(legacy_dir: &Path, new_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(legacy_dir) else {
        return;
    };
    let new_dir_in_use = std::fs::read_dir(new_dir)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if new_dir_in_use {
        return;
    }
    if std::fs::create_dir_all(new_dir).is_err() {
        return;
    }
    let mut moved_any = false;
    for entry in entries.flatten() {
        let target = new_dir.join(entry.file_name());
        if target.exists() {
            continue;
        }
        match std::fs::rename(entry.path(), &target) {
            Ok(()) => moved_any = true,
            // 跨设备 rename 会失败：退化为复制，旧文件留在原地也无妨。
            Err(_) => moved_any |= std::fs::copy(entry.path(), &target).is_ok(),
        }
    }
    if moved_any {
        tracing::info!(
            "已把 JS 音源缓存从 {} 迁移到 {}",
            legacy_dir.display(),
            new_dir.display()
        );
    }
    // 搬空后顺手清掉旧目录；里面还有别的文件时删不掉，留着也无妨。
    let _ = std::fs::remove_dir(legacy_dir);
}

/// 获取 URL 对应的本地缓存路径。
pub fn cached_source_path(url: &str) -> PathBuf {
    use md5::Digest;

    let hash = md5::Md5::digest(url.as_bytes());
    sources_cache_dir().join(format!("{:x}.js", hash))
}

pub fn is_source_cached(url: &str) -> bool {
    valid_cached_path(&cached_source_path(url)).is_some()
}

fn valid_cached_path(path: &Path) -> Option<PathBuf> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Some(path.to_path_buf()),
        _ => None,
    }
}

fn integrity_path(path: &Path) -> PathBuf {
    path.with_extension("sha256")
}

fn sha256_hex(code: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(code))
}

fn write_integrity(path: &Path, code: &[u8]) -> Result<(), String> {
    std::fs::write(integrity_path(path), sha256_hex(code))
        .map_err(|e| format!("写入 JS 音源完整性记录失败: {e}"))
}

fn verified_cached_path(path: &Path) -> Result<Option<PathBuf>, String> {
    let Some(path) = valid_cached_path(path) else {
        return Ok(None);
    };
    let code = std::fs::read(&path).map_err(|e| format!("读取 JS 音源缓存失败: {e}"))?;
    let actual = sha256_hex(&code);
    let pin_path = integrity_path(&path);
    match std::fs::read_to_string(&pin_path) {
        Ok(expected) if expected.trim().eq_ignore_ascii_case(&actual) => Ok(Some(path)),
        Ok(_) => Err("JS 音源缓存完整性校验失败".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_integrity(&path, &code)?;
            Ok(Some(path))
        }
        Err(error) => Err(format!("读取 JS 音源完整性记录失败: {error}")),
    }
}

fn write_cache(path: &Path, code: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建缓存目录失败: {e}"))?;
    }

    let temporary_path = unique_sibling_path(path, "tmp");
    std::fs::write(&temporary_path, code).map_err(|e| format!("写入缓存临时文件失败: {e}"))?;
    if let Err(error) = replace_file(&temporary_path, path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(format!("替换 JS 音源缓存失败: {error}"));
    }
    Ok(())
}

fn unique_sibling_path(path: &Path, suffix: &str) -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!(
        "{}.{}.{}.{}",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("cache"),
        std::process::id(),
        id,
        suffix
    ))
}

#[cfg(not(windows))]
fn replace_file(temporary_path: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::rename(temporary_path, path)
}

#[cfg(windows)]
fn replace_file(temporary_path: &Path, path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return std::fs::rename(temporary_path, path);
    }

    let backup_path = unique_sibling_path(path, "backup");
    std::fs::rename(path, &backup_path)?;
    match std::fs::rename(temporary_path, path) {
        Ok(()) => {
            let _ = std::fs::remove_file(backup_path);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::rename(&backup_path, path);
            Err(error)
        }
    }
}

fn replace_cache(path: &Path, code: &[u8]) -> Result<(), String> {
    write_cache(path, code)?;
    if let Err(error) = write_integrity(path, code) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

/// 下载 JS 音源文件到本地缓存。网络失败时保留并使用已有有效缓存。
///
/// 返回缓存后的文件路径。文件名基于 URL 的 MD5 hash。
pub async fn download_source(url: &str) -> Result<PathBuf, String> {
    download_source_with_policy(url, IntegrityPolicy::Enforce)
        .await
        .map_err(|error| error.message)
}

async fn download_source_with_policy(
    url: &str,
    policy: IntegrityPolicy,
) -> Result<PathBuf, SourceDownloadError> {
    let path = cached_source_path(url);

    if let Some(local_path) = local_source_path(url) {
        let code = std::fs::read(&local_path).map_err(|e| {
            SourceDownloadError::fatal(format!(
                "读取本地 JS 音源失败（{}）: {e}",
                local_path.display()
            ))
        })?;
        if code.iter().all(u8::is_ascii_whitespace) {
            let trusted_cache = verified_cached_path(&path).map_err(SourceDownloadError::fatal)?;
            return trusted_cache
                .ok_or_else(|| SourceDownloadError::fatal("本地 JS 音源内容为空".to_string()));
        }
        replace_cache(&path, &code).map_err(SourceDownloadError::fatal)?;
        return Ok(path);
    }

    // `trusted_cache` 有值时，任何下载失败都退回这份可用缓存（ok_or 才会
    // 把失败向上传）。
    let trusted_cache = verified_cached_path(&path).map_err(SourceDownloadError::fatal)?;
    let client = super::super::http::client();

    let response = client
        .get(url)
        .send_with_retry(crate::http::RETRY_ATTEMPTS)
        .await;

    let mut resp = match response {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            let error = SourceDownloadError::fatal(format!(
                "下载 JS 音源失败（HTTP {}）",
                resp.status().as_u16()
            ));
            return trusted_cache.ok_or(error);
        }
        Err(e) => {
            let error =
                SourceDownloadError::transient(format!("下载 JS 音源失败（网络错误）: {e}"));
            return trusted_cache.ok_or(error);
        }
    };

    // 先看 content-length，超限直接拒绝
    if let Some(length) = resp.content_length()
        && length > MAX_SOURCE_SIZE
    {
        let error = SourceDownloadError::fatal(format!(
            "JS 音源超过 {}MB 大小上限，已拒绝下载",
            MAX_SOURCE_SIZE / 1024 / 1024
        ));
        return trusted_cache.ok_or(error);
    }

    // 分块累计下载并检查上限，防止超大响应拖垮内存
    let mut code_bytes: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if code_bytes.len() as u64 + chunk.len() as u64 > MAX_SOURCE_SIZE {
                    let error = SourceDownloadError::fatal(format!(
                        "JS 音源超过 {}MB 大小上限，已拒绝下载",
                        MAX_SOURCE_SIZE / 1024 / 1024
                    ));
                    return trusted_cache.ok_or(error);
                }
                code_bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                let error = SourceDownloadError::transient(format!("读取 JS 音源内容失败: {e}"));
                return trusted_cache.ok_or(error);
            }
        }
    }

    let code = match String::from_utf8(code_bytes) {
        Ok(code) if !code.trim().is_empty() => code,
        Ok(_) => {
            let error = SourceDownloadError::fatal("下载的 JS 音源内容为空".to_string());
            return trusted_cache.ok_or(error);
        }
        Err(_) => {
            let error =
                SourceDownloadError::fatal("下载的 JS 音源内容不是有效 UTF-8 文本".to_string());
            return trusted_cache.ok_or(error);
        }
    };

    let downloaded_hash = sha256_hex(code.as_bytes());
    if policy == IntegrityPolicy::Enforce
        && let Ok(expected) = std::fs::read_to_string(integrity_path(&path))
        && !expected.trim().eq_ignore_ascii_case(&downloaded_hash)
    {
        tracing::warn!(
            "remote JS source changed; keeping pinned cache until the user re-imports it"
        );
        let error = SourceDownloadError::fatal(
            "远程 JS 音源内容已变化，请在设置中重新导入以确认更新".to_string(),
        );
        return trusted_cache.ok_or(error);
    }

    // 先完整写入临时文件，再替换缓存，避免失败下载截断上一份可用脚本。
    replace_cache(&path, code.as_bytes()).map_err(SourceDownloadError::fatal)?;

    Ok(path)
}

/// 下载并启动一个 JS 音源。下载阶段的网络类瞬时错误最多重试三次；
/// 脚本初始化失败是确定性错误，不重试（原因见 [`SourceDownloadError`]）。
pub async fn load_source(url: &str, default_source: &str) -> Result<JsSource, String> {
    load_source_with_policy(url, default_source, IntegrityPolicy::Enforce).await
}

/// 用户主动导入音源时允许确认同一 URL 的内容更新。
pub async fn load_source_approving_update(
    url: &str,
    default_source: &str,
) -> Result<JsSource, String> {
    load_source_with_policy(url, default_source, IntegrityPolicy::ApproveChanges).await
}

async fn load_source_with_policy(
    url: &str,
    default_source: &str,
    policy: IntegrityPolicy,
) -> Result<JsSource, String> {
    let cache_path = cached_source_path(url);
    let previous_cache = std::fs::read(&cache_path)
        .ok()
        .filter(|code| !code.is_empty());
    let previous_integrity = std::fs::read(integrity_path(&cache_path)).ok();

    // 失败后尽量把上一份可用脚本恢复回缓存：绝对不能把失败下载留下的半截
    // 内容留在缓存里，也不能因为这次失败弄丢上次能用且校验过的脚本。
    let restore_previous_cache = |message: &mut String| {
        if let Some(code) = &previous_cache {
            if let Err(restore_error) = write_cache(&cache_path, code) {
                *message = format!("{message}；恢复旧缓存失败: {restore_error}");
            } else if let Some(integrity) = &previous_integrity {
                if let Err(restore_error) = std::fs::write(integrity_path(&cache_path), integrity) {
                    *message = format!("{message}；恢复完整性记录失败: {restore_error}");
                }
            } else if let Err(restore_error) = write_integrity(&cache_path, code) {
                *message = format!("{message}；重建完整性记录失败: {restore_error}");
            }
        } else {
            let _ = std::fs::remove_file(&cache_path);
            let _ = std::fs::remove_file(integrity_path(&cache_path));
        }
    };

    // 1) 下载：网络类瞬时错误最多重试三次，确定性失败立刻返回。
    let mut path = None;
    let mut last_error = "未知错误".to_string();
    for attempt in 1..=3 {
        match download_source_with_policy(url, policy).await {
            Ok(downloaded) => {
                path = Some(downloaded);
                break;
            }
            Err(error) => {
                last_error = error.message;
                restore_previous_cache(&mut last_error);
                if !error.retryable {
                    return Err(last_error);
                }
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_millis(250 * attempt as u64)).await;
                }
            }
        }
    }
    let Some(path) = path else {
        return Err(format!("JS 音源下载失败（已重试 3 次）: {last_error}"));
    };

    // 2) 引擎启动：node 缺失、不支持权限模式、脚本初始化崩溃都是确定性
    //    失败（错误里已带退出码与 stderr 详情），重试只会重复同一个错误。
    let path_string = path
        .to_str()
        .ok_or_else(|| "JS 音源路径包含无效字符".to_string())?
        .to_string();
    let engine = match tokio::task::spawn_blocking(move || JsEngine::new(&path_string)).await {
        Ok(Ok(engine)) => engine,
        Ok(Err(error)) => {
            let mut message = error;
            restore_previous_cache(&mut message);
            return Err(message);
        }
        Err(error) => {
            let mut message = format!("启动 JS 引擎任务失败: {error}");
            restore_previous_cache(&mut message);
            return Err(message);
        }
    };
    let name = source_display_name(&path, url);
    Ok(JsSource::new(name, engine, default_source.to_string()))
}

/// JS 音源的展示名称：优先读脚本里的 `@name`，否则从来源 URL / 本地路径推导。
///
/// 加载成功时用作 `JsSource` 的名字；加载失败时调用方也用它给用户一个可读的
/// 名称（而不是一长串 ghproxy URL）。两处必须是同一份逻辑，否则名称对不上。
///
/// `path` 不存在时（例如首次下载就失败）会退回按 `origin` 推导，不会 panic。
pub fn source_display_name(path: &std::path::Path, origin: &str) -> String {
    if let Ok(code) = std::fs::read_to_string(path)
        && let Some(name) = metadata_value(&code, "name")
    {
        return name;
    }

    let clean_origin = origin.split(['?', '#']).next().unwrap_or(origin);
    let segments = clean_origin
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let file_name = segments
        .last()
        .copied()
        .unwrap_or("JS 音源")
        .strip_suffix(".js")
        .unwrap_or_else(|| segments.last().copied().unwrap_or("JS 音源"));
    if file_name == "latest" {
        return segments
            .iter()
            .rev()
            .nth(1)
            .copied()
            .unwrap_or("JS 音源")
            .to_string();
    }
    if file_name.is_empty() {
        "JS 音源".to_string()
    } else {
        file_name.to_string()
    }
}

fn metadata_value(code: &str, key: &str) -> Option<String> {
    code.lines().find_map(|line| {
        let line = line
            .trim()
            .trim_start_matches("/*")
            .trim_start_matches('*')
            .trim_start_matches("//")
            .trim();
        let value = line.strip_prefix(&format!("@{key}"))?;
        if !value.starts_with(char::is_whitespace) {
            return None;
        }
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{metadata_value, migrate_legacy_sources_cache, source_display_name, write_cache};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_lx_music_script_name_metadata() {
        let code = "/*\n * @name\t聚合测试音源\n * @version 1.0.0\n */";
        assert_eq!(
            metadata_value(code, "name").as_deref(),
            Some("聚合测试音源")
        );
    }

    #[test]
    fn legacy_sources_dir_migrates_once_and_never_overwrites() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "voicefox-loader-migrate-{}-{unique}",
            std::process::id(),
        ));
        let legacy = root.join("lx-tui").join("sources");
        let new_dir = root.join("voicefox").join("sources");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("a.js"), b"// old script a").unwrap();
        std::fs::write(legacy.join("b.sha256"), b"deadbeef").unwrap();

        migrate_legacy_sources_cache(&legacy, &new_dir);

        assert_eq!(
            std::fs::read(new_dir.join("a.js")).unwrap(),
            b"// old script a"
        );
        assert!(new_dir.join("b.sha256").is_file());
        assert!(!legacy.exists(), "搬空后的旧目录应被清理");

        // 新目录已有内容时绝不覆盖：旧目录里的东西留在原地。
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("a.js"), b"// legacy copy").unwrap();
        migrate_legacy_sources_cache(&legacy, &new_dir);
        assert_eq!(
            std::fs::read(new_dir.join("a.js")).unwrap(),
            b"// old script a",
            "迁移不得覆盖新目录已有文件"
        );
        assert!(legacy.join("a.js").is_file(), "未搬走的文件应留在旧目录");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn source_name_falls_back_to_origin_filename() {
        let name = source_display_name(
            std::path::Path::new("/tmp/voicefox-missing-source.js"),
            "https://example.com/custom-source.js?raw=1",
        );
        assert_eq!(name, "custom-source");

        let latest = source_display_name(
            std::path::Path::new("/tmp/voicefox-missing-source.js"),
            "https://example.com/juhe/latest.js",
        );
        assert_eq!(latest, "juhe");
    }

    #[test]
    fn cache_write_replaces_existing_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "voicefox-loader-test-{}-{unique}",
            std::process::id(),
        ));
        let path = dir.join("source.js");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"old").unwrap();

        write_cache(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn invalid_source_url_returns_error_without_panicking() {
        let result = tokio::spawn(super::download_source("::::not-a-url::::"))
            .await
            .expect("loader task must not panic");
        assert!(result.is_err());
    }
}
