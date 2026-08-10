use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::AppAction;
use lx_core::model::song::SongInfo;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::{Block, Borders};

use crate::context::AppContext;
use crate::pages::sort::{SortState, SortTarget, SortedListCache};

pub fn handle_mouse(
    event: MouseEvent,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    cache: &mut SortedListCache,
    activate: bool,
) -> AppAction {
    let songs = sorted_local_songs(ctx, state, cache);
    let scroll_amount = ctx.config.read().unwrap().ui.scroll_amount.max(1);
    match event.kind {
        MouseEventKind::ScrollUp => {
            state.selected = state.selected.saturating_sub(scroll_amount);
        }
        MouseEventKind::ScrollDown => {
            state.selected = (state.selected + scroll_amount).min(songs.len().saturating_sub(1));
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let position = Position::new(event.column, event.row);
            if let Some(index) = song_index_at(area, position, state.scroll, songs.len()) {
                state.selected = index;
                if activate {
                    return AppAction::PlaySong {
                        songs: songs.to_vec(),
                        index,
                    };
                }
            }
        }
        _ => {}
    }
    AppAction::None
}

pub fn context_song_at(
    event: MouseEvent,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    cache: &mut SortedListCache,
) -> Option<(Vec<SongInfo>, usize)> {
    let songs = sorted_local_songs(ctx, state, cache);
    let position = Position::new(event.column, event.row);
    let index = song_index_at(area, position, state.scroll, songs.len())?;
    state.selected = index;
    Some((songs.to_vec(), index))
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

fn song_index_at(area: Rect, position: Position, scroll: usize, len: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let visible_height = inner.height.saturating_sub(2);
    let list_area = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        visible_height,
    );
    if !list_area.contains(position) {
        return None;
    }

    let index = scroll + position.y.saturating_sub(list_area.y) as usize;
    (index < len).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::song_index_at;
    use ratatui::layout::{Position, Rect};

    #[test]
    fn maps_visible_rows_to_scrolled_song_indices() {
        let area = Rect::new(10, 5, 80, 12);

        assert_eq!(song_index_at(area, Position::new(12, 7), 4, 20), Some(4));
        assert_eq!(song_index_at(area, Position::new(12, 10), 4, 20), Some(7));
    }

    #[test]
    fn ignores_header_border_and_unused_bottom_row() {
        let area = Rect::new(10, 5, 80, 12);

        assert_eq!(song_index_at(area, Position::new(12, 6), 0, 20), None);
        assert_eq!(song_index_at(area, Position::new(12, 15), 0, 20), None);
        assert_eq!(song_index_at(area, Position::new(9, 7), 0, 20), None);
    }
}
