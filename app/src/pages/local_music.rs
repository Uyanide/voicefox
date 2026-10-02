use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::AppAction;
use lx_core::model::song::SongInfo;
use ratatui::layout::{Position, Rect};

use crate::context::AppContext;
use crate::pages::components::context_menu::MenuHitSource;
use crate::pages::components::hit_test::PanelRows;
use crate::pages::components::list_filter::ListFilter;
use crate::pages::sort::{SortState, SortTarget, SortedListCache};

/// 排序 + 过滤后的本地歌曲视图：键盘、鼠标和渲染共用同一份下标映射，
/// 保证"光标所在行 = 双击播放的歌"在任何过滤状态下都一致。
///
/// 与收藏页的 `filtered_song_indices`、历史页的 `HistoryIndices` 同一模式：
/// query 为空时零分配恒等映射，否则只保存命中下标，不深拷贝歌曲。
pub enum LocalSongView<'a> {
    Identity(&'a [SongInfo]),
    Filtered(&'a [SongInfo], Vec<usize>),
}

impl<'a> LocalSongView<'a> {
    pub fn build(all: &'a [SongInfo], query: &str) -> Self {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Self::Identity(all);
        }
        let indices = all
            .iter()
            .enumerate()
            .filter(|(_, song)| {
                song.name.to_lowercase().contains(&query)
                    || song.singer.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        Self::Filtered(all, indices)
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Identity(songs) => songs.len(),
            Self::Filtered(_, indices) => indices.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 视图下标 → 歌曲引用（自动穿透过滤映射）。
    pub fn get(&self, index: usize) -> Option<&SongInfo> {
        match self {
            Self::Identity(songs) => songs.get(index),
            Self::Filtered(songs, indices) => songs.get(*indices.get(index)?),
        }
    }

    /// 具体化播放队列：恒等视图整表克隆，过滤视图只克隆命中歌曲。
    pub fn to_queue(&self) -> Vec<SongInfo> {
        match self {
            Self::Identity(songs) => songs.to_vec(),
            Self::Filtered(songs, indices) => indices.iter().map(|&i| songs[i].clone()).collect(),
        }
    }
}

pub fn handle_mouse(
    event: MouseEvent,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    cache: &mut SortedListCache,
    filter: &ListFilter,
    activate: bool,
) -> AppAction {
    use crate::pages::components::song_table::{self, ColumnResizeOutcome};

    let all_songs = sorted_local_songs(ctx, state, cache);
    let view = LocalSongView::build(all_songs, filter.query());

    // 与渲染共用行账本；过滤行可见性统一走 `ListFilter::is_visible()`。
    let rows = PanelRows::new(area, filter.is_visible(), false, true);
    let inner = rows.inner;
    match song_table::handle_column_resize(
        &mut state.column_resize,
        &mut state.columns,
        event,
        rows.header,
        inner,
    ) {
        ColumnResizeOutcome::Updated => return AppAction::None,
        ColumnResizeOutcome::Finished => {
            return AppAction::CommitColumnResize {
                page_key: state.page_key.to_string(),
                columns: state.columns.clone(),
            };
        }
        ColumnResizeOutcome::NotHandled => {}
    }

    let scroll_amount = ctx
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .ui
        .scroll_amount
        .max(1);
    let position = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::ScrollUp if rows.list.contains(position) => {
            state.selected = state.selected.saturating_sub(scroll_amount);
        }
        MouseEventKind::ScrollDown if rows.list.contains(position) => {
            state.selected = (state.selected + scroll_amount).min(view.len().saturating_sub(1));
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(index) = rows.index_at(position, state.scroll, view.len()) {
                state.selected = index;
                if activate {
                    return AppAction::PlaySong {
                        songs: view.to_queue(),
                        index,
                    };
                }
            }
        }
        _ => {}
    }
    AppAction::None
}

/// 歌曲表头所在的一行（供 main.rs 判定"表头右键 → 列菜单"）。
pub fn table_header_rect(area: Rect, filter: &ListFilter) -> Option<Rect> {
    PanelRows::new(area, filter.is_visible(), false, true).header
}

/// 自动列宽的测量样本，**无副作用**。
pub fn autofit_samples(ctx: &AppContext) -> Vec<SongInfo> {
    ctx.source_manager.local_source().all_songs()
}

pub fn context_song_at(
    source: MenuHitSource,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    cache: &mut SortedListCache,
    filter: &ListFilter,
) -> Option<(Vec<SongInfo>, usize)> {
    let all_songs = sorted_local_songs(ctx, state, cache);
    let view = LocalSongView::build(all_songs, filter.query());
    let rows = PanelRows::new(area, filter.is_visible(), false, true);
    let index = source.resolve_index(
        |event| {
            rows.index_at(
                Position::new(event.column, event.row),
                state.scroll,
                view.len(),
            )
        },
        state.selected,
        view.len(),
    )?;
    state.selected = index;
    Some((view.to_queue(), index))
}

/// 获取排序后的本地歌曲列表，结果按曲库代次 + 排序方式缓存，
/// 渲染路径不再每帧全量 clone + 排序。
pub fn sorted_local_songs<'a>(
    ctx: &AppContext,
    state: &SortState,
    cache: &'a mut SortedListCache,
) -> &'a [SongInfo] {
    let source = ctx.source_manager.local_source();
    cache.get_or_build(
        source.library_generation(),
        state.mode,
        SortTarget::Local,
        || source.all_songs(),
    )
}

