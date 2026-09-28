mod matcher;
pub mod model;

use crate::model::song::SongInfo;
use crate::model::source::SourceId;
use async_trait::async_trait;
use thiserror::Error;

pub use matcher::match_songs;
pub use model::*;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("未找到同步对象: {0}")]
    CollectionNotFound(String),
    #[error("音源未登录: {0}")]
    NotLoggedIn(String),
    #[error("音源不支持同步写入: {0}")]
    WriteUnsupported(String),
    #[error("同步请求失败: {0}")]
    Provider(String),
    #[error("同步参数无效: {0}")]
    InvalidOptions(String),
}

/// 一次远端集合拉取的结果。
///
/// `failed` 记录逐个集合拉取失败的原因。调用方**必须**据此决定能否整体替换
/// 本地缓存：只有 `is_complete()` 为真时才允许覆盖/删除旧数据，否则只能按 id
/// 增量合并，不然一次频控就会把用户已有的歌单静默清空。
#[derive(Debug, Clone, Default)]
pub struct SyncCollectionSet {
    pub playlists: Vec<SyncCollection>,
    pub favorites: Vec<SyncCollection>,
    /// 拉取失败的集合：(名称, 原因)。
    pub failed: Vec<(String, String)>,
}

impl SyncCollectionSet {
    /// 是否所有远端集合都成功拉取（可以安全地整体替换旧缓存）。
    pub fn is_complete(&self) -> bool {
        self.failed.is_empty()
    }

    /// 普通歌单与收藏集合的全集。
    pub fn into_all(self) -> Vec<SyncCollection> {
        let mut all = self.playlists;
        all.extend(self.favorites);
        all
    }
}

#[async_trait]
pub trait SyncProvider: Send + Sync {
    fn source_id(&self) -> SourceId;
    fn source_name(&self) -> &str;
    async fn list_collections(
        &self,
        kind: SyncCollectionKind,
    ) -> Result<Vec<SyncCollection>, SyncError>;
    async fn get_collection(
        &self,
        kind: SyncCollectionKind,
        id: &str,
    ) -> Result<SyncCollection, SyncError>;
    async fn create_collection(
        &self,
        kind: SyncCollectionKind,
        name: &str,
    ) -> Result<SyncCollection, SyncError>;
    async fn add_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError>;
    async fn remove_songs(
        &self,
        collection: &SyncCollection,
        songs: &[SongInfo],
    ) -> Result<usize, SyncError>;
    async fn search_song(&self, song: &SongInfo) -> Result<Vec<SongInfo>, SyncError>;
    /// 一次性取出全部远端集合（普通歌单 + 收藏），并带上逐个集合的失败信息。
    ///
    /// 默认实现退化为两次 `list_collections` 且不报告局部失败。像网易云这种
    /// 「先取一份用户歌单再按 kind 过滤」的音源应当覆写：同一份远端数据拉两遍
    /// 既慢，也会把网易云打到频控（HTTP 200 + `code=405 操作频繁`）上。
    async fn collect_all(&self) -> Result<SyncCollectionSet, SyncError> {
        Ok(SyncCollectionSet {
            playlists: self.list_collections(SyncCollectionKind::Playlist).await?,
            favorites: self.list_collections(SyncCollectionKind::Favorites).await?,
            failed: Vec::new(),
        })
    }
    async fn supports_write(&self, _kind: SyncCollectionKind) -> bool {
        true
    }
}

pub struct SyncEngine;
impl SyncEngine {
    pub fn plan(
        source: SyncCollection,
        target: SyncCollection,
        options: &SyncOptions,
    ) -> Result<SyncPlan, SyncError> {
        if source.source == target.source && source.id == target.id {
            return Err(SyncError::InvalidOptions(
                "源歌单和目标歌单不能是同一个对象".into(),
            ));
        }
        let matches = match_songs(
            &source.songs,
            &target.songs,
            options.match_duration_tolerance_ms,
            options.fuzzy_threshold,
        );
        let additions = matches
            .iter()
            .filter(|m| m.target.is_none())
            .map(|m| m.source.clone())
            .collect::<Vec<_>>();
        let removals = if matches!(options.policy, SyncPolicy::Mirror) {
            target
                .songs
                .iter()
                .filter(|candidate| {
                    !matches.iter().any(|m| {
                        m.target
                            .as_ref()
                            .is_some_and(|t| t.id == candidate.id && t.source == candidate.source)
                    })
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        Ok(SyncPlan {
            source,
            target,
            matches,
            additions: additions.clone(),
            removals,
            unmatched: additions,
        })
    }

    pub async fn execute<P: SyncProvider + ?Sized>(
        provider: &P,
        plan: &SyncPlan,
        options: &SyncOptions,
        allow_removals: bool,
    ) -> Result<SyncReport, SyncError> {
        if !provider.supports_write(plan.target.kind).await {
            return Err(SyncError::WriteUnsupported(provider.source_name().into()));
        }
        let mut added = 0;
        let mut failed = Vec::new();
        for chunk in plan.additions.chunks(options.batch_size.max(1)) {
            let mut resolved = Vec::new();
            for song in chunk {
                let candidates = provider.search_song(song).await?;
                if let Some(best) = best_candidate(song, &candidates, options) {
                    resolved.push(best.clone());
                } else {
                    failed.push(SyncFailure {
                        song: song.name.clone(),
                        artist: song.singer.clone(),
                        reason: "目标音源找不到可匹配歌曲".into(),
                    });
                }
            }
            if !resolved.is_empty() {
                added += provider.add_songs(&plan.target, &resolved).await?;
            }
        }
        let removed = if allow_removals
            && matches!(options.policy, SyncPolicy::Mirror)
            && !plan.removals.is_empty()
        {
            provider.remove_songs(&plan.target, &plan.removals).await?
        } else {
            0
        };
        Ok(SyncReport {
            source: plan.source.source,
            target: plan.target.source,
            collection_name: plan.target.name.clone(),
            added,
            removed,
            already_present: plan.matches.len().saturating_sub(plan.additions.len()),
            unmatched: failed.len(),
            failed,
        })
    }
}
fn best_candidate<'a>(
    source: &SongInfo,
    candidates: &'a [SongInfo],
    options: &SyncOptions,
) -> Option<&'a SongInfo> {
    let matched = match_songs(
        std::slice::from_ref(source),
        candidates,
        options.match_duration_tolerance_ms,
        options.fuzzy_threshold,
    )
    .into_iter()
    .next()?;
    let target = matched.target?;
    candidates
        .iter()
        .find(|c| c.id == target.id && c.source == target.source)
}

/// 为双向同步生成两个独立计划；执行时仍逐向确认，避免一次操作同时修改两个平台。
impl SyncEngine {
    pub fn plan_bidirectional(
        left: SyncCollection,
        right: SyncCollection,
        options: &SyncOptions,
    ) -> Result<(SyncPlan, SyncPlan), SyncError> {
        let mut forward = options.clone();
        forward.direction = SyncDirection::SourceToTarget;
        let mut backward = options.clone();
        backward.direction = SyncDirection::TargetToSource;
        Ok((
            Self::plan(left.clone(), right.clone(), &forward)?,
            Self::plan(right, left, &backward)?,
        ))
    }
}

impl SyncPlan {
    pub fn additions_count(&self) -> usize {
        self.additions.len()
    }
    pub fn removals_count(&self) -> usize {
        self.removals.len()
    }
    pub fn matched_count(&self) -> usize {
        self.matches
            .iter()
            .filter(|item| item.target.is_some())
            .count()
    }
}
