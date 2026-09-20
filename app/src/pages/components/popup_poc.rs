//! `tui-popup` POC：统一居中弹窗的尺寸、边界和状态。
//! 当前不替换业务弹窗，只验证它与现有 Ratatui 0.30 栈的兼容性。
#![allow(dead_code)]

use ratatui::style::Style;
use tui_popup::Popup;

pub fn popup<'a>(title: &'a str, body: &'a str) -> Popup<'a, &'a str> {
    Popup::new(body)
        .title(title)
        .style(Style::new().fg(ratatui::style::Color::White))
}

#[cfg(test)]
mod tests {
    use super::popup;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn popup_renders_on_ratatui_030() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| frame.render_widget(popup("测试", "hello"), frame.area()))
            .unwrap();
    }
}
