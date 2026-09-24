use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::storage::{CustomPlaylist, Storage};

use lx_core::model::source::SourceId;
use lx_core::sync::{SyncCollection, SyncCollectionKind, SyncEngine, SyncOptions};
use lx_source::sync::provider;

#[derive(Debug, Default, Clone)]
pub struct NeteaseSyncReport {
    pub playlists_created: usize,
    pub playlists_pulled: usize,
    pub songs_pulled: usize,
    pub songs_pushed: usize,
    pub favorites_pulled: usize,
    pub favorites_pushed: usize,
    pub unmatched: usize,
    pub failed: usize,
}
#[derive(Debug, Clone, Default)]
pub struct PlaylistDiff {
    pub name: String,
    pub upload: usize,
    pub download: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub mapping: String,
}
#[derive(Debug, Clone, Default)]
pub struct NeteaseSyncPreview {
    pub playlists: Vec<PlaylistDiff>,
    pub upload: usize,
    pub download: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub favorites_upload: usize,
    pub favorites_download: usize,
    pub remote_only: usize,
    pub local_only: usize,
    pub unmatched_songs: Vec<(String, String)>,
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
fn local_collection(p: &CustomPlaylist) -> SyncCollection {
    SyncCollection {
        kind: SyncCollectionKind::Playlist,
        id: p.id.clone(),
        name: p.name.clone(),
        source: SourceId::Local,
        songs: p.songs.clone(),
    }
}

pub async fn preview_source(
    storage: &Storage,
    source: SourceId,
) -> Result<NeteaseSyncPreview, String> {
    if source == SourceId::Wy {
        let _ = lx_source::wy::login::refresh().await;
    }
    let provider = provider(source).ok_or_else(|| "同步适配器不可用".to_string())?;
    let remote = provider
        .list_collections(SyncCollectionKind::Playlist)
        .await
        .map_err(|e| e.to_string())?;
    let mut remote_by_id = std::collections::HashMap::new();
    let mut remote_by_name = std::collections::HashMap::new();
    for p in remote {
        remote_by_name.insert(p.name.clone(), p.clone());
        remote_by_id.insert(p.id.clone(), p);
    }
    let locals = storage
        .custom_playlist_summaries()
        .into_iter()
        .filter_map(|s| storage.custom_playlist(&s.id))
        .collect::<Vec<_>>();
    let mut out = NeteaseSyncPreview::default();
    for local in &locals {
        let mapped = storage.sync_playlist_mapping(source, &local.id);
        let target = mapped
            .as_deref()
            .and_then(|id| remote_by_id.get(id))
            .or_else(|| remote_by_name.get(&local.name));
        match target {
            Some(target) => {
                let a = SyncEngine::plan(
                    local_collection(local),
                    target.clone(),
                    &SyncOptions::default(),
                )
                .map_err(|e| e.to_string())?;
                let b = SyncEngine::plan(
                    target.clone(),
                    local_collection(local),
                    &SyncOptions::default(),
                )
                .map_err(|e| e.to_string())?;
                let upload = a.additions_count();
                let download = b.additions_count();
                let unmatched = a.unmatched.len() + b.unmatched.len();
                out.upload += upload;
                out.download += download;
                out.matched += a.matched_count();
                out.unmatched += unmatched;
                for song in a.unmatched.iter().chain(b.unmatched.iter()) {
                    if out.unmatched_songs.len() < 100 {
                        out.unmatched_songs
                            .push((song.name.clone(), song.singer.clone()));
                    }
                }
                out.playlists.push(PlaylistDiff {
                    name: local.name.clone(),
                    upload,
                    download,
                    matched: a.matched_count(),
                    unmatched,
                    mapping: if mapped.is_some() {
                        "已绑定".into()
                    } else {
                        "名称匹配".into()
                    },
                });
            }
            None => {
                out.local_only += 1;
                out.upload += local.songs.len();
                out.playlists.push(PlaylistDiff {
                    name: local.name.clone(),
                    upload: local.songs.len(),
                    ..Default::default()
                });
            }
        }
    }
    for remote in remote_by_name.values() {
        if remote.name != "我喜欢"
            && !locals.iter().any(|p| {
                p.name == remote.name
                    || storage.sync_playlist_mapping(source, &p.id).as_deref() == Some(&remote.id)
            })
        {
            out.remote_only += 1;
            out.download += remote.songs.len();
        }
    }
    let rf = provider
        .list_collections(SyncCollectionKind::Favorites)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "网易云未找到“我喜欢”".to_string())?;
    let lf = SyncCollection {
        kind: SyncCollectionKind::Favorites,
        id: "local:favorites".into(),
        name: "本地收藏".into(),
        source: SourceId::Local,
        songs: storage.load_favorites(),
    };
    let a = SyncEngine::plan(lf.clone(), rf.clone(), &SyncOptions::default())
        .map_err(|e| e.to_string())?;
    let b = SyncEngine::plan(rf, lf, &SyncOptions::default()).map_err(|e| e.to_string())?;
    out.favorites_upload = a.additions_count();
    out.favorites_download = b.additions_count();
    out.unmatched += a.unmatched.len() + b.unmatched.len();
    for song in a.unmatched.iter().chain(b.unmatched.iter()) {
        if out.unmatched_songs.len() < 100 {
            out.unmatched_songs
                .push((song.name.clone(), song.singer.clone()));
        }
    }
    Ok(out)
}
pub async fn sync_source_with_control(
    storage: &Storage,
    source: SourceId,
    control: &SyncControl,
) -> Result<NeteaseSyncReport, String> {
    let provider = provider(source).ok_or_else(|| "同步适配器不可用".to_string())?;
    let remote = provider
        .list_collections(SyncCollectionKind::Playlist)
        .await
        .map_err(|e| e.to_string())?;
    let mut by_id = std::collections::HashMap::new();
    let mut by_name = std::collections::HashMap::new();
    for p in remote {
        by_name.insert(p.name.clone(), p.clone());
        by_id.insert(p.id.clone(), p);
    }
    let locals = storage
        .custom_playlist_summaries()
        .into_iter()
        .filter_map(|s| storage.custom_playlist(&s.id))
        .collect::<Vec<_>>();
    control.set_total(
        locals.iter().map(|p| p.songs.len()).sum::<usize>()
            + by_name
                .values()
                .filter(|p| p.name != "我喜欢")
                .map(|p| p.songs.len())
                .sum::<usize>(),
    );
    let mut report = NeteaseSyncReport::default();
    for remote in by_name.values() {
        if control.is_cancelled() {
            return Err("同步已取消".into());
        }
        if remote.name == "我喜欢" {
            continue;
        }
        let local = locals
            .iter()
            .find(|p| storage.sync_playlist_mapping(source, &p.id).as_deref() == Some(&remote.id))
            .cloned()
            .or_else(|| locals.iter().find(|p| p.name == remote.name).cloned());
        let local = match local {
            Some(p) => p,
            None => {
                report.playlists_created += 1;
                storage
                    .create_custom_playlist(&remote.name)
                    .map_err(|e| e.to_string())?
            }
        };
        let (added, _) = storage
            .add_songs_to_custom_playlist(&local.id, &remote.songs)
            .map_err(|e| e.to_string())?;
        report.songs_pulled += added;
        report.playlists_pulled += 1;
        storage
            .set_sync_playlist_mapping(source, &local.id, &remote.id)
            .map_err(|e| e.to_string())?;
        for _ in &remote.songs {
            control.inc();
        }
    }
    for local in locals {
        if control.is_cancelled() {
            return Err("同步已取消".into());
        }
        let target = storage
            .sync_playlist_mapping(source, &local.id)
            .and_then(|id| by_id.get(&id).cloned())
            .or_else(|| by_name.get(&local.name).cloned());
        let target = match target {
            Some(p) => p,
            None => {
                report.playlists_created += 1;
                provider
                    .create_collection(SyncCollectionKind::Playlist, &local.name)
                    .await
                    .map_err(|e| e.to_string())?
            }
        };
        storage
            .set_sync_playlist_mapping(source, &local.id, &target.id)
            .map_err(|e| e.to_string())?;
        let plan = SyncEngine::plan(local_collection(&local), target, &SyncOptions::default())
            .map_err(|e| e.to_string())?;
        report.unmatched += plan.unmatched.len();
        let result = SyncEngine::execute(provider.as_ref(), &plan, &SyncOptions::default(), false)
            .await
            .map_err(|e| e.to_string())?;
        report.songs_pushed += result.added;
        report.failed += result.failed.len();
        for _ in &local.songs {
            control.inc();
        }
    }
    let rf = provider
        .list_collections(SyncCollectionKind::Favorites)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "网易云未找到“我喜欢”".to_string())?;
    for song in &rf.songs {
        if control.is_cancelled() {
            return Err("同步已取消".into());
        }
        if storage.add_favorite(song) {
            report.favorites_pulled += 1
        }
        control.inc();
    }
    let lf = SyncCollection {
        kind: SyncCollectionKind::Favorites,
        id: "local:favorites".into(),
        name: "本地收藏".into(),
        source: SourceId::Local,
        songs: storage.load_favorites(),
    };
    let plan = SyncEngine::plan(lf, rf, &SyncOptions::default()).map_err(|e| e.to_string())?;
    report.unmatched += plan.unmatched.len();
    report.favorites_pushed =
        SyncEngine::execute(provider.as_ref(), &plan, &SyncOptions::default(), false)
            .await
            .map_err(|e| e.to_string())?
            .added;
    Ok(report)
}
