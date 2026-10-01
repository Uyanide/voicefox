//! 顶部信息栏：左侧播放状态与时间，中间歌名 / 歌手 - 专辑。
//!
//! 音量、播放模式、音源、自定义音源状态只在底部状态栏展示（那里可点击、
//! 可交互）。此前顶栏右列重复渲染同样的信息还伴随 "SOUR" 这类截断，
//! 已整体移除，信息口径以底栏为准。

use lx_core::model::source::PlayerState;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::context::AppContext;
use crate::fmt::format_duration;
use crate::pages::components::text::truncate_width;

pub fn render(area: Rect, buf: &mut Buffer, ctx: &AppContext) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(crate::theme::border(ctx)));
    let inner = block.inner(area);
    block.render(area, buf);
    if inner.height == 0 {
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24.min(inner.width / 3)),
            Constraint::Min(10),
        ])
        .split(inner);
    let state = *ctx.player_state.borrow();
    // 状态用词与底部状态栏保持一致（播放/暂停/缓冲…），避免同一状态两个名字。
    let state_label = match state {
        PlayerState::Playing => "播放",
        PlayerState::Paused => "暂停",
        PlayerState::Loading => "缓冲",
        PlayerState::Stopped => "停止",
        PlayerState::Idle => "空闲",
    };
    let position = *ctx.position.borrow();
    let duration = *ctx.duration.borrow();
    Paragraph::new(vec![
        Line::from(Span::styled(
            format!("[{state_label}]"),
            Style::new()
                .fg(crate::theme::yellow(ctx))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "{} / {}",
            format_duration(position),
            format_duration(duration)
        )),
    ])
    .render(columns[0], buf);

    let song = ctx.current_song.read().unwrap_or_else(|e| e.into_inner());
    let (title, detail) = song.as_ref().map_or_else(
        || ("暂无播放".to_string(), "使用搜索添加歌曲".to_string()),
        |song| {
            (
                song.name.clone(),
                format!(
                    "{}{}",
                    song.singer,
                    if song.album_name.trim().is_empty() {
                        String::new()
                    } else {
                        format!(" - {}", song.album_name)
                    }
                ),
            )
        },
    );
    // 居中列宽由布局保证；这里先按显示宽度截断，超长的部分带省略号，
    // 完整歌名仍可在详情/右键菜单看到。
    let title_width = columns[1].width as usize;
    let title = truncate_width(&title, title_width).into_owned();
    let detail = truncate_width(&detail, title_width).into_owned();
    Paragraph::new(vec![
        Line::from(Span::styled(
            title,
            Style::new()
                .fg(crate::theme::text(ctx))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            detail,
            Style::new().fg(crate::theme::muted(ctx)),
        )),
    ])
    .alignment(Alignment::Center)
    .render(columns[1], buf);
}
