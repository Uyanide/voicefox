use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::model::source::SourceId;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::context::AppContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSelectorKey {
    All,
    Custom,
    Favorites,
    Source(SourceId),
}

#[derive(Debug, Clone)]
pub struct SourceSelector {
    items: Vec<(SourceSelectorKey, String)>,
    pub selected: usize,
    pub scroll: usize,
    open: bool,
}

impl SourceSelector {
    pub fn new(items: Vec<(SourceSelectorKey, String)>, selected: usize) -> Self {
        Self {
            selected: selected.min(items.len().saturating_sub(1)),
            scroll: 0,
            items,
            open: false,
        }
    }

    pub fn from_sources(sources: &[SourceId], include_all: bool) -> Self {
        let mut items = Vec::with_capacity(sources.len() + usize::from(include_all));
        if include_all {
            items.push((SourceSelectorKey::All, "全部".into()));
        }
        items.extend(
            sources
                .iter()
                .copied()
                .map(|s| (SourceSelectorKey::Source(s), s.display_name().to_string())),
        );
        Self::new(items, 0)
    }

    pub fn current(&self) -> Option<SourceSelectorKey> {
        self.items.get(self.selected).map(|x| x.0)
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn select(&mut self, index: usize) -> Option<SourceSelectorKey> {
        if index >= self.items.len() {
            return None;
        }
        self.selected = index;
        Some(self.items[index].0)
    }

    pub fn cycle(&mut self, delta: isize) -> Option<SourceSelectorKey> {
        if self.items.is_empty() {
            return None;
        }
        self.selected =
            (self.selected as isize + delta).rem_euclid(self.items.len() as isize) as usize;
        self.current()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<SourceSelectorKey> {
        if self.open {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
                    self.close()
                }
                (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                    self.select(self.selected.saturating_sub(1));
                }
                (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                    self.select((self.selected + 1).min(self.items.len().saturating_sub(1)));
                }
                (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::NONE, KeyCode::Char('g')) => {
                    self.select(0);
                }
                (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::NONE, KeyCode::Char('G')) => {
                    self.select(self.items.len().saturating_sub(1));
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    self.close();
                    return self.current();
                }
                _ => {}
            }
            return None;
        }
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('p' | 'P')) => {
                self.open();
                None
            }
            (KeyModifiers::NONE, KeyCode::Left) | (KeyModifiers::NONE, KeyCode::Char('[')) => {
                self.cycle(-1)
            }
            (KeyModifiers::NONE, KeyCode::Right) | (KeyModifiers::NONE, KeyCode::Char(']')) => {
                self.cycle(1)
            }
            _ => None,
        }
    }

    fn popup_rect(&self, area: Rect) -> Rect {
        let width = area.width.saturating_sub(4).clamp(30, 48);
        let rows = self
            .items
            .len()
            .min(area.height.saturating_sub(8) as usize)
            .max(1);
        let height = rows as u16 + 5;
        Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        )
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Option<SourceSelectorKey> {
        if !self.open || !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return None;
        }
        let popup = self.popup_rect(area);
        if !popup.contains((event.column, event.row).into()) {
            self.close();
            return None;
        }
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(3),
        );
        if inner.contains((event.column, event.row).into()) {
            let index = event.row.saturating_sub(inner.y) as usize;
            if index < self.items.len() {
                self.selected = index;
                self.close();
                return self.current();
            }
        }
        None
    }

    pub fn render_tabs(&self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        if area.width == 0 || area.height == 0 || self.items.is_empty() {
            return;
        }
        let mut spans = vec![Span::styled(
            " 音源：",
            Style::new().fg(crate::theme::muted(ctx)),
        )];
        let mut used = 4usize;
        for (index, (_, label)) in self.items.iter().enumerate() {
            let width = label.chars().count() + 3;
            if used + width + 10 > area.width as usize {
                break;
            }
            if index > 0 {
                spans.push(Span::raw("  "));
                used += 2;
            }
            let style = if index == self.selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(crate::theme::accent(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::muted(ctx))
            };
            spans.push(Span::styled(format!(" {} ", label), style));
            used += width;
        }
        spans.push(Span::styled(
            "  P 切换",
            Style::new()
                .fg(crate::theme::accent(ctx))
                .add_modifier(Modifier::BOLD),
        ));
        Paragraph::new(Line::from(spans)).render(Rect::new(area.x, area.y, area.width, 1), buf);
    }

    pub fn render_popup(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext, title: &str) {
        if !self.open || self.items.is_empty() || area.width == 0 || area.height == 0 {
            return;
        }
        let popup = self.popup_rect(area);
        Clear.render(popup, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .style(Style::new().bg(crate::theme::surface0(ctx)))
            .title(format!(" {} · P ", title));
        let inner = block.inner(popup);
        block.render(popup, buf);
        let visible = inner.height.saturating_sub(1) as usize;
        let start = self.selected.saturating_sub(visible.saturating_sub(1));
        for (row, index) in (start..self.items.len()).take(visible).enumerate() {
            let selected = index == self.selected;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                Style::new()
                    .bg(crate::theme::accent(ctx))
                    .fg(crate::theme::selection_fg(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::text(ctx))
            };
            Paragraph::new(Line::from(Span::styled(
                format!("{}{}", marker, self.items[index].1),
                style,
            )))
            .render(
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                buf,
            );
        }
    }

    #[allow(dead_code)]
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext, title: &str) {
        if area.width == 0 || area.height == 0 || self.items.is_empty() {
            return;
        }
        let mut spans = vec![Span::styled(
            " 音源：",
            Style::new().fg(crate::theme::muted(ctx)),
        )];
        for (i, (_, label)) in self.items.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let style = if i == self.selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(crate::theme::accent(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::muted(ctx))
            };
            spans.push(Span::styled(format!(" {} ", label), style));
        }
        spans.push(Span::styled(
            "  P 切换",
            Style::new()
                .fg(crate::theme::accent(ctx))
                .add_modifier(Modifier::BOLD),
        ));
        Paragraph::new(Line::from(spans)).render(Rect::new(area.x, area.y, area.width, 1), buf);

        if !self.open {
            return;
        }
        let popup = self.popup_rect(area);
        Clear.render(popup, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .style(Style::new().bg(crate::theme::surface0(ctx)))
            .title(format!(" {} · P ", title));
        let inner = block.inner(popup);
        block.render(popup, buf);
        let visible = inner.height.saturating_sub(1) as usize;
        let start = self.selected.saturating_sub(visible.saturating_sub(1));
        for (row, index) in (start..self.items.len()).take(visible).enumerate() {
            let selected = index == self.selected;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                Style::new()
                    .bg(crate::theme::accent(ctx))
                    .fg(crate::theme::selection_fg(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::text(ctx))
            };
            Paragraph::new(Line::from(Span::styled(
                format!("{}{}", marker, self.items[index].1),
                style,
            )))
            .render(
                Rect::new(inner.x, inner.y + row as u16, inner.width, 1),
                buf,
            );
        }
    }
}
