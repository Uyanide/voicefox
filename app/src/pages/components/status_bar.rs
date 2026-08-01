//! 底部状态栏

use lx_core::model::config::StatusBarItem;
use lx_core::model::source::{PlayerState, Quality};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::context::AppContext;

pub fn render(area: Rect, buf: &mut Buffer, ctx: &AppContext, sort_status: Option<&'static str>) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let background = crate::theme::mantle(ctx);
    Block::default()
        .style(Style::new().bg(background).fg(crate::theme::text(ctx)))
        .render(area, buf);

    let state = *ctx.player_state.borrow();
    let current_song = ctx.current_song.read().unwrap();
    let position = *ctx.position.borrow();
    let duration = *ctx.duration.borrow();
    let volume = ctx.player.volume();
    let (queue, queue_index) = ctx.playlist.snapshot();
    let (quality, status_bar_items) = {
        let config = ctx.config.read().unwrap();
        (config.player.quality, config.ui.status_bar_items.clone())
    };

    let (state_text, state_color) = match state {
        PlayerState::Playing => ("播放", crate::theme::green(ctx)),
        PlayerState::Paused => ("暂停", crate::theme::yellow(ctx)),
        PlayerState::Loading => ("缓冲", crate::theme::sapphire(ctx)),
        PlayerState::Stopped => ("停止", crate::theme::overlay1(ctx)),
        PlayerState::Idle => ("空闲", crate::theme::overlay1(ctx)),
    };
    let time = if duration.is_zero() {
        format_duration(position)
    } else {
        format!(
            "{}/{}",
            format_duration(position),
            format_duration(duration)
        )
    };
    let song = current_song.as_ref().map_or_else(
        || "voicefox".to_string(),
        |song| {
            if song.singer.trim().is_empty() {
                song.name.clone()
            } else {
                format!("{} - {}", song.name, song.singer)
            }
        },
    );
    let source = current_song
        .as_ref()
        .map(|song| {
            let js_index = *ctx.play_js_source_index.lock().unwrap();
            js_index
                .and_then(|index| ctx.source_manager.js_source_name(index))
                .or_else(|| {
                    ctx.source_manager
                        .get(song.source)
                        .map(|source| source.name().to_string())
                })
                .unwrap_or_else(|| song.source.as_str().to_string())
        })
        .unwrap_or_else(|| "-".to_string());
    let source_online = ctx.source_manager.has_js_source();
    let queue_position = if queue.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", queue_index.saturating_add(1), queue.len())
    };
    let mode = ctx.playlist.mode().label();

    let total_width = area.width as usize;
    let mut used_width = 0;
    let mut spans = Vec::new();

    for item in status_bar_items {
        let remaining = remaining_segment_width(used_width, total_width, spans.is_empty());
        let segment = match item {
            StatusBarItem::State => Some((
                format!(" {} ", state_text),
                Style::new()
                    .fg(state_color)
                    .bg(crate::theme::surface0(ctx))
                    .add_modifier(Modifier::BOLD),
            )),
            StatusBarItem::Source => {
                let source_width = remaining
                    .saturating_sub(UnicodeWidthStr::width("音源 "))
                    .min(match area.width {
                        0..=49 => 8,
                        50..=89 => 14,
                        _ => 20,
                    });
                (source_width > 0).then(|| {
                    (
                        format!("音源 {}", truncate(&source, source_width)),
                        Style::new()
                            .fg(crate::theme::peach(ctx))
                            .bg(background)
                            .add_modifier(Modifier::BOLD),
                    )
                })
            }
            StatusBarItem::Sort => sort_status.map(|sort_status| {
                (
                    format!("排序 {} (s)", sort_status),
                    Style::new()
                        .fg(crate::theme::yellow(ctx))
                        .bg(background)
                        .add_modifier(Modifier::BOLD),
                )
            }),
            StatusBarItem::Song => (remaining > 0).then(|| {
                (
                    truncate(&song, remaining.min(28)),
                    Style::new()
                        .fg(crate::theme::text(ctx))
                        .bg(background)
                        .add_modifier(Modifier::BOLD),
                )
            }),
            StatusBarItem::Time => Some((
                time.clone(),
                Style::new().fg(crate::theme::subtext1(ctx)).bg(background),
            )),
            StatusBarItem::Volume => Some((
                format!("音量 {}%", volume),
                Style::new().fg(crate::theme::sky(ctx)).bg(background),
            )),
            StatusBarItem::PlayMode => Some((
                mode.to_string(),
                Style::new().fg(crate::theme::lavender(ctx)).bg(background),
            )),
            StatusBarItem::Quality => Some((
                quality_label(quality).to_string(),
                Style::new().fg(crate::theme::peach(ctx)).bg(background),
            )),
            StatusBarItem::Queue => Some((
                format!("队列 {}", queue_position),
                Style::new().fg(crate::theme::teal(ctx)).bg(background),
            )),
            StatusBarItem::JsSourceState => Some((
                if source_online {
                    "自定义音源在线".to_string()
                } else {
                    "自定义音源离线".to_string()
                },
                Style::new()
                    .fg(if source_online {
                        crate::theme::green(ctx)
                    } else {
                        crate::theme::maroon(ctx)
                    })
                    .bg(background),
            )),
        };
        if let Some((text, style)) = segment {
            append_segment(
                &mut spans,
                &mut used_width,
                total_width,
                text,
                style,
                ctx,
                background,
            );
        }
    }

    Paragraph::new(Line::from(spans))
        .style(Style::new().bg(background))
        .render(Rect::new(area.x, area.y, area.width, 1), buf);
}

fn separator(ctx: &AppContext, background: ratatui::style::Color) -> Span<'static> {
    Span::styled(
        "  ·  ",
        Style::new().fg(crate::theme::overlay0(ctx)).bg(background),
    )
}

fn separator_width() -> usize {
    UnicodeWidthStr::width("  ·  ")
}

#[allow(clippy::too_many_arguments)]
fn append_segment<'a>(
    spans: &mut Vec<Span<'a>>,
    used_width: &mut usize,
    total_width: usize,
    text: String,
    style: Style,
    ctx: &AppContext,
    background: ratatui::style::Color,
) -> bool {
    let text_width = UnicodeWidthStr::width(text.as_str());
    let separator_width = if spans.is_empty() {
        0
    } else {
        separator_width()
    };
    let required = separator_width.saturating_add(text_width);
    if text_width == 0 || used_width.saturating_add(required) > total_width {
        return false;
    }
    if !spans.is_empty() {
        spans.push(separator(ctx, background));
    }
    spans.push(Span::styled(text, style));
    *used_width += required;
    true
}

fn remaining_segment_width(used_width: usize, total_width: usize, first: bool) -> usize {
    total_width.saturating_sub(used_width + if first { 0 } else { separator_width() })
}

fn truncate(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut result = String::new();
    let mut rendered = 0;
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if rendered + character_width > width - 1 {
            break;
        }
        result.push(character);
        rendered += character_width;
    }
    result.push('…');
    result
}

fn quality_label(quality: Quality) -> &'static str {
    match quality {
        Quality::Low128 => "128K",
        Quality::High320 => "320K",
        Quality::Flac => "FLAC",
        Quality::Flac24 => "Hi-Res",
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
