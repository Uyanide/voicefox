//! 播放历史页面

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::{AppAction, InsertPosition};
use lx_core::keybinding::{Action, KeybindingResolver};
use lx_core::model::song::SongInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::context::AppContext;
use crate::pages::components::list_filter::ListFilter;
use crate::pages::sort::{SortState, SortTarget, SortedListCache};

pub fn render(
    area: Rect,
    buf: &mut Buffer,
    ctx: &AppContext,
    state: &mut SortState,
    filter: &ListFilter,
    cache: &mut SortedListCache,
) {
    let (sorted, indices) = history_view(ctx, state, filter.query(), cache);
    let history_len = indices.len();
    let filter_visible = filter.is_active() || !filter.query().is_empty();
    let filter_suffix = if filter.query().is_empty() {
        String::new()
    } else {
        format!(" · 过滤 '{}' ({} 匹配)", filter.query(), history_len)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(crate::theme::border(ctx)))
        .title(format!(
            "播放历史 ({} 首) · 排序 {} · s 切换{}",
            history_len,
            state.mode.label(SortTarget::History),
            filter_suffix
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if filter_visible {
        filter.render(Rect::new(inner.x, inner.y, inner.width, 1), buf, ctx);
    }

    let content_y = if filter_visible { inner.y + 1 } else { inner.y };
    let content_height = if filter_visible {
        inner.height.saturating_sub(1)
    } else {
        inner.height
    };

    if history_len == 0 {
        Paragraph::new(Line::from(Span::styled(
            if filter.query().is_empty() {
                "暂无播放历史"
            } else {
                "无匹配历史记录，按 Esc 清除过滤"
            },
            Style::new().fg(crate::theme::muted(ctx)),
        )))
        .render(
            Rect::new(inner.x, content_y, inner.width, content_height),
            buf,
        );
        return;
    }

    // 确保 selected 不越界
    if state.selected >= history_len {
        state.selected = 0;
    }

    let selected_style = Style::new()
        .bg(crate::theme::accent(ctx))
        .fg(crate::theme::selection_fg(ctx))
        .add_modifier(Modifier::BOLD);
    let normal_style = Style::new().fg(crate::theme::text(ctx));

    if content_height == 0 {
        return;
    }
    Paragraph::new(Line::from(Span::styled(
        super::components::song_table::header(inner.width),
        Style::new().fg(crate::theme::muted(ctx)),
    )))
    .render(Rect::new(inner.x, content_y, inner.width, 1), buf);
    let list = Rect::new(
        inner.x,
        content_y.saturating_add(1),
        inner.width,
        content_height.saturating_sub(1),
    );
    let visible_height = list.height as usize;
    if visible_height == 0 {
        return;
    }
    let total = history_len;

    // 自动调整 scroll
    if state.selected >= state.scroll + visible_height {
        state.scroll = state.selected.saturating_sub(visible_height - 1);
    } else if state.selected < state.scroll {
        state.scroll = state.selected;
    }
    state.scroll = state.scroll.min(total.saturating_sub(visible_height));

    let end = (state.scroll + visible_height).min(total);
    for (view_index, &song_index) in indices.iter().enumerate().take(end).skip(state.scroll) {
        let Some(song) = sorted.get(song_index) else {
            continue;
        };
        let row = view_index - state.scroll;
        if row as u16 >= list.height {
            break;
        }
        let text = super::components::song_table::row(song, view_index, list.width);
        let line_area = Rect::new(list.x, list.y + row as u16, list.width, 1);
        let style = if view_index == state.selected {
            selected_style
        } else {
            normal_style
        };
        Paragraph::new(Line::from(Span::styled(text, style))).render(line_area, buf);
    }
}

pub fn handle_input(
    key: &KeyEvent,
    ctx: &AppContext,
    state: &mut SortState,
    filter_query: &str,
    resolver: &KeybindingResolver,
    cache: &mut SortedListCache,
) -> AppAction {
    let (sorted, indices) = history_view(ctx, state, filter_query, cache);
    let len = indices.len();
    // 按键路径按视图下标直接取歌，避免每次按键都复制整份过滤结果。
    let song_at = |view_index: usize| {
        indices
            .get(view_index)
            .and_then(|&index| sorted.get(index))
            .cloned()
    };

    if let Some(action) = resolver.resolve_page("history", key) {
        match action {
            Action::ListCycleSort => {
                let mode = state.cycle();
                return AppAction::ShowNotification(lx_core::events::Notification::info(format!(
                    "历史排序: {}",
                    mode.label(SortTarget::History)
                )));
            }
            Action::HistoryFilter => {
                return AppAction::None;
            }
            Action::ListSelectUp => {
                if len != 0 {
                    if state.selected > 0 {
                        state.selected -= 1;
                    } else if ctx.config.read().unwrap().ui.wrap_navigation {
                        state.selected = len.saturating_sub(1);
                    }
                }
                return AppAction::None;
            }
            Action::ListSelectDown => {
                if len != 0 {
                    if state.selected + 1 < len {
                        state.selected += 1;
                    } else if ctx.config.read().unwrap().ui.wrap_navigation {
                        state.selected = 0;
                    }
                }
                return AppAction::None;
            }
            Action::ListSelectFirst => {
                state.selected = 0;
                return AppAction::None;
            }
            Action::ListSelectLast => {
                state.selected = len.saturating_sub(1);
                return AppAction::None;
            }
            Action::ListPageUp => {
                state.selected = state.selected.saturating_sub(10);
                return AppAction::None;
            }
            Action::ListPageDown => {
                state.selected = (state.selected + 10).min(len.saturating_sub(1));
                return AppAction::None;
            }
            Action::ListAddToQueue => {
                if let Some(song) = song_at(state.selected) {
                    return AppAction::AddToQueue {
                        song: Box::new(song),
                        position: InsertPosition::End,
                    };
                }
                return AppAction::None;
            }
            Action::ListAddToQueueNext => {
                if let Some(song) = song_at(state.selected) {
                    return AppAction::AddToQueue {
                        song: Box::new(song),
                        position: InsertPosition::Next,
                    };
                }
                return AppAction::None;
            }
            Action::ListActivate => {
                if len != 0 && state.selected < len {
                    let songs = view_songs(sorted, &indices);
                    let index = state.selected;
                    return AppAction::PlaySong { songs, index };
                }
                return AppAction::None;
            }
            Action::ListToggleFavorite => {
                if let Some(song) = song_at(state.selected) {
                    return AppAction::ToggleFavoriteSong(Box::new(song));
                }
                return AppAction::None;
            }
            _ => {}
        }
    }

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('s')) => {
            let mode = state.cycle();
            return AppAction::ShowNotification(lx_core::events::Notification::info(format!(
                "历史排序: {}",
                mode.label(SortTarget::History)
            )));
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if len != 0 {
                if state.selected > 0 {
                    state.selected -= 1;
                } else if ctx.config.read().unwrap().ui.wrap_navigation {
                    state.selected = len.saturating_sub(1);
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if len != 0 {
                if state.selected + 1 < len {
                    state.selected += 1;
                } else if ctx.config.read().unwrap().ui.wrap_navigation {
                    state.selected = 0;
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::NONE, KeyCode::Char('g')) => {
            state.selected = 0;
        }
        (KeyModifiers::NONE, KeyCode::End)
        | (KeyModifiers::NONE, KeyCode::Char('G'))
        | (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
            state.selected = len.saturating_sub(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::NONE, KeyCode::PageUp) => {
            state.selected = state.selected.saturating_sub(10);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) | (KeyModifiers::NONE, KeyCode::PageDown) => {
            state.selected = (state.selected + 10).min(len.saturating_sub(1));
        }
        _ if super::is_song_activation_key(key)
            && len != 0
            && state.selected < len =>
        {
            let songs = view_songs(sorted, &indices);
            let index = state.selected;
            return AppAction::PlaySong { songs, index };
        }
        (KeyModifiers::NONE, KeyCode::Char('a')) => {
            if let Some(song) = song_at(state.selected) {
                return AppAction::AddToQueue {
                    song: Box::new(song),
                    position: InsertPosition::End,
                };
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('A')) | (KeyModifiers::SHIFT, KeyCode::Char('A')) => {
            if let Some(song) = song_at(state.selected) {
                return AppAction::AddToQueue {
                    song: Box::new(song),
                    position: InsertPosition::Next,
                };
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('d')) | (KeyModifiers::NONE, KeyCode::Delete) => {
            if let Some(song) = song_at(state.selected) {
                state.selected = state.selected.min(len.saturating_sub(2));
                return AppAction::RemoveHistory(Box::new(song));
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) => {
            if let Some(song) = song_at(state.selected) {
                return AppAction::ToggleFavoriteSong(Box::new(song));
            }
        }
        (KeyModifiers::SHIFT, KeyCode::Char('D')) | (KeyModifiers::NONE, KeyCode::Char('D')) => {
            state.reset_position();
            return AppAction::ClearHistory;
        }
        _ => {}
    }
    AppAction::None
}

pub fn handle_mouse(
    event: MouseEvent,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    filter_query: &str,
    cache: &mut SortedListCache,
    activate: bool,
) -> AppAction {
    let (sorted, indices) = history_view(ctx, state, filter_query, cache);
    let len = indices.len();
    let scroll_amount = ctx.config.read().unwrap().ui.scroll_amount.max(1);
    let mut activate_index = None;
    match event.kind {
        MouseEventKind::ScrollUp => {
            state.selected = state.selected.saturating_sub(scroll_amount);
        }
        MouseEventKind::ScrollDown => {
            state.selected = (state.selected + scroll_amount).min(len.saturating_sub(1));
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let inner = Block::default().borders(Borders::ALL).inner(area);
            let list_y = inner.y.saturating_add(1);
            if event.row >= list_y && event.row < inner.bottom() {
                let index = state.scroll + event.row.saturating_sub(list_y) as usize;
                if index < len {
                    state.selected = index;
                    if activate {
                        activate_index = Some(index);
                    }
                }
            }
        }
        _ => {}
    }
    if let Some(index) = activate_index {
        return AppAction::PlaySong {
            songs: view_songs(sorted, &indices),
            index,
        };
    }
    AppAction::None
}

pub fn context_song_at(
    event: MouseEvent,
    area: Rect,
    ctx: &AppContext,
    state: &mut SortState,
    filter_query: &str,
    cache: &mut SortedListCache,
) -> Option<(Vec<SongInfo>, usize)> {
    let history = filtered_history(ctx, state, filter_query, cache);
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let list_y = inner.y.saturating_add(1);
    if event.row < list_y || event.row >= inner.bottom() {
        return None;
    }
    let index = state.scroll + event.row.saturating_sub(list_y) as usize;
    if index >= history.len() {
        return None;
    }
    state.selected = index;
    Some((history, index))
}

/// 获取排序后的历史视图：已排序切片 + 匹配过滤的下标。
///
/// 渲染路径直接借用缓存的排序结果，只有过滤命中时才额外分配下标数组。
fn history_view<'a>(
    ctx: &AppContext,
    state: &SortState,
    filter_query: &str,
    cache: &'a mut SortedListCache,
) -> (&'a [SongInfo], Vec<usize>) {
    let version = ctx.storage.generation();
    let sorted = cache.get_or_build(version, state.mode, SortTarget::History, || {
        ctx.storage.load_history()
    });
    if filter_query.trim().is_empty() {
        let indices = (0..sorted.len()).collect();
        return (sorted, indices);
    }

    let query = filter_query.trim().to_lowercase();
    let indices = sorted
        .iter()
        .enumerate()
        .filter(|(_, song)| {
            song.name.to_lowercase().contains(&query) || song.singer.to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .collect();
    (sorted, indices)
}

/// 把历史视图下标还原成歌曲列表，仅在按键/鼠标事件等低频路径调用。
fn view_songs(sorted: &[SongInfo], indices: &[usize]) -> Vec<SongInfo> {
    indices
        .iter()
        .filter_map(|&index| sorted.get(index).cloned())
        .collect()
}

fn filtered_history(
    ctx: &AppContext,
    state: &SortState,
    filter_query: &str,
    cache: &mut SortedListCache,
) -> Vec<SongInfo> {
    let (sorted, indices) = history_view(ctx, state, filter_query, cache);
    view_songs(sorted, &indices)
}
