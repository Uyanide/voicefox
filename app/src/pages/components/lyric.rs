//! 歌词显示组件
//!
//! 对标 go-musicfox internal/ui/lyric.go

use crate::context::AppContext;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

/// 歌词区最小可用高度（含边框）。
///
/// 当前行固定在 inner 的正中（visible_rows / 2），翻译紧跟其后占一行。
/// 要让「上一句 + 当前行 + 翻译 + 下一句」同时可见，inner 至少要 5 行，
/// 加上下边框即 7
pub const MIN_HEIGHT: u16 = 7;

/// 渲染歌词显示
/// area: 可用区域
/// 显示当前行前后各 N 行，使当前行尽量居中
pub fn render(area: Rect, buf: &mut Buffer, ctx: &AppContext) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(crate::theme::border(ctx)))
        .title(" 歌词 ");
    let inner = block.inner(area);
    block.render(area, buf);

    let state = ctx.lyric_service.current_state();

    if state.is_empty || state.lines.is_empty() {
        let text = "♫  暂无歌词";
        let y = inner.y + inner.height / 2;
        Paragraph::new(text)
            .style(Style::new().fg(crate::theme::muted(ctx)))
            .alignment(Alignment::Center)
            .render(Rect::new(inner.x, y, inner.width, 1), buf);
        return;
    }

    let current = state.current_line;
    // 每句歌词至少预留两行：长句使用 ratatui 的 Wrap 自动折行，而不是
    // 静默截断成省略号。这样 CJK/英文长歌词都能完整显示。
    let row_height = 2usize;
    let visible_rows = (inner.height as usize / row_height).max(1);
    if visible_rows == 0 {
        return;
    }

    // 翻译属于当前歌词的核心信息：即使面板很矮，也优先保住“当前行 + 翻译”。
    // 例如 inner 高度只有 4 行时，仍应使用前两行显示当前句和翻译，而不是直接隐藏翻译。
    let translation_visible = state.translation.is_some() && visible_rows >= 2;
    let current_row = if translation_visible {
        (visible_rows / 2).min(visible_rows.saturating_sub(2))
    } else {
        visible_rows / 2
    };
    let start = current.saturating_sub(current_row);
    let end = (current + visible_rows + 1).min(state.lines.len());

    for line_idx in start..end {
        let relative = line_idx as isize - current as isize;
        let row = if relative <= 0 {
            current_row as isize + relative
        } else {
            current_row as isize + relative + isize::from(translation_visible)
        };
        if row < 0 || row >= visible_rows as isize {
            continue;
        }
        let y = inner.y + (row as usize * row_height) as u16;

        let line = &state.lines[line_idx];
        let distance = (line_idx as isize - current as isize).unsigned_abs();

        if line_idx == current && !state.yrc_words.is_empty() {
            render_karaoke_line(
                Rect::new(inner.x, y, inner.width, row_height as u16),
                buf,
                ctx,
                &state.yrc_words,
                state.position_ms,
            );
            continue;
        }

        let (prefix, style) = if line_idx == current {
            (
                "❯ ",
                Style::new()
                    .fg(crate::theme::lavender(ctx))
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            let color = match distance {
                1 => crate::theme::subtext1(ctx),
                2 => crate::theme::subtext0(ctx),
                3..=4 => crate::theme::overlay1(ctx),
                _ => crate::theme::overlay0(ctx),
            };
            ("  ", Style::new().fg(color))
        };

        let text = format!("{}{}", prefix, line.text.trim());
        Paragraph::new(Line::from(Span::styled(text, style)))
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: false })
            .render(Rect::new(inner.x, y, inner.width, row_height as u16), buf);
    }

    if translation_visible && let Some(ref translation) = state.translation {
        let y = inner.y + ((current_row + 1) * row_height) as u16;
        Paragraph::new(Line::from(Span::styled(
            translation.trim(),
            Style::new()
                .fg(crate::theme::teal(ctx))
                .add_modifier(Modifier::ITALIC),
        )))
        .alignment(Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .render(Rect::new(inner.x, y, inner.width, row_height as u16), buf);
    }
}

fn render_karaoke_line(
    area: Rect,
    buf: &mut Buffer,
    ctx: &AppContext,
    words: &[lx_core::model::lyric::YrcWord],
    position_ms: u64,
) {
    let mut spans = vec![Span::styled(
        "❯ ",
        Style::new()
            .fg(crate::theme::lavender(ctx))
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(words.iter().map(|word| {
        let end = word.start.saturating_add(word.duration.max(1));
        let style = if position_ms >= end {
            Style::new()
                .fg(crate::theme::lavender(ctx))
                .add_modifier(Modifier::BOLD)
        } else if position_ms >= word.start {
            Style::new()
                .fg(crate::theme::peach(ctx))
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new().fg(crate::theme::overlay1(ctx))
        };
        Span::styled(word.text.clone(), style)
    }));

    Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .render(area, buf);
}
