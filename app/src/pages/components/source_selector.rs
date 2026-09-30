use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::model::source::SourceId;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::context::AppContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSelectorKey {
    All,
    Custom,
    Favorites,
    /// 登录账号下的个人歌单（与同音源的公开/推荐入口并存，因此必须区分）。
    Account(SourceId),
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

    /// 标签条前缀：`" 音源："` 含全角冒号，终端显示宽度 **7** 列。
    ///
    /// 以前的命中算法按 4 列估算，导致命中区整体左移 3 列、并逐项累积漂移，
    /// 点左侧 tab 会选中隔壁音源。现在渲染与命中都从 [`Self::tab_rects`] 取几何。
    const TAB_PREFIX: &'static str = " 音源：";
    /// 标签条尾部提示。
    const TAB_SUFFIX: &'static str = "  P 切换";
    /// 相邻两个 tab 之间的空白。
    const TAB_GAP: u16 = 2;

    fn tab_prefix_width() -> u16 {
        UnicodeWidthStr::width(Self::TAB_PREFIX) as u16
    }

    fn tab_suffix_width() -> u16 {
        UnicodeWidthStr::width(Self::TAB_SUFFIX) as u16
    }

    /// 单个 tab 占用的显示宽度：`" label "`。
    fn tab_cell_width(label: &str) -> u16 {
        UnicodeWidthStr::width(label) as u16 + 2
    }

    /// 标签条上每个 tab 的矩形（含它在 `items` 里的下标）。
    ///
    /// **渲染与鼠标命中共用这一份几何**，右侧要给尾部提示留位置，
    /// 放不下的 tab 直接不参与排布（渲染也不画）。
    pub fn tab_rects(&self, area: Rect) -> Vec<(usize, SourceSelectorKey, Rect)> {
        let mut rects = Vec::new();
        if area.width == 0 || area.height == 0 || self.items.is_empty() {
            return rects;
        }
        let y = area.y;
        let mut x = area
            .x
            .saturating_add(Self::tab_prefix_width())
            .min(area.right());
        for (index, (key, label)) in self.items.iter().enumerate() {
            if index > 0 {
                x = x.saturating_add(Self::TAB_GAP);
            }
            let width = Self::tab_cell_width(label);
            // 放不下的 tab 不再排布（渲染也不画）；尾部提示在渲染时按剩余宽度决定
            // 是否绘制，因此这里不预先扣掉它，窄终端也能把 tab 排出来。
            if x.saturating_add(width) > area.right() {
                break;
            }
            rects.push((index, *key, Rect::new(x, y, width, 1)));
            x = x.saturating_add(width);
        }
        rects
    }

    /// 命中标签条上的某个 tab。
    pub fn tab_at(
        &self,
        area: Rect,
        position: ratatui::layout::Position,
    ) -> Option<SourceSelectorKey> {
        self.tab_rects(area)
            .into_iter()
            .find(|(_, _, rect)| rect.contains(position))
            .map(|(_, key, _)| key)
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
        let inner = Block::default().borders(Borders::ALL).inner(popup);
        let visible = inner.height.saturating_sub(1) as usize;
        // 与 render_popup 用同一套窗口推导：列表首行下标 = selected 反向滚动出的 start。
        // 少了这个偏移，start > 0 时点中的行会和实际行错位。
        let start = self.selected.saturating_sub(visible.saturating_sub(1));
        let list = Rect::new(inner.x, inner.y, inner.width, visible as u16);
        if list.contains((event.column, event.row).into()) {
            let index = start + event.row.saturating_sub(inner.y) as usize;
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
        let rects = self.tab_rects(area);
        if rects.is_empty() {
            return;
        }

        let prefix_width = Self::tab_prefix_width().min(area.width);
        if prefix_width > 0 {
            Paragraph::new(Span::styled(
                Self::TAB_PREFIX,
                Style::new().fg(crate::theme::muted(ctx)),
            ))
            .render(Rect::new(area.x, area.y, prefix_width, 1), buf);
        }

        for (index, _, rect) in &rects {
            let Some((_, label)) = self.items.get(*index) else {
                continue;
            };
            let style = if *index == self.selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(crate::theme::accent(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::muted(ctx))
            };
            Paragraph::new(Span::styled(format!(" {label} "), style)).render(*rect, buf);
        }

        // 尾部提示紧跟在最后一个 tab 之后
        let suffix_start = rects
            .last()
            .map(|(_, _, rect)| rect.right())
            .unwrap_or(area.x);
        let suffix_width = Self::tab_suffix_width().min(area.right().saturating_sub(suffix_start));
        if suffix_width > 0 {
            Paragraph::new(Span::styled(
                Self::TAB_SUFFIX,
                Style::new()
                    .fg(crate::theme::accent(ctx))
                    .add_modifier(Modifier::BOLD),
            ))
            .render(Rect::new(suffix_start, area.y, suffix_width, 1), buf);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::{Position, Rect};

    fn selector() -> SourceSelector {
        SourceSelector::new(
            vec![
                (SourceSelectorKey::Custom, "自建".to_string()),
                (SourceSelectorKey::Favorites, "已收藏".to_string()),
                (
                    SourceSelectorKey::Source(SourceId::Wy),
                    "网易云".to_string(),
                ),
            ],
            0,
        )
    }

    /// 命中几何必须与渲染几何同源：前缀 `" 音源："` 实宽 7 列，
    /// 每个 tab 是 `" label "`（CJK 按 2 列计），项间 2 列空白。
    #[test]
    fn tab_rects_follow_the_rendered_prefix_and_cell_widths() {
        let s = selector();
        let rects = s.tab_rects(Rect::new(0, 0, 80, 1));

        let x: Vec<u16> = rects.iter().map(|(_, _, rect)| rect.x).collect();
        let w: Vec<u16> = rects.iter().map(|(_, _, rect)| rect.width).collect();
        assert_eq!(x, vec![7, 15, 25], "首项从 7 列开始（前缀宽度）");
        assert_eq!(w, vec![6, 8, 8], "自建=4+2、已收藏=6+2、网易云=6+2");
    }

    #[test]
    fn tab_hit_test_matches_the_drawn_cells() {
        let s = selector();
        let area = Rect::new(0, 0, 80, 1);
        let rects = s.tab_rects(area);

        for (_, key, rect) in &rects {
            // 格子首列与末列都必须命中自己
            assert_eq!(s.tab_at(area, Position::new(rect.x, 0)), Some(*key));
            assert_eq!(
                s.tab_at(area, Position::new(rect.right() - 1, 0)),
                Some(*key)
            );
        }
        // 前缀区不再是"第一个 tab"
        assert_eq!(s.tab_at(area, Position::new(0, 0)), None);
        assert_eq!(s.tab_at(area, Position::new(6, 0)), None);
        // 相邻 tab 之间的空白不属于任何一个
        assert_eq!(s.tab_at(area, Position::new(13, 0)), None);
        assert_eq!(s.tab_at(area, Position::new(23, 0)), None);
        // 最后一个 tab 之后再无命中
        assert_eq!(s.tab_at(area, Position::new(33, 0)), None);
    }

    /// 每点必中：标签条上**任何**一个 tab 的每个可见列都映射到它自己，
    /// 这正是旧实现（前缀 4 列、每项 +3 列）做不到的。
    #[test]
    fn clicking_any_visible_column_of_a_tab_selects_that_tab() {
        let s = selector();
        let area = Rect::new(0, 0, 80, 1);
        for (_, key, rect) in s.tab_rects(area) {
            for column in rect.x..rect.right() {
                assert_eq!(
                    s.tab_at(area, Position::new(column, 0)),
                    Some(key),
                    "第 {column} 列应属于 {}",
                    rect.x
                );
            }
        }
    }

    #[test]
    fn tabs_that_do_not_fit_are_not_laid_out() {
        let s = selector();
        // 宽度只够前缀 + 第一个 tab
        let rects = s.tab_rects(Rect::new(0, 0, 14, 1));
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].2.x, 7);
    }
}
