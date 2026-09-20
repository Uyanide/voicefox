//! `rat-menu` POC：只负责菜单呈现与输入状态，不承载 voicefox 业务动作。
//!
//! 后续迁移 ContextMenu 时，业务层继续使用 `SongMenuAction`，这里作为 UI adapter。
#![allow(dead_code)]

use crossterm::event::Event;
use rat_menu::event::MenuOutcome;
use rat_menu::menuitem::MenuItem;
use rat_menu::popup_menu::{PopupMenu, PopupMenuState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;

pub struct RatMenuAdapter {
    menu: PopupMenu<'static>,
    state: PopupMenuState,
}

impl RatMenuAdapter {
    pub fn new(labels: impl IntoIterator<Item = String>) -> Self {
        let mut menu = PopupMenu::new();
        for label in labels {
            menu = menu.item(MenuItem::new_string(label));
        }
        let mut state = PopupMenuState::new();
        state.set_active(true);
        Self { menu, state }
    }

    pub fn handle(&mut self, event: &Event) -> MenuOutcome {
        rat_menu::popup_menu::handle_popup_events(&mut self.state, event)
    }

    pub fn selected(&self) -> Option<usize> {
        self.state.selected()
    }

    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.menu.clone().render(area, buf, &mut self.state);
    }
}

#[cfg(test)]
mod tests {
    use super::RatMenuAdapter;
    use crossterm::event::{Event, KeyCode, KeyEvent};

    #[test]
    fn adapter_builds_and_handles_keyboard_events() {
        let mut adapter = RatMenuAdapter::new(["播放".into(), "下载".into()]);
        assert!(adapter.is_active());
        let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 5));
        adapter.render(ratatui::layout::Rect::new(0, 0, 20, 5), &mut buffer);
        let _ = adapter.handle(&Event::Key(KeyEvent::from(KeyCode::Down)));
        assert_eq!(adapter.selected(), Some(1));
    }
}
