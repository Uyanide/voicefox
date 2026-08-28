//! 文本按显示宽度截断的工具。
//!
//! 终端按“列”（单元格）排版，CJK 等宽字符占 2 列，因此截断必须以
//! 显示宽度为准；此前多处实现按 `chars().count()` 计数，导致含中文的
//! 行在行尾溢出或截得过短。这里统一收敛为一个实现。

use std::borrow::Cow;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 按显示宽度截断字符串；超宽时保留 `max_cols - 1` 列并以省略号结尾。
///
/// 返回 `Cow`：未超宽时零拷贝借用原串。宽字符在边界放不下（会溢出
/// 半列）时提前停止，避免行尾出现空白缺口。
pub fn truncate_width(s: &str, max_cols: usize) -> Cow<'_, str> {
    if UnicodeWidthStr::width(s) <= max_cols {
        return Cow::Borrowed(s);
    }
    if max_cols <= 1 {
        // 宽度不足以容纳省略号本身时，按列数截出占位符
        return Cow::Owned("…".chars().take(max_cols).collect());
    }
    let mut result = String::new();
    let mut used = 0;
    for character in s.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > max_cols - 1 {
            break;
        }
        result.push(character);
        used += character_width;
    }
    result.push('…');
    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use super::truncate_width;

    #[test]
    fn borrows_when_within_width() {
        let text = "abc";
        assert!(matches!(
            truncate_width(text, 5),
            std::borrow::Cow::Borrowed("abc")
        ));
    }

    #[test]
    fn truncates_cjk_by_display_columns() {
        // “晴天”占 4 列，加省略号共 6 列，超过 5 列的宽度限制
        assert_eq!(truncate_width("晴天周杰伦", 5), "晴天…");
        assert_eq!(truncate_width("晴天", 4), "晴天");
        // 宽字符放不下半列时提前停止，不留缺口："晴ab" 占 4 列 > 3
        assert_eq!(truncate_width("晴ab", 3), "晴…");
        // 恰好占满可用宽度时不加省略号
        assert_eq!(truncate_width("晴a", 3), "晴a");
    }

    #[test]
    fn tiny_width_keeps_placeholder() {
        assert_eq!(truncate_width("abc", 1), "…");
        assert_eq!(truncate_width("abc", 0), "");
    }
}
