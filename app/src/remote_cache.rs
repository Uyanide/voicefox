//! 网易云「远程集合」进程缓存。
//!
//! 网易云歌单不再复制成本地歌单，而是把远端歌单连同歌曲元数据缓存在这里；
//! 播放时仍以 `SourceId::Wy` 走现有的网易云地址解析。
//!
//! 两个关键约束：
//! - **增量合并优先**：远端拉取出现局部失败时只允许按 id 合并，不能整体替换，
//!   否则一次频控就会把用户已缓存的歌单静默清空。
//! - **落盘**：缓存必须跨进程存活，否则重启后歌单页会从「我的歌单」悄悄退回
//!   「热门歌单」、收藏页变成空，同一功能两次启动行为不一致。

use lx_core::sync::{SyncCollection, SyncCollectionKind};
use std::path::Path;
#[cfg(not(test))]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

static NETEASE_CACHE: OnceLock<RwLock<Vec<SyncCollection>>> = OnceLock::new();
static NETEASE_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(not(test))]
fn cache_path() -> PathBuf {
    crate::storage::default_data_dir().join("netease_collections.json")
}

fn cache() -> &'static RwLock<Vec<SyncCollection>> {
    NETEASE_CACHE.get_or_init(|| {
        let loaded = load_from_disk();
        if !loaded.is_empty() {
            // 从磁盘恢复的内容也算「有新数据」，否则第一次访问时页面缓存会
            // 认为代次仍是 0，从而把已落盘的歌单当成空的。
            NETEASE_GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        RwLock::new(loaded)
    })
}

/// 读缓存文件。文件缺失或 JSON 损坏都当空处理：一份缓存坏了不能拖垮启动。
fn load_from(path: &Path) -> Vec<SyncCollection> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<SyncCollection>>(&raw) {
        Ok(collections) => collections,
        Err(error) => {
            tracing::warn!("网易云远程歌单缓存解析失败({}): {error}", path.display());
            Vec::new()
        }
    }
}

/// 原子写缓存文件：先写临时文件再 rename，避免写到一半被中断留下半个 JSON。
fn save_to(path: &Path, collections: &[SyncCollection]) {
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("创建网易云缓存目录失败: {error}");
        return;
    }
    let Ok(body) = serde_json::to_vec(collections) else {
        tracing::warn!("序列化网易云远程歌单缓存失败");
        return;
    };
    let temp = path.with_extension("json.tmp");
    if let Err(error) = std::fs::write(&temp, &body) {
        tracing::warn!("写入网易云缓存失败: {error}");
        return;
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        tracing::warn!("替换网易云缓存失败: {error}");
        let _ = std::fs::remove_file(&temp);
    }
}

#[cfg(not(test))]
fn load_from_disk() -> Vec<SyncCollection> {
    load_from(&cache_path())
}

/// 单元测试不读写真实数据目录，避免污染用户数据。
#[cfg(test)]
fn load_from_disk() -> Vec<SyncCollection> {
    Vec::new()
}

#[cfg(not(test))]
fn save_to_disk(collections: &[SyncCollection]) {
    save_to(&cache_path(), collections);
}

#[cfg(test)]
fn save_to_disk(_collections: &[SyncCollection]) {}

