//! 歌曲右键上下文菜单。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::keybinding::{Action, KeybindingResolver};
use lx_core::model::song::SongInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::context::AppContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongMenuKind {
    Queue,
    Standard,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongMenuAction {
    Play,
    PlayNext,
    AddToQueue,
    ToggleFavorite,
    RemoveFromQueue,
    DeleteLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOutcome {
    None,
    Close,
    Action(SongMenuAction),
}

#[derive(Debug, Clone)]
struct MenuItem {
    label: String,
    action: SongMenuAction,
}

#[derive(Debug, Clone)]
pub struct SongContextMenu {
    origin: Position,
    songs: Vec<SongInfo>,
    index: usize,
    selected: usize,
    items: Vec<MenuItem>,
}

impl SongContextMenu {
    pub fn new(
        origin: Position,
        songs: Vec<SongInfo>,
        index: usize,
        kind: SongMenuKind,
        is_favorite: bool,
    ) -> Option<Self> {
        songs.get(index)?;
        let mut items = vec![MenuItem {
            label: "播放".to_string(),
            action: SongMenuAction::Play,
        }];
        if kind != SongMenuKind::Queue {
            items.extend([
                MenuItem {
                    label: "设为下一首".to_string(),
                    action: SongMenuAction::PlayNext,
                },
                MenuItem {
                    label: "加入队尾".to_string(),
                    action: SongMenuAction::AddToQueue,
                },
            ]);
        }
        items.push(MenuItem {
            label: if is_favorite {
                "取消收藏".to_string()
            } else {
                "收藏歌曲".to_string()
            },
            action: SongMenuAction::ToggleFavorite,
        });
        match kind {
            SongMenuKind::Queue => items.push(MenuItem {
                label: "从队列移除".to_string(),
                action: SongMenuAction::RemoveFromQueue,
            }),
            SongMenuKind::Local => items.push(MenuItem {
                label: "删除本地文件".to_string(),
                action: SongMenuAction::DeleteLocal,
            }),
            SongMenuKind::Standard => {}
        }

        Some(Self {
            origin,
            songs,
            index,
            selected: 0,
            items,
        })
    }

    pub fn songs(&self) -> &[SongInfo] {
        &self.songs
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn song(&self) -> &SongInfo {
        &self.songs[self.index]
    }

    pub fn handle_key(
        &mut self,
        key: &KeyEvent,
        resolver: &KeybindingResolver,
        page_scope: &str,
    ) -> MenuOutcome {
        if matches!(
            (key.modifiers, key.code),
            (KeyModifiers::NONE, KeyCode::Esc | KeyCode::Char('q'))
        ) {
            return MenuOutcome::Close;
        }
        if matches!(
            (key.modifiers, key.code),
            (KeyModifiers::NONE, KeyCode::Enter)
        ) {
            return self.activate();
        }

        match resolver.resolve_page(page_scope, key) {
            Some(Action::ListSelectUp) => self.select_previous(),
            Some(Action::ListSelectDown) => self.select_next(),
            Some(Action::ListActivate) => return self.activate(),
            Some(Action::ListGoBack) => return MenuOutcome::Close,
            _ => match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Up) => self.select_previous(),
                (KeyModifiers::NONE, KeyCode::Down) => self.select_next(),
                _ => {}
            },
        }
        MenuOutcome::None
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, bounds: Rect) -> MenuOutcome {
        let area = self.area(bounds);
        match event.kind {
            MouseEventKind::ScrollUp => self.select_previous(),
            MouseEventKind::ScrollDown => self.select_next(),
            MouseEventKind::Moved => {
                if let Some(index) = item_at(
                    area,
                    Position::new(event.column, event.row),
                    self.items.len(),
                ) {
                    self.selected = index;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let position = Position::new(event.column, event.row);
                let Some(index) = item_at(area, position, self.items.len()) else {
                    return MenuOutcome::Close;
                };
                self.selected = index;
                return self.activate();
            }
            MouseEventKind::Down(MouseButton::Right) => return MenuOutcome::Close,
            _ => {}
        }
        MenuOutcome::None
    }

    pub fn render(&self, bounds: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let area = self.area(bounds);
        if area.width == 0 || area.height == 0 {
            return;
        }
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .style(
                Style::new()
                    .bg(crate::theme::surface0(ctx))
                    .fg(crate::theme::text(ctx)),
            )
            .title(" 歌曲操作 ");
        let inner = block.inner(area);
        block.render(area, buf);

        for (row, item) in self.items.iter().take(inner.height as usize).enumerate() {
            let style = if row == self.selected {
                Style::new()
                    .bg(crate::theme::accent(ctx))
                    .fg(crate::theme::selection_fg(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new()
                    .bg(crate::theme::surface0(ctx))
                    .fg(crate::theme::text(ctx))
            };
            Paragraph::new(Line::from(Span::styled(format!(" {}", item.label), style)))
                .style(style)
                .render(
                    Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                    buf,
                );
        }
    }

    fn area(&self, bounds: Rect) -> Rect {
        menu_area(bounds, self.origin, self.items.len())
    }

    fn select_previous(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.items.len() - 1
        } else {
            self.selected - 1
        };
    }

    fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    fn activate(&self) -> MenuOutcome {
        self.items
            .get(self.selected)
            .map(|item| MenuOutcome::Action(item.action))
            .unwrap_or(MenuOutcome::Close)
    }
}

fn menu_area(bounds: Rect, origin: Position, item_count: usize) -> Rect {
    if bounds.width == 0 || bounds.height == 0 {
        return Rect::default();
    }
    let width = 24.min(bounds.width);
    let height = (item_count as u16 + 2).min(bounds.height);
    let max_x = bounds.right().saturating_sub(width);
    let max_y = bounds.bottom().saturating_sub(height);
    Rect::new(
        origin.x.clamp(bounds.x, max_x),
        origin.y.clamp(bounds.y, max_y),
        width,
        height,
    )
}

fn item_at(area: Rect, position: Position, item_count: usize) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !inner.contains(position) {
        return None;
    }
    let index = position.y.saturating_sub(inner.y) as usize;
    (index < item_count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::{item_at, menu_area};
    use ratatui::layout::{Position, Rect};

    #[test]
    fn context_menu_is_clamped_inside_content_area() {
        let bounds = Rect::new(10, 5, 40, 12);
        let area = menu_area(bounds, Position::new(48, 15), 5);

        assert!(bounds.contains(Position::new(area.x, area.y)));
        assert_eq!(area.right(), bounds.right());
        assert_eq!(area.bottom(), bounds.bottom());
    }

    #[test]
    fn menu_item_hit_test_ignores_border() {
        let area = Rect::new(10, 5, 24, 7);

        assert_eq!(item_at(area, Position::new(12, 6), 5), Some(0));
        assert_eq!(item_at(area, Position::new(12, 10), 5), Some(4));
        assert_eq!(item_at(area, Position::new(10, 6), 5), None);
    }
}
