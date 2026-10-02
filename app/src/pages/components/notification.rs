//! 通知 toast 组件
//!
//! 右下角堆叠显示最近几条通知：最新的贴着状态栏，旧的依次向上摞，
//! 间隔 1 行。action 按钮只保留在最新一条上——旧条目的按钮既不可见也
//! 不可点（[`action_url_at`] 只认最新条），画出来只会误导。

use lx_core::events::NotificationLevel;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::context::AppContext;

/// 同屏最多堆叠的 toast 条数（存储上限是 8 条，屏幕上只露最近的几条）。
const VISIBLE_TOASTS: usize = 3;
/// 相邻两条 toast 之间的空行。
const TOAST_GAP: u16 = 1;

/// 单条 toast 的布局参数（按最新→最旧排列）。
struct ToastLayout {
    rect: Rect,
    /// 是否渲染 action 按钮行（只有最新一条为 true）。
    has_action: bool,
}

/// 计算当前应显示的 toast 布局：宽度统一取各条期望值的最大值（右对齐），
/// 高度按各自内容收缩；总高度放不下时从最旧的开始丢弃。
fn toast_layouts(screen: Rect, ctx: &AppContext) -> Option<Vec<ToastLayout>> {
    if !ctx
        .config
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .notification
        .in_app
    {
        return None;
    }
    if screen.width < 12 || screen.height < 4 {
        return None;
    }
    let notifs = ctx.notifications.read().unwrap_or_else(|e| e.into_inner());
    // 从最新到最旧取最多 VISIBLE_TOASTS 条。
    let items: Vec<_> = notifs.iter().rev().take(VISIBLE_TOASTS).collect();
    if items.is_empty() {
        return None;
    }

    let available_width = screen.width.saturating_sub(2);
    // 统一宽度：所有条右对齐且同宽，union 矩形的内区与最新条的内区同 x，
    // `action_url_at` 的命中计算才保持正确。
    let width = items
        .iter()
        .map(|n| desired_width(n))
        .max()
        .unwrap_or(28)
        .min(available_width);
    let content_width = width.saturating_sub(2).max(1) as usize;

    // 逐条算高度（消息行数按屏幕高度收缩），总高度超出可用空间时丢最旧的。
    let frame_rows = screen.height.saturating_sub(1);
    let mut layouts: Vec<ToastLayout> = Vec::with_capacity(items.len());
    let mut used_height = 0u16;
    for (index, notification) in items.iter().enumerate() {
        let has_action = index == 0 && notification.action_url.is_some();
        let height = toast_height(notification, content_width, frame_rows, has_action);
        let gap = u16::from(!layouts.is_empty()) * TOAST_GAP;
        if used_height + gap + height > frame_rows && !layouts.is_empty() {
            break;
        }
        used_height += gap + height;
        layouts.push(ToastLayout {
            rect: Rect::default(),
            has_action,
        });
        let last = layouts.last_mut().expect("just pushed");
        last.rect.height = height;
    }
    if layouts.is_empty() {
        return None;
    }

    // 从下往上落位：最新条贴着状态栏上方，旧的依次向上摞。
    let x = screen.right().saturating_sub(width).saturating_sub(1);
    let mut bottom = screen.bottom().saturating_sub(2).max(screen.y);
    for layout in layouts.iter_mut() {
        layout.rect.y = bottom.saturating_sub(layout.rect.height).max(screen.y);
        layout.rect.x = x;
        layout.rect.width = width;
        bottom = layout.rect.y.saturating_sub(TOAST_GAP);
    }
    Some(layouts)
}

/// 单条 toast 的期望宽度（消息/标题/action 标签的最长显示宽 + 边距）。
fn desired_width(notification: &lx_core::events::Notification) -> u16 {
    let title_width = notification
        .title
        .as_deref()
        .map_or(0, UnicodeWidthStr::width);
    let action_width = notification
        .action_label
        .as_deref()
        .map_or(0, UnicodeWidthStr::width)
        .saturating_add(4);
    UnicodeWidthStr::width(notification.message.as_str())
        .max(title_width)
        .max(action_width)
        .saturating_add(4)
        .clamp(28, 72) as u16
}

