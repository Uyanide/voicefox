//! 渲染期间的「终端光标锚点」登记处。
//!
//! ## 为什么需要它
//!
//! 输入法（fcitx5、ibus 等）的候选框跟随**终端光标**的位置。而 ratatui 用的是
//! 差分渲染：当我们不设置 `Frame::cursor_position` 时，`Terminal::try_draw` 只发
//! 一个「隐藏光标」序列、**不移动光标**，于是光标就停在**本帧最后一个被写入的
//! 单元格**上（`Buffer::diff` 按行序遍历，所以实际上是最下方那一行最后一个变化
//! 的格子）。
//!
//! 打字时，搜索框那一行在变，底部的状态栏/进度条也在变。哪一行「最后变」取决于
//! 本帧的差分结果，于是在两处之间来回切换 —— 表现就是输入法候选框上下反复跳跃
//! （issue #42）。英文输入法没有候选框，所以看不出问题。
//!
//! ## 怎么做
//!
//! 每个文本输入在渲染时把**插入点**登记进来；主循环每帧据此显式设置
//! `Frame::cursor_position`。这样光标位置只由「当前焦点在哪个输入框」决定，
//! 与差分结果无关，候选框自然稳定地待在正在输入的位置。
//!
//! 注意：登记的是**插入点**，不是鼠标位置；没有文本输入获得焦点时不做登记，
//! 主循环会保持光标隐藏（ratatui 的默认行为）。

use std::cell::Cell;

use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

thread_local! {
    /// 本帧的插入点请求。渲染在单线程上跑，所以用 thread-local 就够了，
    /// 也免得给每个页面/组件的 render 再加一个出参。
    static REQUEST: Cell<Option<Position>> = const { Cell::new(None) };
}

/// 开始新的一帧：清空上一帧的登记。
pub fn clear() {
    REQUEST.with(|cell| cell.set(None));
}

/// 取走本帧的登记（`clear` 之后到下一次 `clear` 之前最多只有一个）。
pub fn take() -> Option<Position> {
    REQUEST.with(|cell| cell.take())
}

/// 登记插入点：`area` 内先画 `prefix`、再画 `text` 之后的位置。
///
/// `prefix` 必须与调用方实际绘制的前缀完全一致（例如搜索框的 `" / "`），
/// 这样光标才会落在刚输入的字后面。按显示宽度计算（CJK 宽字符占 2 列），
/// 超出 `area` 时夹在最后一个可见列，避免把光标放到区域外。
pub fn request_after(area: Rect, prefix: &str, text: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let used = UnicodeWidthStr::width(prefix) + UnicodeWidthStr::width(text);
    let max_column = area.width.saturating_sub(1) as usize;
    let x = area.x + used.min(max_column) as u16;
    REQUEST.with(|cell| cell.set(Some(Position::new(x, area.y))));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_lands_right_after_the_typed_text() {
        clear();
        // 边框内区域从 x=10 开始；前缀 " / " 占 3 列，"晴天" 占 4 列。
        request_after(Rect::new(10, 5, 40, 1), " / ", "晴天");
        assert_eq!(take(), Some(Position::new(10 + 3 + 4, 5)));
        // 取走之后就没有了。
        assert_eq!(take(), None);
    }

    #[test]
    fn request_is_clamped_inside_the_area() {
        clear();
        // 区域只有 6 列：插入点不能跑到区域外。
        request_after(Rect::new(0, 2, 6, 1), " / ", "很长很长很长的输入");
        assert_eq!(take(), Some(Position::new(5, 2)));
    }

    #[test]
    fn degenerate_area_is_ignored() {
        clear();
        request_after(Rect::new(0, 0, 0, 1), "", "x");
        assert_eq!(take(), None);
    }

    #[test]
    fn clear_drops_a_pending_request() {
        request_after(Rect::new(0, 0, 20, 1), "", "abc");
        clear();
        assert_eq!(take(), None);
    }

    /// 端到端验证 `页面登记插入点 → 主循环设置 Frame::cursor_position` 这条链路：
    /// 终端光标最终落在输入框里，而不是差分渲染最后碰过的那个单元格上。
    #[test]
    fn anchor_reaches_the_terminal_cursor() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::widgets::Paragraph;

        let mut terminal = Terminal::new(TestBackend::new(40, 12)).expect("test terminal");
        terminal
            .draw(|frame| {
                clear();
                // 先在底部画一行（模拟状态栏），它会让差分渲染的"最后一个变化
                // 单元格"落在屏幕底部——正是候选框乱跳的来源。
                frame.render_widget(Paragraph::new("状态栏"), Rect::new(0, 11, 40, 1));
                // 再模拟搜索框登记插入点，并按主循环的做法设置光标。
                request_after(Rect::new(2, 3, 30, 1), " / ", "晴天");
                if let Some(anchor) = take() {
                    frame.set_cursor_position(anchor);
                }
            })
            .expect("draw");

        // 光标应当钉在搜索框的插入点上：x = 2 + 宽度(" / ")=3 + 宽度("晴天")=4。
        assert_eq!(
            terminal.get_cursor_position().expect("cursor position"),
            Position::new(2 + 3 + 4, 3)
        );
    }
}
