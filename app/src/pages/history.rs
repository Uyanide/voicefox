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
use crate::pages::sort::{SortState, SortTarget, sorted_songs};

pub fn render(area: Rect, buf: &mut Buffer, ctx: &AppContext, state: &mut SortState) {
    let history = sorted_history(ctx, state);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(crate::theme::border(ctx)))
        .title(format!(
            "播放历史 ({} 首) · 排序 {} · s 切换",
            history.len(),
            state.mode.label(SortTarget::History)
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if history.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "暂无播放历史",
            Style::new().fg(crate::theme::muted(ctx)),
        )))
        .render(inner, buf);
        return;
    }

    // 确保 selected 不越界
    if state.selected >= history.len() {
        state.selected = 0;
    }

    let selected_style = Style::new()
        .bg(crate::theme::accent(ctx))
        .fg(crate::theme::selection_fg(ctx))
        .add_modifier(Modifier::BOLD);
    let normal_style = Style::new().fg(crate::theme::text(ctx));

    if inner.height == 0 {
        return;
    }
    Paragraph::new(Line::from(Span::styled(
        super::components::song_table::header(inner.width),
        Style::new().fg(crate::theme::muted(ctx)),
    )))
    .render(Rect::new(inner.x, inner.y, inner.width, 1), buf);
    let list = Rect::new(
        inner.x,
        inner.y.saturating_add(1),
        inner.width,
        inner.height.saturating_sub(1),
    );
    let visible_height = list.height as usize;
    if visible_height == 0 {
        return;
    }
    let total = history.len();

    // 自动调整 scroll
    if state.selected >= state.scroll + visible_height {
        state.scroll = state.selected.saturating_sub(visible_height - 1);
    } else if state.selected < state.scroll {
        state.scroll = state.selected;
    }
    state.scroll = state.scroll.min(total.saturating_sub(visible_height));

    let end = (state.scroll + visible_height).min(total);
    for (i, song) in history.iter().enumerate().take(end).skip(state.scroll) {
        let row = i - state.scroll;
        if row as u16 >= list.height {
            break;
        }
        let text = super::components::song_table::row(song, i, list.width);
        let line_area = Rect::new(list.x, list.y + row as u16, list.width, 1);
        let style = if i == state.selected {
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
    resolver: &KeybindingResolver,
) -> AppAction {
    let history = sorted_history(ctx, state);

    if let Some(action) = resolver.resolve_page("history", key) {
        match action {
            Action::ListCycleSort => {
                let mode = state.cycle();
                return AppAction::ShowNotification(lx_core::events::Notification::info(format!(
                    "历史排序: {}",
                    mode.label(SortTarget::History)
                )));
            }
            Action::ListSelectUp => {
                if !history.is_empty() {
                    if state.selected > 0 {
                        state.selected -= 1;
                    } else if ctx.config.read().unwrap().ui.wrap_navigation {
                        state.selected = history.len().saturating_sub(1);
                    }
                }
                return AppAction::None;
            }
            Action::ListSelectDown => {
                if !history.is_empty() {
                    if state.selected + 1 < history.len() {
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
                state.selected = history.len().saturating_sub(1);
                return AppAction::None;
            }
            Action::ListPageUp => {
                state.selected = state.selected.saturating_sub(10);
                return AppAction::None;
            }
            Action::ListPageDown => {
                state.selected = (state.selected + 10).min(history.len().saturating_sub(1));
                return AppAction::None;
            }
            Action::ListAddToQueue => {
                if let Some(song) = history.get(state.selected).cloned() {
                    return AppAction::AddToQueue {
                        song: Box::new(song),
                        position: InsertPosition::End,
                    };
                }
                return AppAction::None;
            }
            Action::ListAddToQueueNext => {
                if let Some(song) = history.get(state.selected).cloned() {
                    return AppAction::AddToQueue {
                        song: Box::new(song),
                        position: InsertPosition::Next,
                    };
                }
                return AppAction::None;
            }
            Action::ListActivate => {
                if !history.is_empty() && state.selected < history.len() {
                    let songs = history.clone();
                    let index = state.selected;
                    return AppAction::PlaySong { songs, index };
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
            if !history.is_empty() {
                if state.selected > 0 {
                    state.selected -= 1;
                } else if ctx.config.read().unwrap().ui.wrap_navigation {
                    state.selected = history.len().saturating_sub(1);
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if !history.is_empty() {
                if state.selected + 1 < history.len() {
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
            state.selected = history.len().saturating_sub(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::NONE, KeyCode::PageUp) => {
            state.selected = state.selected.saturating_sub(10);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) | (KeyModifiers::NONE, KeyCode::PageDown) => {
            state.selected = (state.selected + 10).min(history.len().saturating_sub(1));
        }
        _ if super::is_song_activation_key(key)
            && !history.is_empty()
            && state.selected < history.len() =>
        {
            let songs = history.clone();
            let index = state.selected;
            return AppAction::PlaySong { songs, index };
        }
        (KeyModifiers::NONE, KeyCode::Char('a')) => {
            if let Some(song) = history.get(state.selected).cloned() {
                return AppAction::AddToQueue {
                    song: Box::new(song),
                    position: InsertPosition::End,
                };
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('A')) | (KeyModifiers::SHIFT, KeyCode::Char('A')) => {
            if let Some(song) = history.get(state.selected).cloned() {
                return AppAction::AddToQueue {
                    song: Box::new(song),
                    position: InsertPosition::Next,
                };
            }
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
    activate: bool,
) -> AppAction {
    let history = sorted_history(ctx, state);
    let scroll_amount = ctx.config.read().unwrap().ui.scroll_amount.max(1);
    match event.kind {
        MouseEventKind::ScrollUp => {
            state.selected = state.selected.saturating_sub(scroll_amount);
        }
        MouseEventKind::ScrollDown => {
            state.selected = (state.selected + scroll_amount).min(history.len().saturating_sub(1));
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let inner = Block::default().borders(Borders::ALL).inner(area);
            let list_y = inner.y.saturating_add(1);
            if event.row >= list_y && event.row < inner.bottom() {
                let index = state.scroll + event.row.saturating_sub(list_y) as usize;
                if index < history.len() {
                    state.selected = index;
                    if activate {
                        return AppAction::PlaySong {
                            songs: history,
                            index,
                        };
                    }
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
) -> Option<(Vec<SongInfo>, usize)> {
    let history = sorted_history(ctx, state);
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

fn sorted_history(ctx: &AppContext, state: &SortState) -> Vec<SongInfo> {
    sorted_songs(ctx.storage.load_history(), state.mode, SortTarget::History)
}