#[cfg(test)]
mod tests {
    use crate::pages::components::hit_test::PanelRows;
    use ratatui::layout::{Position, Rect};

    /// 面板行账本：过滤行 → 表头 → 列表。命中与渲染共用同一份。
    fn rows(area: Rect, filter_visible: bool) -> PanelRows {
        PanelRows::new(area, filter_visible, false, true)
    }

    #[test]
    fn maps_visible_rows_to_scrolled_song_indices() {
        let area = Rect::new(10, 5, 80, 12);
        let rows = rows(area, false);

        assert_eq!(rows.index_at(Position::new(12, 7), 4, 20), Some(4));
        assert_eq!(rows.index_at(Position::new(12, 10), 4, 20), Some(7));
    }

    #[test]
    fn ignores_header_border_and_outside_rows() {
        let area = Rect::new(10, 5, 80, 12);
        let rows = rows(area, false);

        assert_eq!(rows.index_at(Position::new(12, 6), 0, 20), None, "表头行");
        assert_eq!(rows.index_at(Position::new(12, 5), 0, 20), None, "上边框");
        // inner = (11,6,78,10)：表头占 1 行，数据行 7..16 全部可用
        assert_eq!(rows.list.y, 7);
        assert_eq!(rows.list.bottom(), 16);
        assert_eq!(
            rows.index_at(Position::new(12, 15), 0, 20),
            Some(8),
            "面板内最后一行数据行应当可点（旧实现会白丢这一行）"
        );
        assert_eq!(
            rows.index_at(Position::new(12, 16), 0, 20),
            None,
            "下边框不属于列表"
        );
        assert_eq!(rows.index_at(Position::new(9, 7), 0, 20), None, "边框左侧");
    }

    #[test]
    fn filter_row_shifts_hit_area_down_by_one() {
        let area = Rect::new(10, 5, 80, 12);
        let filtered = rows(area, true);
        let plain = rows(area, false);

        // 过滤条可见时数据行整体下移一行
        assert_eq!(filtered.index_at(Position::new(12, 7), 0, 20), None);
        assert_eq!(filtered.index_at(Position::new(12, 8), 0, 20), Some(0));
        assert_eq!(plain.index_at(Position::new(12, 7), 0, 20), Some(0));
    }

    /// 过滤行可见时，命中区**不能**比渲染多出一行（旧实现底部会多一行，
    /// 点到面板下边框会选中屏幕外的歌）。
    #[test]
    fn filtered_and_plain_hit_areas_never_exceed_the_rendered_rows() {
        let area = Rect::new(10, 5, 80, 12);
        let filtered = rows(area, true);
        let plain = rows(area, false);

        // 渲染的数据行数：面板可用高度 - 过滤行 - 表头
        assert_eq!(filtered.list.height, plain.list.height - 1);
        assert_eq!(filtered.list.bottom(), plain.list.bottom(), "两者底部对齐");

        let last_row = Position::new(12, filtered.list.bottom() - 1);
        assert_eq!(
            filtered.index_at(last_row, 0, 20),
            Some(filtered.list.height as usize - 1),
            "最后一行可见数据行的下标"
        );
        assert_eq!(
            filtered.index_at(Position::new(12, filtered.list.bottom()), 0, 20),
            None,
            "面板下边框那一行不属于列表"
        );
    }
}
