//! 播放进度条

use lx_core::model::source::PlayerState;

use crate::context::AppContext;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::time::Duration;

use crate::fmt::format_duration;

pub fn render(area: Rect, buf: &mut Buffer, ctx: &AppContext) {
    let accent = crate::theme::accent(ctx);
    if area.height == 0 || area.width == 0 {
        return;
    }
    let position = *ctx.position.borrow();
    let duration = *ctx.duration.borrow();

    if duration == Duration::ZERO {
        // 时长未知（缓冲中）时画一段来回游走的色块；停止/空闲时保持空白。
        // 相位由时钟无状态推导，不存任何每帧状态，就不会抖动或闪烁。
        if *ctx.player_state.borrow() == PlayerState::Loading {
            render_indeterminate(area, buf, ctx, accent);
        }
        return;
    }

    let ratio = (position.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0);
    if area.width < 18 {
        Paragraph::new(format!(
            "{}/{}",
            format_duration(position),
            format_duration(duration)
        ))
        .style(Style::new().fg(crate::theme::subtext0(ctx)))
        .render(area, buf);
        return;
    }
    let bar_width = area.width.saturating_sub(13) as usize;
    let filled = (bar_width as f64 * ratio) as usize;
    let empty = bar_width.saturating_sub(filled);

    let bar_spans = vec![
        Span::styled(
            format!(" {} ", format_duration(position)),
            Style::new().fg(crate::theme::subtext0(ctx)),
        ),
        Span::styled("█".repeat(filled), Style::new().fg(accent)),
        Span::styled(
            "░".repeat(empty),
            Style::new().fg(crate::theme::surface1(ctx)),
        ),
        Span::styled(
            format!(" {}", format_duration(duration)),
            Style::new().fg(crate::theme::subtext0(ctx)),
        ),
    ];

    Paragraph::new(Line::from(bar_spans)).render(area, buf);
}

/// 缓冲中（时长未知）的不确定态：一段 6 格的色块在轨道上来回游走。
///
/// 相位由 `SystemTime` 毫秒按固定步长量化后走三角波：同一 300ms 窗口内
/// 渲染多少次都稳定，多帧之间自然衔接，不存任何每帧状态。
fn render_indeterminate(
    area: Rect,
    buf: &mut Buffer,
    ctx: &AppContext,
    accent: ratatui::style::Color,
) {
    const STEP_MS: u128 = 300;
    const BLOCK: usize = 6;
    let track = area.width.saturating_sub(13) as usize;
    if track <= BLOCK + 2 {
        return;
    }
    let cycle = 2 * (track - BLOCK);
    let phase = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_millis() / STEP_MS)
        .unwrap_or_default() as usize)
        % cycle;
    let offset = if phase <= track - BLOCK {
        phase
    } else {
        cycle - phase
    };
    let spans = vec![
        Span::styled(" --:-- ", Style::new().fg(crate::theme::subtext0(ctx))),
        Span::styled(
            "░".repeat(offset),
            Style::new().fg(crate::theme::surface1(ctx)),
        ),
        Span::styled("█".repeat(BLOCK), Style::new().fg(accent)),
        Span::styled(
            "░".repeat(track - offset - BLOCK),
            Style::new().fg(crate::theme::surface1(ctx)),
        ),
        Span::styled(" --:-- ", Style::new().fg(crate::theme::subtext0(ctx))),
    ];
    Paragraph::new(Line::from(spans)).render(area, buf);
}

pub fn seek_position(area: Rect, column: u16, duration: Duration) -> Option<Duration> {
    if duration.is_zero() || area.width == 0 || !area.contains((column, area.y).into()) {
        return None;
    }
    if area.width < 18 {
        let offset = column.saturating_sub(area.x);
        let denominator = area.width.saturating_sub(1).max(1);
        let ratio = f64::from(offset) / f64::from(denominator);
        return Some(Duration::from_secs_f64(
            duration.as_secs_f64() * ratio.clamp(0.0, 1.0),
        ));
    }

    let bar_x = area.x.saturating_add(7);
    let bar_width = area.width.saturating_sub(13).max(1);
    let offset = column
        .saturating_sub(bar_x)
        .min(bar_width.saturating_sub(1));
    let denominator = bar_width.saturating_sub(1).max(1);
    let ratio = if column < bar_x {
        0.0
    } else if column >= bar_x.saturating_add(bar_width) {
        1.0
    } else {
        f64::from(offset) / f64::from(denominator)
    };
    Some(Duration::from_secs_f64(
        duration.as_secs_f64() * ratio.clamp(0.0, 1.0),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::layout::Rect;

    use super::seek_position;

    #[test]
    fn seek_uses_the_rendered_bar_instead_of_the_full_row() {
        let area = Rect::new(10, 4, 100, 1);
        let duration = Duration::from_secs(200);

        assert_eq!(
            seek_position(area, 17, duration),
            Some(Duration::from_secs(0))
        );
        assert_eq!(
            seek_position(area, 103, duration),
            Some(Duration::from_secs(200))
        );
        // 进度条从 x=17 起、宽 87 列（覆盖 17..=103），真实中点在 60 列附近
        let middle = seek_position(area, 60, duration).unwrap();
        assert!((middle.as_secs_f64() - 100.0).abs() < 2.0);
    }

    #[test]
    fn clicks_on_time_labels_clamp_to_the_nearest_end() {
        let area = Rect::new(0, 0, 80, 1);
        let duration = Duration::from_secs(120);

        assert_eq!(
            seek_position(area, 2, duration),
            Some(Duration::from_secs(0))
        );
        assert_eq!(
            seek_position(area, 76, duration),
            Some(Duration::from_secs(120))
        );
    }
}