/// 按 id 增量合并（远端已删除的歌单不会被移除）。
///
/// 用于远端拉取存在局部失败的场合：成功的那部分照常更新，失败的那部分保留
/// 缓存里的旧数据。
pub fn merge_netease(collections: Vec<SyncCollection>) {
    let Ok(mut guard) = cache().write() else {
        return;
    };
    for collection in collections {
        match guard.iter_mut().find(|item| item.id == collection.id) {
            Some(existing) => *existing = collection,
            None => guard.push(collection),
        }
    }
    save_to_disk(&guard);
    NETEASE_GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// 整体替换。
///
/// 只有确认远端结果是完整的时候才允许调用（见 `SyncCollectionSet::is_complete`），
/// 这样远端删除歌单才会从缓存里消失。
pub fn replace_netease(collections: Vec<SyncCollection>) {
    if let Ok(mut guard) = cache().write() {
        *guard = collections;
        save_to_disk(&guard);
        NETEASE_GENERATION.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn generation() -> u64 {
    NETEASE_GENERATION.load(Ordering::Relaxed)
}

pub fn all_netease() -> Vec<SyncCollection> {
    cache()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

/// 只读借用，避免渲染路径每帧深拷贝整个缓存。
pub fn with_netease<R>(read: impl FnOnce(&[SyncCollection]) -> R) -> R {
    match cache().read() {
        Ok(guard) => read(&guard),
        Err(poisoned) => read(&poisoned.into_inner()),
    }
}

/// `(总数, 普通歌单数, 红心歌单数)`，供设置页渲染使用（不拷贝歌曲）。
pub fn summary_counts() -> (usize, usize, usize) {
    with_netease(|collections| {
        let normal = collections
            .iter()
            .filter(|item| item.kind == SyncCollectionKind::Playlist)
            .count();
        let favorites = collections
            .iter()
            .filter(|item| item.kind == SyncCollectionKind::Favorites)
            .count();
        (collections.len(), normal, favorites)
    })
}

/// 红心歌单的歌曲；没有缓存时返回空。
pub fn favorites_songs() -> Vec<lx_core::model::song::SongInfo> {
    with_netease(|collections| {
        collections
            .iter()
            .find(|item| item.kind == SyncCollectionKind::Favorites)
            .map(|item| item.songs.clone())
            .unwrap_or_default()
    })
}

pub fn playlist(id: &str) -> Option<SyncCollection> {
    with_netease(|collections| collections.iter().find(|item| item.id == id).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::SourceId;

    /// 这些测试共用同一个进程级缓存，而 `cargo test` 默认多线程并行跑，
    /// 不加锁会互相覆盖（一个测试的 replace 会抹掉另一个测试刚写的数据）。
    fn lock_cache() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn collection(kind: SyncCollectionKind, id: &str, name: &str, songs: usize) -> SyncCollection {
        SyncCollection {
            kind,
            id: id.into(),
            name: name.into(),
            source: SourceId::Wy,
            songs: (0..songs)
                .map(|index| {
                    SongInfo::new(
                        index.to_string(),
                        SourceId::Wy,
                        format!("Song{index}"),
                        "Singer".into(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn replaces_and_reads_netease_collections() {
        let _guard = lock_cache();
        replace_netease(vec![collection(
            SyncCollectionKind::Playlist,
            "p1",
            "Test",
            1,
        )]);
        assert_eq!(playlist("p1").unwrap().songs[0].name, "Song0");
    }

    #[test]
    fn merge_keeps_collections_that_are_absent_from_the_update() {
        let _guard = lock_cache();
        replace_netease(vec![
            collection(SyncCollectionKind::Playlist, "p1", "Old", 3),
            collection(SyncCollectionKind::Playlist, "p2", "Untouched", 2),
        ]);
        // 只更新 p1：p2 必须原样保留，否则局部失败就等于丢歌单。
        merge_netease(vec![collection(
            SyncCollectionKind::Playlist,
            "p1",
            "New",
            5,
        )]);

        let p1 = playlist("p1").expect("p1");
        assert_eq!(p1.name, "New");
        assert_eq!(p1.songs.len(), 5);
        let p2 = playlist("p2").expect("p2");
        assert_eq!(p2.songs.len(), 2);
    }

    #[test]
    fn replace_removes_collections_gone_from_remote() {
        let _guard = lock_cache();
        replace_netease(vec![
            collection(SyncCollectionKind::Playlist, "p1", "A", 1),
            collection(SyncCollectionKind::Playlist, "p2", "B", 1),
        ]);
        replace_netease(vec![collection(SyncCollectionKind::Playlist, "p1", "A", 1)]);
        assert!(playlist("p2").is_none());
    }

    #[test]
    fn summary_counts_splits_normal_and_favorites() {
        let _guard = lock_cache();
        replace_netease(vec![
            collection(SyncCollectionKind::Playlist, "p1", "A", 1),
            collection(SyncCollectionKind::Playlist, "p2", "B", 1),
            collection(SyncCollectionKind::Favorites, "f1", "我喜欢的音乐", 4),
        ]);
        assert_eq!(summary_counts(), (3, 2, 1));
    }

    #[test]
    fn favorites_songs_reads_only_the_favorites_collection() {
        let _guard = lock_cache();
        replace_netease(vec![
            collection(SyncCollectionKind::Playlist, "p1", "A", 9),
            collection(SyncCollectionKind::Favorites, "f1", "我喜欢的音乐", 4),
        ]);
        assert_eq!(favorites_songs().len(), 4);
    }

    #[test]
    fn collections_survive_a_json_round_trip() {
        // 落盘依赖 SyncCollection（含 SongInfo）的 serde 往返；这里直接验证
        // 序列化再反序列化不会丢字段，否则重启后缓存会变成空歌单。
        let original = vec![
            collection(SyncCollectionKind::Playlist, "p1", "歌单 A", 3),
            collection(SyncCollectionKind::Favorites, "f1", "我喜欢的音乐", 2),
        ];
        let encoded = serde_json::to_vec(&original).expect("serialize");
        let decoded: Vec<SyncCollection> = serde_json::from_slice(&encoded).expect("deserialize");

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].name, "歌单 A");
        assert_eq!(decoded[0].songs.len(), 3);
        assert_eq!(decoded[0].songs[0].name, "Song0");
        assert_eq!(decoded[0].songs[0].source, SourceId::Wy);
        assert_eq!(decoded[1].kind, SyncCollectionKind::Favorites);
        assert_eq!(decoded[1].songs.len(), 2);
    }

    #[test]
    fn cache_file_round_trips_and_survives_corruption() {
        // 直接测真实读写路径：`save_to_disk`/`load_from_disk` 在 cfg(test) 下是
        // 空实现（避免污染用户数据），所以这里用临时目录验证落盘本身。
        let dir = std::env::temp_dir().join(format!(
            "voicefox-netease-cache-test-{}",
            std::process::id()
        ));
        let path = dir.join("netease_collections.json");
        let original = vec![collection(SyncCollectionKind::Playlist, "p1", "歌单 A", 3)];

        save_to(&path, &original);
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "歌单 A");
        assert_eq!(loaded[0].songs.len(), 3);
        assert_eq!(loaded[0].songs[0].source, SourceId::Wy);

        // 损坏的缓存只能被当成空，不能让启动失败。
        std::fs::write(&path, b"{ this is not json").expect("write corrupt");
        assert!(load_from(&path).is_empty());

        // 文件缺失同理。
        let _ = std::fs::remove_file(&path);
        assert!(load_from(&path).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
