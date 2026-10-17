//! 远程歌单「只读镜像」进程缓存（网易云 / QQ 音乐）。
//!
//! 账号歌单不再复制成本地歌单，而是把远端歌单连同歌曲元数据缓存在这里；
//! 播放时仍以各自平台的 `SourceId` 走现有的地址解析。每个平台一个槽位、
//! 一份落盘文件，互不干扰。
//!
//! 两个关键约束：
//! - **增量合并优先**：远端拉取出现局部失败时只允许按 id 合并，不能整体替换，
//!   否则一次频控就会把用户已缓存的歌单静默清空。
//! - **落盘**：缓存必须跨进程存活，否则重启后歌单页会从「我的歌单」悄悄退回
//!   「热门歌单」、收藏页变成空，同一功能两次启动行为不一致。

use lx_core::model::source::SourceId;
use lx_core::sync::{SyncCollection, SyncCollectionKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

/// 单个平台的镜像槽位：内存镜像 + 代次 + 落盘文件名。
struct Slot {
    lock: OnceLock<RwLock<Vec<SyncCollection>>>,
    generation: AtomicU64,
    file_name: &'static str,
}

impl Slot {
    const fn new(file_name: &'static str) -> Self {
        Self {
            lock: OnceLock::new(),
            generation: AtomicU64::new(0),
            file_name,
        }
    }

    #[cfg(not(test))]
    fn cache_path(&self) -> PathBuf {
        crate::storage::default_data_dir().join(self.file_name)
    }

    /// 测试构建下没有磁盘缓存，指向一个必然不存在的路径即可。
    #[cfg(test)]
    fn cache_path(&self) -> PathBuf {
        std::env::temp_dir().join(format!("voicefox-test-{}-absent.json", self.file_name))
    }

    fn cache(&self) -> &RwLock<Vec<SyncCollection>> {
        self.lock.get_or_init(|| {
            let loaded = load_from(&self.cache_path());
            if !loaded.is_empty() {
                // 从磁盘恢复的内容也算「有新数据」，否则第一次访问时页面缓存会
                // 认为代次仍是 0，从而把已落盘的歌单当成空的。
                self.generation.fetch_add(1, Ordering::Relaxed);
            }
            RwLock::new(loaded)
        })
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

static NETEASE_SLOT: Slot = Slot::new("netease_collections.json");
static TX_SLOT: Slot = Slot::new("qq_collections.json");

/// 平台 → 镜像槽位；未接入镜像的平台没有槽位。
fn slot(source: SourceId) -> Option<&'static Slot> {
    match source {
        SourceId::Wy => Some(&NETEASE_SLOT),
        SourceId::Tx => Some(&TX_SLOT),
        _ => None,
    }
}

/// 已接入镜像的平台（顺序即界面上展示的顺序）。
const MIRRORED_SOURCES: [SourceId; 2] = [SourceId::Wy, SourceId::Tx];

/// 远程缓存文件的总大小（字节）；文件不存在时为 0。
pub fn cache_file_size() -> u64 {
    MIRRORED_SOURCES
        .iter()
        .filter_map(|source| slot(*source))
        .map(|s| {
            std::fs::metadata(s.cache_path())
                .map(|meta| meta.len())
                .unwrap_or(0)
        })
        .sum()
}

/// 删除磁盘上的远程缓存文件并清空内存镜像。
///
/// 缓存随时可以在下次同步时重建，因此清理是安全的；正在展示的
/// 「远程歌单」窗口内容会随 generation 失效而刷新。
pub fn clear() -> std::io::Result<()> {
    for source in MIRRORED_SOURCES {
        let Some(s) = slot(source) else {
            continue;
        };
        if let Some(cache) = s.lock.get() {
            cache
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        match std::fs::remove_file(s.cache_path()) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// 读缓存文件。文件缺失或 JSON 损坏都当空处理：一份缓存坏了不能拖垮启动。
fn load_from(path: &Path) -> Vec<SyncCollection> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<SyncCollection>>(&raw) {
        Ok(collections) => collections,
        Err(error) => {
            tracing::warn!("远程歌单缓存解析失败({}): {error}", path.display());
            Vec::new()
        }
    }
}

/// 原子写缓存文件：先写临时文件再 rename，避免写到一半被中断留下半个 JSON。
fn save_to(path: &Path, collections: &[SyncCollection]) {
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("创建远程歌单缓存目录失败: {error}");
        return;
    }
    let Ok(body) = serde_json::to_vec(collections) else {
        tracing::warn!("序列化远程歌单缓存失败");
        return;
    };
    let temp = path.with_extension("json.tmp");
    if let Err(error) = std::fs::write(&temp, &body) {
        tracing::warn!("写入远程歌单缓存失败: {error}");
        return;
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        tracing::warn!("替换远程歌单缓存失败: {error}");
        let _ = std::fs::remove_file(&temp);
    }
}

/// 按 id 增量合并（远端已删除的歌单不会被移除）。
///
/// 用于远端拉取存在局部失败的场合：成功的那部分照常更新，失败的那部分保留
/// 缓存里的旧数据。
pub fn merge(source: SourceId, collections: Vec<SyncCollection>) {
    let Some(s) = slot(source) else {
        return;
    };
    let Ok(mut guard) = s.cache().write() else {
        return;
    };
    for collection in collections {
        match guard.iter_mut().find(|item| item.id == collection.id) {
            Some(existing) => *existing = collection,
            None => guard.push(collection),
        }
    }
    save_to(&s.cache_path(), &guard);
    s.generation.fetch_add(1, Ordering::Relaxed);
}

/// 整体替换。
///
/// 只有确认远端结果是完整的时候才允许调用（见 `SyncCollectionSet::is_complete`），
/// 这样远端删除歌单才会从缓存里消失。
pub fn replace(source: SourceId, collections: Vec<SyncCollection>) {
    let Some(s) = slot(source) else {
        return;
    };
    if let Ok(mut guard) = s.cache().write() {
        *guard = collections;
        save_to(&s.cache_path(), &guard);
        s.generation.fetch_add(1, Ordering::Relaxed);
    }
}

/// 平台镜像的全部集合（深拷贝快照）。
pub fn all(source: SourceId) -> Vec<SyncCollection> {
    with_source(source, |collections| collections.to_vec()).unwrap_or_default()
}

/// 只读借用指定平台的镜像，避免渲染路径每帧深拷贝整个缓存。
pub fn with_source<R>(source: SourceId, read: impl FnOnce(&[SyncCollection]) -> R) -> Option<R> {
    let s = slot(source)?;
    let guard = s
        .cache()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Some(read(&guard))
}

/// 所有平台镜像合集（网易云在前），供「我的歌单」等聚合展示使用。
pub fn all_mirrored() -> Vec<SyncCollection> {
    MIRRORED_SOURCES
        .iter()
        .flat_map(|source| all(*source))
        .collect()
}

/// 任一镜像有更新就递增的组合代次：收藏页等聚合视图用它当缓存版本。
pub fn generation() -> u64 {
    MIRRORED_SOURCES
        .iter()
        .filter_map(|source| slot(*source))
        .map(Slot::generation)
        .sum()
}

/// 全部镜像的 `(总数, 普通歌单数, 红心歌单数)`，供设置页渲染使用（不拷贝歌曲）。
pub fn summary_counts() -> (usize, usize, usize) {
    let mut total = 0;
    let mut normal = 0;
    let mut favorites = 0;
    for source in MIRRORED_SOURCES {
        with_source(source, |collections| {
            for collection in collections {
                total += 1;
                if collection.kind == SyncCollectionKind::Favorites {
                    favorites += 1;
                } else {
                    normal += 1;
                }
            }
        });
    }
    (total, normal, favorites)
}

/// 指定平台红心歌单的歌曲；没有缓存时返回空。
pub fn favorites_songs(source: SourceId) -> Vec<lx_core::model::song::SongInfo> {
    with_source(source, |collections| {
        collections
            .iter()
            .find(|item| item.kind == SyncCollectionKind::Favorites)
            .map(|item| item.songs.clone())
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// 远端歌单 id → 集合；只在指定平台的镜像里找。
pub fn playlist(source: SourceId, id: &str) -> Option<SyncCollection> {
    with_source(source, |collections| {
        collections.iter().find(|item| item.id == id).cloned()
    })
    .unwrap_or(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lx_core::model::song::SongInfo;

    /// 这些测试共用同一组进程级缓存，而 `cargo test` 默认多线程并行跑，
    /// 不加锁会互相覆盖（一个测试的 replace 会抹掉另一个测试刚写的数据）。
    fn lock_cache() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn collection(
        source: SourceId,
        kind: SyncCollectionKind,
        id: &str,
        name: &str,
        songs: usize,
    ) -> SyncCollection {
        SyncCollection {
            kind,
            id: id.into(),
            name: name.into(),
            source,
            songs: (0..songs)
                .map(|index| {
                    SongInfo::new(
                        index.to_string(),
                        source,
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
        // 槽位是进程级静态：先清空，断言才不依赖其它测试的遗留状态。
        clear().unwrap();
        replace(
            SourceId::Wy,
            vec![collection(
                SourceId::Wy,
                SyncCollectionKind::Playlist,
                "p1",
                "Test",
                1,
            )],
        );
        assert_eq!(playlist(SourceId::Wy, "p1").unwrap().songs[0].name, "Song0");
        // QQ 槽位互不干扰
        assert!(playlist(SourceId::Tx, "p1").is_none());
    }

    #[test]
    fn qq_slot_is_independent_from_netease() {
        let _guard = lock_cache();
        clear().unwrap();
        replace(
            SourceId::Wy,
            vec![collection(
                SourceId::Wy,
                SyncCollectionKind::Playlist,
                "p1",
                "网易歌单",
                2,
            )],
        );
        replace(
            SourceId::Tx,
            vec![collection(
                SourceId::Tx,
                SyncCollectionKind::Playlist,
                "p1",
                "QQ歌单",
                5,
            )],
        );

        assert_eq!(playlist(SourceId::Wy, "p1").unwrap().name, "网易歌单");
        assert_eq!(playlist(SourceId::Tx, "p1").unwrap().name, "QQ歌单");
        assert_eq!(playlist(SourceId::Tx, "p1").unwrap().songs.len(), 5);
        // 聚合视图两个平台的都看得到，顺序稳定（网易云在前）
        let all = all_mirrored();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].source, SourceId::Wy);
        assert_eq!(all[1].source, SourceId::Tx);
    }

    #[test]
    fn merge_keeps_collections_that_are_absent_from_the_update() {
        let _guard = lock_cache();
        replace(
            SourceId::Wy,
            vec![
                collection(SourceId::Wy, SyncCollectionKind::Playlist, "p1", "Old", 3),
                collection(
                    SourceId::Wy,
                    SyncCollectionKind::Playlist,
                    "p2",
                    "Untouched",
                    2,
                ),
            ],
        );
        // 只更新 p1：p2 必须原样保留，否则局部失败就等于丢歌单。
        merge(
            SourceId::Wy,
            vec![collection(
                SourceId::Wy,
                SyncCollectionKind::Playlist,
                "p1",
                "New",
                5,
            )],
        );

        let p1 = playlist(SourceId::Wy, "p1").expect("p1");
        assert_eq!(p1.name, "New");
        assert_eq!(p1.songs.len(), 5);
        let p2 = playlist(SourceId::Wy, "p2").expect("p2");
        assert_eq!(p2.songs.len(), 2);
    }

    #[test]
    fn replace_removes_collections_gone_from_remote() {
        let _guard = lock_cache();
        replace(
            SourceId::Wy,
            vec![
                collection(SourceId::Wy, SyncCollectionKind::Playlist, "p1", "A", 1),
                collection(SourceId::Wy, SyncCollectionKind::Playlist, "p2", "B", 1),
            ],
        );
        replace(
            SourceId::Wy,
            vec![collection(
                SourceId::Wy,
                SyncCollectionKind::Playlist,
                "p1",
                "A",
                1,
            )],
        );
        assert!(playlist(SourceId::Wy, "p2").is_none());
    }

    #[test]
    fn summary_counts_split_across_platforms() {
        let _guard = lock_cache();
        clear().unwrap();
        replace(
            SourceId::Wy,
            vec![
                collection(SourceId::Wy, SyncCollectionKind::Playlist, "p1", "A", 1),
                collection(SourceId::Wy, SyncCollectionKind::Favorites, "f1", "红心", 4),
            ],
        );
        replace(
            SourceId::Tx,
            vec![collection(
                SourceId::Tx,
                SyncCollectionKind::Playlist,
                "q1",
                "B",
                2,
            )],
        );
        assert_eq!(summary_counts(), (3, 2, 1));

        // 清理后各槽位都清空
        clear().unwrap();
        assert_eq!(summary_counts(), (0, 0, 0));
    }

    #[test]
    fn favorites_songs_reads_only_the_requested_platform() {
        let _guard = lock_cache();
        clear().unwrap();
        replace(
            SourceId::Wy,
            vec![
                collection(SourceId::Wy, SyncCollectionKind::Playlist, "p1", "A", 9),
                collection(
                    SourceId::Wy,
                    SyncCollectionKind::Favorites,
                    "f1",
                    "我喜欢的音乐",
                    4,
                ),
            ],
        );
        replace(
            SourceId::Tx,
            vec![collection(
                SourceId::Tx,
                SyncCollectionKind::Favorites,
                "201",
                "我喜欢",
                6,
            )],
        );
        assert_eq!(favorites_songs(SourceId::Wy).len(), 4);
        assert_eq!(favorites_songs(SourceId::Tx).len(), 6);
        assert_eq!(favorites_songs(SourceId::Kw).len(), 0);
    }

    #[test]
    fn collections_survive_a_json_round_trip() {
        // 落盘依赖 SyncCollection（含 SongInfo）的 serde 往返；这里直接验证
        // 序列化再反序列化不会丢字段，否则重启后缓存会变成空歌单。
        let original = vec![
            collection(
                SourceId::Wy,
                SyncCollectionKind::Playlist,
                "p1",
                "歌单 A",
                3,
            ),
            collection(
                SourceId::Wy,
                SyncCollectionKind::Favorites,
                "f1",
                "我喜欢的音乐",
                2,
            ),
        ];
        let encoded = serde_json::to_vec(&original).expect("serialize");
        let decoded: Vec<SyncCollection> = serde_json::from_slice(&encoded).expect("deserialize");

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].name, "歌单 A");
        assert_eq!(decoded[0].songs.len(), 3);
        assert_eq!(decoded[0].songs[0].source, SourceId::Wy);
        assert_eq!(decoded[1].kind, SyncCollectionKind::Favorites);
        assert_eq!(decoded[1].songs.len(), 2);
    }

    #[test]
    fn cache_file_round_trips_and_survives_corruption() {
        // 直接测真实读写路径：槽位的 save/load 在 cfg(test) 下指向必然不
        // 存在的路径，所以这里用临时目录验证落盘本身。
        let dir = std::env::temp_dir().join(format!(
            "voicefox-netease-cache-test-{}",
            std::process::id()
        ));
        let path = dir.join("netease_collections.json");
        let original = vec![collection(
            SourceId::Wy,
            SyncCollectionKind::Playlist,
            "p1",
            "歌单 A",
            1,
        )];

        save_to(&path, &original);
        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "歌单 A");
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