/// 单条 toast 的高度：标题行 + 折行后的消息行 + action 行 + 上下边框。
fn toast_height(
    notification: &lx_core::events::Notification,
    content_width: usize,
    frame_rows: u16,
    has_action: bool,
) -> u16 {
    let message_width = UnicodeWidthStr::width(notification.message.as_str()).max(1);
    let title_lines = notification.title.as_deref().map_or(0, |title| {
        UnicodeWidthStr::width(title).div_ceil(content_width) as u16
    });
    let action_lines = u16::from(has_action);
    // 消息行数按屏幕可用高度动态收缩，保证标题与 action 行落在弹窗可视区内
    // —— 否则超长消息会把 action 行挤出弹窗，而命中判定仍在最后一行。
    let chrome_rows = 2u16
        .saturating_add(title_lines)
        .saturating_add(action_lines);
    let max_message_lines = frame_rows.saturating_sub(chrome_rows).max(1);
    (message_width.div_ceil(content_width) as u16)
        .clamp(1, max_message_lines)
        .saturating_add(title_lines)
        .saturating_add(action_lines)
        .saturating_add(2)
        .min(frame_rows)
}

/// 所有可见 toast 的外接矩形（无通知时 `None`）。
///
/// 命中测试用它做第一层过滤；action 按钮的精确命中在 [`action_url_at`]。
pub fn area(screen: Rect, ctx: &AppContext) -> Option<Rect> {
    let layouts = toast_layouts(screen, ctx)?;
    let first = layouts.first()?.rect;
    layouts
        .iter()
        .skip(1)
        .try_fold(first, |acc, layout| Some(acc.union(layout.rect)))
}

pub fn action_url_at(
    notification_area: Rect,
    column: u16,
    row: u16,
    ctx: &AppContext,
) -> Option<String> {
    let notification = ctx
        .notifications
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .back()
        .cloned()?;
    let url = notification.action_url?;
    let label = notification.action_label?;
    // 最新一条贴着外接矩形的底边，它的 action 行就是外接矩形内区的最后一行。
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(notification_area);
    if inner.height == 0 || row != inner.bottom().saturating_sub(1) {
        return None;
    }
    let start = inner.x;
    let end = start
        .saturating_add(UnicodeWidthStr::width(label.as_str()) as u16)
        .saturating_add(4)
        .min(inner.right());
    (column >= start && column < end).then_some(url)
}

pub fn render(screen: Rect, buf: &mut Buffer, ctx: &AppContext) {
    let Some(layouts) = toast_layouts(screen, ctx) else {
        return;
    };
    let lifetime = ctx.notification_timeout();
    let notifs = ctx.notifications.read().unwrap_or_else(|e| e.into_inner());
    // layouts 与 notifs.iter().rev() 同序。
    for (layout, notification) in layouts.iter().zip(notifs.iter().rev()) {
        render_toast(layout, notification, buf, ctx, lifetime);
    }
}

fn render_toast(
    layout: &ToastLayout,
    notification: &lx_core::events::Notification,
    buf: &mut Buffer,
    ctx: &AppContext,
    lifetime: std::time::Duration,
) {
    let (label, level_color) = match notification.level {
        NotificationLevel::Info => ("信息", crate::theme::blue(ctx)),
        NotificationLevel::Success => ("成功", crate::theme::green(ctx)),
        NotificationLevel::Warn => ("警告", crate::theme::yellow(ctx)),
        NotificationLevel::Error => ("错误", crate::theme::red(ctx)),
    };
    let faded = notification.age() >= lifetime.mul_f32(0.75);
    let text_color = if faded {
        crate::theme::muted(ctx)
    } else {
        crate::theme::text(ctx)
    };
    let style = Style::new().bg(crate::theme::surface0(ctx)).fg(text_color);

    let area = layout.rect;
    Clear.render(area, buf);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(level_color))
        .style(style)
        .title(format!(" {} · {} ", label, notification.timestamp()));
    let inner = block.inner(area);
    block.render(area, buf);
    let mut lines = Vec::new();
    if let Some(title) = notification.title.as_ref() {
        lines.push(Line::from(Span::styled(
            title.as_str(),
            Style::new().fg(level_color),
        )));
    }
    lines.push(Line::from(notification.message.as_str()));
    // action 按钮只画在最新一条上：旧条目的按钮不可点（见 action_url_at）。
    if layout.has_action
        && let (Some(label), Some(_)) = (
            notification.action_label.as_ref(),
            notification.action_url.as_ref(),
        )
    {
        lines.push(Line::from(Span::styled(
            format!("[ {label} ]"),
            Style::new().fg(level_color),
        )));
    }
    Paragraph::new(lines)
        .style(style)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
}
