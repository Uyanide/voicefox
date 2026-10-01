use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::storage::Storage;

use lx_core::model::source::SourceId;
use lx_core::sync::{SyncCollection, SyncCollectionKind, SyncCollectionSet};
use lx_source::sync::provider;

/// 一次远程集合刷新的结果。
///
/// 网易云歌单现在是「只读镜像进远程缓存」，不再往本地建歌单、也不再往远端
/// 推送，所以这里只描述读到了什么、相对缓存变了多少。
#[derive(Debug, Default, Clone)]
pub struct NeteaseSyncReport {
    pub playlists: usize,
    pub favorites: usize,
    pub songs: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// 本次未取到、保留旧缓存的歌单：(名称, 原因)。
    pub failed: Vec<(String, String)>,
}

/// 预览里的单个歌单。
#[derive(Debug, Clone, Default)]
pub struct PlaylistDiff {
    pub name: String,
    /// "歌单" 或 "红心"。
    pub kind: String,
    /// 缓存里已有的歌曲数（没有则为 0）。
    pub cached: usize,
    /// 本次远端返回的歌曲数。
    pub remote: usize,
    /// "新增" / "更新" / "相同"。
    pub status: String,
}

#[derive(Debug, Clone, Default)]
pub struct NeteaseSyncPreview {
    pub playlists: usize,
    pub favorites: usize,
    pub songs: usize,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    /// 本次未取到、将保留旧缓存的歌单：(名称, 原因)。
    pub failed: Vec<(String, String)>,
    pub rows: Vec<PlaylistDiff>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncControl {
    pub cancelled: Arc<AtomicBool>,
    pub done: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
}
impl SyncControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn set_total(&self, n: usize) {
        self.total.store(n, Ordering::Release);
    }
    pub fn inc(&self) {
        self.done.fetch_add(1, Ordering::AcqRel);
    }
    pub fn progress(&self) -> (usize, usize) {
        (
            self.done.load(Ordering::Acquire),
            self.total.load(Ordering::Acquire),
        )
    }
}

/// 远端集合相对当前缓存的差异统计。
fn diff_against_cache(remote: &[SyncCollection]) -> (usize, usize, usize) {
    crate::remote_cache::with_netease(|cached| {
        let cached_by_id: HashMap<&str, &SyncCollection> =
            cached.iter().map(|item| (item.id.as_str(), item)).collect();
        let mut added = 0;
        let mut updated = 0;
        for collection in remote {
            match cached_by_id.get(collection.id.as_str()) {
                None => added += 1,
                Some(existing)
                    if existing.songs.len() != collection.songs.len()
                        || existing.name != collection.name =>
                {
                    updated += 1
                }
                Some(_) => {}
            }
        }
        let removed = cached
            .iter()
            .filter(|existing| !remote.iter().any(|collection| collection.id == existing.id))
            .count();
        (added, updated, removed)
    })
}

fn kind_label(kind: SyncCollectionKind) -> &'static str {
    match kind {
        SyncCollectionKind::Favorites => "红心",
        SyncCollectionKind::Playlist => "歌单",
    }
}

fn diff_rows(set: &SyncCollectionSet) -> Vec<PlaylistDiff> {
    crate::remote_cache::with_netease(|cached| {
        set.playlists
            .iter()
            .chain(set.favorites.iter())
            .map(|collection| {
                let existing = cached.iter().find(|item| item.id == collection.id);
                let status = match existing {
                    None => "新增",
                    Some(item)
                        if item.songs.len() != collection.songs.len()
                            || item.name != collection.name =>
                    {
                        "更新"
                    }
                    Some(_) => "相同",
                };
                PlaylistDiff {
                    name: collection.name.clone(),
                    kind: kind_label(collection.kind).to_string(),
                    cached: existing.map(|item| item.songs.len()).unwrap_or_default(),
                    remote: collection.songs.len(),
                    status: status.to_string(),
                }
            })
            .collect()
    })
}

/// 只读预览：从远端读一次，和缓存比对，说明「确认」之后会发生什么。
///
/// 注意这里**不写任何远端数据**，也不再像旧版那样声称会把本地歌单上传到网易云。
pub async fn preview_source(
    _storage: &Storage,
    source: SourceId,
) -> Result<NeteaseSyncPreview, String> {
    let provider = provider(source).ok_or_else(|| "同步适配器不可用".to_string())?;
    if source != SourceId::Wy {
        return Err(format!(
            "{}暂不支持远程歌单同步，当前仅支持网易云音乐",
            source.display_name()
        ));
    }
    // 会话有效性预检：未登录与已失效分别给出可操作的提示，不必等几十次
    // 请求跑完。网络错误无法判定失效（`refresh` 返回 Err），交给后续接口
    // 报真正的错误。
    if !lx_source::wy::session::is_logged_in() {
        return Err("尚未登录网易云，请先在设置（8）→ 账号与扫码 扫码登录".into());
    }
    if let Ok(false) = lx_source::wy::login::refresh().await {
        return Err("网易云登录已失效，请重新扫码登录".into());
    }
    let set = provider.collect_all().await.map_err(|e| e.to_string())?;
    let songs = set
        .playlists
        .iter()
        .chain(set.favorites.iter())
        .map(|collection| collection.songs.len())
        .sum();
    let (added, updated, removed) = diff_against_cache(&set.playlists);
    Ok(NeteaseSyncPreview {
        playlists: set.playlists.len(),
        favorites: set.favorites.len(),
        songs,
        added,
        updated,
        removed,
        failed: set.failed.clone(),
        rows: diff_rows(&set),
    })
}

/// 执行刷新：把远端歌单写入远程缓存。
///
/// 完整性约定：只有全部歌单都拉取成功时才整体替换缓存（这样远端删掉的歌单会
/// 跟着消失）；只要有一个歌单失败就退化为按 id 增量合并，失败歌单保留缓存里的
/// 旧数据 —— 绝不能因为一次频控就把用户已缓存的歌单清空。
pub async fn sync_source_with_control(
    _storage: &Storage,
    source: SourceId,
    control: &SyncControl,
) -> Result<NeteaseSyncReport, String> {
    let provider = provider(source).ok_or_else(|| "同步适配器不可用".to_string())?;
    if source != SourceId::Wy {
        return Err(format!(
            "{}暂不支持远程歌单同步，当前仅支持网易云音乐",
            source.display_name()
        ));
    }

    let set = provider.collect_all().await.map_err(|e| e.to_string())?;
    let complete = set.is_complete();
    let failed = set.failed.clone();
    let playlists = set.playlists.len();
    let favorites = set.favorites.len();
    let songs = set
        .playlists
        .iter()
        .chain(set.favorites.iter())
        .map(|collection| collection.songs.len())
        .sum();
    let (added, updated, removed) = diff_against_cache(&set.playlists);

    control.set_total(songs);
    if control.is_cancelled() {
        return Err("同步已取消".into());
    }

    let all = set.into_all();
    if complete {
        crate::remote_cache::replace_netease(all);
    } else {
        crate::remote_cache::merge_netease(all);
    }

    for collection in crate::remote_cache::all_netease() {
        if control.is_cancelled() {
            return Err("同步已取消".into());
        }
        for _ in &collection.songs {
            control.inc();
        }
    }

    Ok(NeteaseSyncReport {
        playlists,
        favorites,
        songs,
        added,
        // 增量合并时远端已删除的歌单不会被移除，所以不能报告 removed。
        updated,
        removed: if complete { removed } else { 0 },
        failed,
    })
}
