//! 通用列表过滤组件
//!
//! 为任何需要搜索/过滤功能的列表页面提供统一的输入处理、过滤逻辑和渲染。
//!
//! # 用法
//! ```ignore
//! let mut filter = ListFilter::new();
//!
//! // 在页面 handle_input 中
//! if filter.handle_input(key) {
//!     return AppAction::None; // 过滤组件消耗了这个按键
//! }
//!
//! // 切换过滤模式
//! if action == Action::FavoritesFilter {
//!     filter.activate();
//! }
//!
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::context::AppContext;

/// 列表过滤状态
#[derive(Debug, Clone)]
pub struct ListFilter {
    query: String,
    active: bool,
}

impl Default for ListFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ListFilter {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            active: false,
        }
    }

    /// 当前是否在输入模式
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 当前过滤字符串
    pub fn query(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    /// 进入过滤输入模式
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// 退出过滤输入模式（不清除 query）
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// 完全重置（清除 query 并退出输入模式）
    pub fn reset(&mut self) {
        self.query.clear();
        self.active = false;
    }

    /// 处理按键输入。
    ///
    /// 当过滤组件处于激活状态时，消耗所有字符输入、Backspace、Esc 和 Enter。
    /// 返回 `true` 表示按键已被消耗，页面不应再处理。
    ///
    /// # 按键映射
    /// - `Esc` → 退出输入模式，保留 query 作为过滤条件
    /// - `Enter` → 退出输入模式，保留 query
    /// - `Backspace` → 删除最后一个字符，同时重置选中到顶部
    /// - 普通字符 → 追加到 query，同时重置选中到顶部
    pub fn handle_input(&mut self, key: &KeyEvent) -> bool {
        if !self.active {
            return false;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.active = false;
                true
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.active = false;
                true
            }
            (_, KeyCode::Backspace)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.pop();
                true
            }
            (_, KeyCode::Char(character))
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.query.push(character);
                true
            }
            _ => true, // 激活状态下消耗所有其他按键，防止误触发全局快捷键
        }
    }

    /// 渲染过滤输入栏。
    ///
    /// 在区域顶部渲染一个高度为 1 的行，显示当前模式（INSERT/FILTER）和 query。
    /// 仅在 `active == true` 或 `query` 非空时渲染有实际内容；
    /// 调用方应根据此方法是否产生可见内容来调整列表区域。
    pub fn render(&self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let mode = if self.active { "INSERT" } else { "FILTER" };
        let accent = if self.active {
            crate::theme::green(ctx)
        } else {
            crate::theme::accent(ctx)
        };
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" / {} ", mode),
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", self.query),
                Style::new()
                    .fg(crate::theme::text(ctx))
                    .bg(crate::theme::surface0(ctx)),
            ),
        ]))
        .style(Style::new().bg(crate::theme::surface0(ctx)))
        .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::ListFilter;
    use crossterm::event::{KeyCode, KeyEvent};

    #[test]
    fn new_filter_is_inactive_with_empty_query() {
        let f = ListFilter::new();
        assert!(!f.is_active());
        assert_eq!(f.query(), "");
    }

    #[test]
    fn activate_and_deactivate() {
        let mut f = ListFilter::new();
        f.activate();
        assert!(f.is_active());
        f.deactivate();
        assert!(!f.is_active());
        assert_eq!(f.query(), ""); // query 保留
    }

    #[test]
    fn reset_clears_everything() {
        let mut f = ListFilter::new();
        f.activate();
        f.query.push_str("hello");
        f.reset();
        assert!(!f.is_active());
        assert_eq!(f.query(), "");
    }

    #[test]
    fn handle_input_consumes_when_active() {
        let mut f = ListFilter::new();
        f.activate();

        // 字符输入
        let key = KeyEvent::from(KeyCode::Char('a'));
        assert!(f.handle_input(&key));
        assert_eq!(f.query(), "a");

        // Backspace
        let key = KeyEvent::from(KeyCode::Backspace);
        assert!(f.handle_input(&key));
        assert_eq!(f.query(), "");

        // Esc 退出
        let key = KeyEvent::from(KeyCode::Esc);
        assert!(f.handle_input(&key));
        assert!(!f.is_active());
    }

    #[test]
    fn handle_input_passes_through_when_inactive() {
        let mut f = ListFilter::new();
        let key = KeyEvent::from(KeyCode::Char('a'));
        assert!(!f.handle_input(&key));
        assert_eq!(f.query(), "");
    }
}
