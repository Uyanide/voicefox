//! rmpc 风格播放队列页面。

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::{AppAction, Notification};
use lx_core::keybinding::{Action, KeybindingResolver};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::context::AppContext;
use crate::cover::{CoverGeometry, CoverRenderer, CoverState};

/// “D 清空整个队列”的确认窗口：首次按下武装，窗口内再按一次才执行。
const CLEAR_QUEUE_CONFIRM_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueEditCommand {
    MoveUp,
    MoveDown,
    RemoveSelected,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResizeTarget {
    WideColumns,
    WideCoverLyrics,
    NarrowQueueLyrics,
}

const DEFAULT_WIDE_COLUMNS_RATIO: f32 = 0.36;
const DEFAULT_WIDE_COVER_RATIO: f32 = 0.52;
const DEFAULT_NARROW_QUEUE_RATIO: f32 = 0.62;
const RESIZE_GRAB_RADIUS: u16 = 1;

fn clamp_ratio(value: f32, min: f32, max: f32) -> f32 {
    value.clamp(min, max)
}

fn queue_edit_command(key: &KeyEvent) -> Option<QueueEditCommand> {
    match (key.modifiers, key.code) {
        (KeyModifiers::SHIFT, KeyCode::Up)
        | (KeyModifiers::SHIFT, KeyCode::Char('k' | 'K'))
        | (KeyModifiers::NONE, KeyCode::Char('K')) => Some(QueueEditCommand::MoveUp),
        (KeyModifiers::SHIFT, KeyCode::Down)
        | (KeyModifiers::SHIFT, KeyCode::Char('j' | 'J'))
        | (KeyModifiers::NONE, KeyCode::Char('J')) => Some(QueueEditCommand::MoveDown),
        (KeyModifiers::NONE, KeyCode::Char('d') | KeyCode::Delete) => {
            Some(QueueEditCommand::RemoveSelected)
        }
        (KeyModifiers::SHIFT, KeyCode::Char('d' | 'D'))
        | (KeyModifiers::NONE, KeyCode::Char('D')) => Some(QueueEditCommand::Clear),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct MainLayout {
    wide: bool,
    left: Rect,
    queue: Rect,
    cover: Rect,
    lyric: Rect,
    cover_geometry: Option<CoverGeometry>,
}

pub struct MainPage {
    selected: usize,
    scroll: usize,
    dragging: Option<usize>,
    cover: CoverRenderer,
    /// 队列内快速过滤；过滤只改变可见集合，不修改真实播放队列。
    queue_filter: String,
    queue_filter_active: bool,
    lyric_fullscreen: bool,
    /// 宽屏：左侧封面/歌词列占整个内容区的比例。
    wide_columns_ratio: f32,
    /// 宽屏：左侧封面占左栏高度的比例。
    wide_cover_ratio: f32,
    /// 窄屏：上方队列占整个内容区高度的比例。
    narrow_queue_ratio: f32,
    /// 当前鼠标正在拖动的布局分隔线。
    resize_target: Option<ResizeTarget>,
    /// 拖动期间只更新视觉预览，不立即提交到稳定布局状态。
    resize_preview: Option<(ResizeTarget, f32)>,
    /// 首次绘制前保持旧版封面高度策略；之后由用户拖拽接管。
    layout_initialized: bool,
    /// “D 清空整个队列”的武装时刻，确认窗口外或 Esc 后解除
    clear_armed: Option<Instant>,
}

impl MainPage {
    pub fn new(cover: CoverRenderer) -> Self {
        Self {
            selected: 0,
            scroll: 0,
            dragging: None,
            cover,
            queue_filter: String::new(),
            queue_filter_active: false,
            lyric_fullscreen: false,
            wide_columns_ratio: DEFAULT_WIDE_COLUMNS_RATIO,
            wide_cover_ratio: DEFAULT_WIDE_COVER_RATIO,
            narrow_queue_ratio: DEFAULT_NARROW_QUEUE_RATIO,
            resize_target: None,
            resize_preview: None,
            layout_initialized: false,
            clear_armed: None,
        }
    }

    /// 释放已解码的封面
    pub fn release_cover_image(&mut self) {
        self.cover.sync(None);
    }

    /// 收取封面后台线程返回的解码与编码结果，返回是否需要重绘
    pub fn poll_cover(&mut self) -> bool {
        self.cover.poll()
    }

    /// 终端尺寸变化后重新读取单元格的像素尺寸
    pub fn refresh_cover_font_size(&mut self) -> bool {
        self.cover.refresh_font_size()
    }

    /// 强制把封面重新传输给终端，用于终端已丢弃此前图片的场合
    pub fn force_cover_reload(&mut self) {
        self.cover.force_reload();
    }

    fn filtered_indices(&self, songs: &[lx_core::model::song::SongInfo]) -> Vec<usize> {
        let query = self.queue_filter.trim().to_lowercase();
        if query.is_empty() {
            return (0..songs.len()).collect();
        }
        songs
            .iter()
            .enumerate()
            .filter_map(|(i, song)| {
                let haystack =
                    format!("{} {} {}", song.name, song.singer, song.album_name).to_lowercase();
                haystack.contains(&query).then_some(i)
            })
            .collect()
    }

    pub fn handle_input(
        &mut self,
        key: &KeyEvent,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> AppAction {
        if self.resize_target.is_some() && key.code == KeyCode::Esc {
            // Esc 取消本次视觉预览，不污染已提交布局。
            self.resize_target = None;
            self.resize_preview = None;
            return AppAction::None;
        }

        let len = {
            let songs = ctx.playlist.borrow();
            let current = ctx.playlist.current_index();
            if self.selected >= songs.len() {
                self.selected = current.min(songs.len().saturating_sub(1));
            }
            songs.len()
        };

        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }

        if !self.queue_filter_active
            && key.modifiers == KeyModifiers::NONE
            && matches!(key.code, KeyCode::Char('v') | KeyCode::Char('V'))
        {
            self.lyric_fullscreen = !self.lyric_fullscreen;
            return AppAction::None;
        }

        if self.queue_filter_active {
            match key.code {
                KeyCode::Esc => {
                    self.queue_filter_active = false;
                    self.queue_filter.clear();
                    self.selected = 0;
                    self.scroll = 0;
                    return AppAction::None;
                }
                KeyCode::Enter => {
                    self.queue_filter_active = false;
                    return AppAction::None;
                }
                KeyCode::Backspace => {
                    self.queue_filter.pop();
                    self.selected = self
                        .filtered_indices(&ctx.playlist.borrow())
                        .first()
                        .copied()
                        .unwrap_or(0);
                    self.scroll = 0;
                    return AppAction::None;
                }
                KeyCode::Char(ch)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    self.queue_filter.push(ch);
                    self.selected = self
                        .filtered_indices(&ctx.playlist.borrow())
                        .first()
                        .copied()
                        .unwrap_or(0);
                    self.scroll = 0;
                    return AppAction::None;
                }
                _ => {}
            }
        } else if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Char('/') {
            self.queue_filter_active = true;
            self.scroll = 0;
            return AppAction::None;
        }

        if let Some(command) = queue_edit_command(key) {
            return match command {
                QueueEditCommand::MoveUp => {
                    if self.selected > 0 {
                        ctx.playlist.move_item(self.selected, self.selected - 1);
                        self.selected -= 1;
                    }
                    AppAction::None
                }
                QueueEditCommand::MoveDown => {
                    if self.selected + 1 < len {
                        ctx.playlist.move_item(self.selected, self.selected + 1);
                        self.selected += 1;
                    }
                    AppAction::None
                }
                QueueEditCommand::RemoveSelected => self.remove_at(self.selected, ctx),
                QueueEditCommand::Clear => {
                    // 二次确认：首次按下只武装并提示，确认窗口内再按一次才清空
                    let now = Instant::now();
                    let confirmed = matches!(
                        self.clear_armed,
                        Some(armed_at) if now.duration_since(armed_at) <= CLEAR_QUEUE_CONFIRM_WINDOW
                    );
                    if !confirmed {
                        self.clear_armed = Some(now);
                        return AppAction::ShowNotification(Notification::warning(
                            "再按一次 D 确认清空队列，Esc 取消",
                        ));
                    }
                    self.clear_armed = None;
                    ctx.playlist.clear();
                    ctx.stop_player();
                    ctx.cover_service.clear();
                    ctx.lyric_service.clear();
                    *ctx.current_song.write().unwrap_or_else(|e| e.into_inner()) = None;
                    self.selected = 0;
                    self.scroll = 0;
                    AppAction::None
                }
            };
        }

        if matches!(
            (key.modifiers, key.code),
            (KeyModifiers::NONE, KeyCode::Esc)
        ) {
            // Esc 取消“清空整个队列”的武装状态（其余行为不变）
            self.clear_armed = None;
        }

        if let Some(action) = resolver.resolve_page("main", key) {
            match action {
                Action::ListSelectUp => {
                    let songs = ctx.playlist.borrow();
                    let visible = self.filtered_indices(&songs);
                    if !visible.is_empty() {
                        let pos = visible
                            .iter()
                            .position(|&i| i == self.selected)
                            .unwrap_or(0);
                        self.selected = if pos == 0 {
                            if ctx
                                .config
                                .read()
                                .unwrap_or_else(|e| e.into_inner())
                                .ui
                                .wrap_navigation
                            {
                                *visible.last().unwrap()
                            } else {
                                visible[0]
                            }
                        } else {
                            visible[pos - 1]
                        };
                    }
                    return AppAction::None;
                }
                Action::ListSelectDown => {
                    let songs = ctx.playlist.borrow();
                    let visible = self.filtered_indices(&songs);
                    if !visible.is_empty() {
                        let pos = visible
                            .iter()
                            .position(|&i| i == self.selected)
                            .unwrap_or(0);
                        self.selected = if pos + 1 < visible.len() {
                            visible[pos + 1]
                        } else if ctx
                            .config
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .ui
                            .wrap_navigation
                        {
                            visible[0]
                        } else {
                            *visible.last().unwrap()
                        };
                    }
                    return AppAction::None;
                }
                Action::ListSelectFirst => {
                    self.selected = 0;
                    return AppAction::None;
                }
                Action::ListSelectLast => {
                    self.selected = len.saturating_sub(1);
                    return AppAction::None;
                }
                Action::ListPageUp => {
                    self.selected = self.selected.saturating_sub(5);
                    return AppAction::None;
                }
                Action::ListPageDown => {
                    self.selected = (self.selected + 5).min(len.saturating_sub(1));
                    return AppAction::None;
                }
                Action::ListActivate => {
                    if self.selected < len {
                        let (songs, _) = ctx.playlist.snapshot();
                        return AppAction::PlaySong {
                            songs,
                            index: self.selected,
                        };
                    }
                    return AppAction::None;
                }
                Action::ListToggleFavorite => {
                    let song = ctx.playlist.borrow().get(self.selected).cloned();
                    if let Some(song) = song {
                        return AppAction::ToggleFavoriteSong(Box::new(song));
                    }
                    return AppAction::None;
                }
                Action::ListDownload => {
                    if let Some(song) = ctx.playlist.borrow().get(self.selected).cloned() {
                        return AppAction::DownloadSong(Box::new(song));
                    }
                    return AppAction::None;
                }
                _ => {}
            }
        }

        match (key.modifiers, key.code) {
            // 裸 Up/Down 在主页被 main.rs 的音量快捷键拦截（见 KEYBINDINGS.md），
            // 这里不再重复处理；列表移动走 Action::ListSelectUp/Down 绑定。
            (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::NONE, KeyCode::Char('g')) => {
                self.selected = 0;
            }
            (KeyModifiers::NONE, KeyCode::End)
            | (KeyModifiers::NONE, KeyCode::Char('G'))
            | (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                self.selected = len.saturating_sub(1);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.selected = self.selected.saturating_sub(5);
            }
            (KeyModifiers::CONTROL, KeyCode::Char('d'))
            | (KeyModifiers::NONE, KeyCode::PageDown) => {
                self.selected = (self.selected + 5).min(len.saturating_sub(1));
            }
            _ if super::is_song_activation_key(key) && self.selected < len => {
                let (songs, _) = ctx.playlist.snapshot();
                return AppAction::PlaySong {
                    songs,
                    index: self.selected,
                };
            }
            (KeyModifiers::NONE, KeyCode::Char('f')) => {
                let song = ctx.playlist.borrow().get(self.selected).cloned();
                if let Some(song) = song {
                    return AppAction::ToggleFavoriteSong(Box::new(song));
                }
            }
            _ => {}
        }
        AppAction::None
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        if self.lyric_fullscreen {
            super::components::lyric::render(area, buf, ctx);
            return;
        }
        let layout = self.compute_layout(area, ctx);
        if layout.wide {
            if let Some(geometry) = layout.cover_geometry {
                if layout.cover.height > 0 {
                    self.render_cover(layout.cover, buf, ctx, geometry);
                }
            }
            super::components::lyric::render(layout.lyric, buf, ctx);
            self.render_queue(layout.queue, buf, ctx);
        } else {
            self.render_queue(layout.queue, buf, ctx);
            super::components::lyric::render(layout.lyric, buf, ctx);
        }
        self.render_resize_dividers(&layout, buf, ctx);
    }

    fn compute_layout(&mut self, area: Rect, ctx: &AppContext) -> MainLayout {
        let effective_ratio = |target: ResizeTarget, committed: f32| {
            self.resize_preview
                .filter(|(preview_target, _)| *preview_target == target)
                .map(|(_, ratio)| ratio)
                .unwrap_or(committed)
        };

        if area.width >= 72 {
            let columns_ratio = effective_ratio(ResizeTarget::WideColumns, self.wide_columns_ratio);
            let left_width = ((area.width as f32) * columns_ratio).round() as u16;
            let left_width = left_width.clamp(24, area.width.saturating_sub(24).max(24));
            let left = Rect::new(area.x, area.y, left_width.min(area.width), area.height);
            let queue = Rect::new(
                left.right().min(area.right()),
                area.y,
                area.right().saturating_sub(left.right()),
                area.height,
            );
            let geometry = ctx
                .config
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .ui
                .show_cover
                .then(|| {
                    CoverGeometry::from_font_size(
                        self.cover.font_size(),
                        ctx.cover_service.image_aspect(),
                    )
                });
            if !self.layout_initialized {
                if let Some(geometry) = geometry {
                    let max_cover = left
                        .height
                        .saturating_sub(super::components::lyric::MIN_HEIGHT);
                    let old_cover = geometry.box_height(left.width, max_cover);
                    if left.height > 0 {
                        self.wide_cover_ratio =
                            clamp_ratio(old_cover as f32 / left.height as f32, 0.20, 0.85);
                    }
                }
                self.layout_initialized = true;
            }
            let max_cover = left
                .height
                .saturating_sub(super::components::lyric::MIN_HEIGHT);
            let cover_ratio = effective_ratio(ResizeTarget::WideCoverLyrics, self.wide_cover_ratio);
            let cover_height = if geometry.is_some() {
                ((left.height as f32) * cover_ratio)
                    .round()
                    .clamp(0.0, f32::from(max_cover)) as u16
            } else {
                0
            };
            let cover = Rect::new(left.x, left.y, left.width, cover_height);
            let lyric = Rect::new(
                left.x,
                cover.bottom().min(left.bottom()),
                left.width,
                left.height.saturating_sub(cover.height),
            );
            MainLayout {
                wide: true,
                left,
                queue,
                cover,
                lyric,
                cover_geometry: geometry,
            }
        } else {
            let min_queue = 7.min(area.height);
            let min_lyric = 5.min(area.height.saturating_sub(min_queue));
            let queue_ratio =
                effective_ratio(ResizeTarget::NarrowQueueLyrics, self.narrow_queue_ratio);
            let queue_height = ((area.height as f32) * queue_ratio).round() as u16;
            let max_queue = area.height.saturating_sub(min_lyric);
            let queue_height = queue_height.clamp(min_queue, max_queue.max(min_queue));
            let queue = Rect::new(area.x, area.y, area.width, queue_height.min(area.height));
            let lyric = Rect::new(
                area.x,
                queue.bottom().min(area.bottom()),
                area.width,
                area.height.saturating_sub(queue.height),
            );
            MainLayout {
                wide: false,
                left: Rect::default(),
                queue,
                cover: Rect::default(),
                lyric,
                cover_geometry: None,
            }
        }
    }

    fn render_resize_dividers(&self, layout: &MainLayout, buf: &mut Buffer, ctx: &AppContext) {
        let style = Style::new().fg(crate::theme::accent(ctx));
        if layout.wide {
            if layout.left.width > 0 && layout.queue.width > 0 {
                let x = layout.left.right().saturating_sub(1);
                for y in layout.left.y..layout.left.bottom() {
                    buf.set_string(x, y, "│", style);
                }
            }
            if layout.cover.height > 0 && layout.lyric.height > 0 {
                let y = layout.cover.bottom().saturating_sub(1);
                for x in layout.left.x..layout.left.right() {
                    buf.set_string(x, y, "─", style);
                }
            }
        } else if layout.queue.height > 0 && layout.lyric.height > 0 {
            let y = layout.queue.bottom().saturating_sub(1);
            for x in layout.queue.x..layout.queue.right() {
                buf.set_string(x, y, "─", style);
            }
        }
    }

    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        area: Rect,
        ctx: &AppContext,
        activate: bool,
    ) -> AppAction {
        let layout = self.compute_layout(area, ctx);
        if let Some(target) = self.resize_target {
            match event.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    self.update_resize_preview(target, event, area, &layout);
                    return AppAction::None;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.commit_resize();
                    return AppAction::None;
                }
                // Resize 会话期间其它鼠标事件不能穿透到队列，避免拖动时误触。
                _ => return AppAction::None,
            }
        } else if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(target) = self.resize_target_at(event, &layout) {
                self.resize_target = Some(target);
                self.resize_preview = Some((target, self.committed_ratio(target)));
                return AppAction::None;
            }
        }

        let scroll_amount = ctx
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ui
            .scroll_amount
            .max(1);
        let mut play_songs = None;
        let mut drag_target = None;
        {
            // 只读阶段：从队列快照中取出本次事件需要的少量信息。
            let songs = ctx.playlist.borrow();
            let current = ctx.playlist.current_index();
            match event.kind {
                MouseEventKind::ScrollUp => {
                    self.dragging = None;
                    self.selected = self.selected.saturating_sub(scroll_amount);
                }
                MouseEventKind::ScrollDown => {
                    self.dragging = None;
                    self.selected =
                        (self.selected + scroll_amount).min(songs.len().saturating_sub(1));
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let index = if self.queue_filter.is_empty() {
                        queue_index_at(event, layout.queue, self.scroll, songs.len())
                    } else {
                        queue_index_at_filtered(
                            event,
                            layout.queue,
                            self.scroll,
                            &self.filtered_indices(&songs),
                        )
                    };
                    if let Some(index) = index {
                        self.selected = index;
                        self.dragging = Some(index);
                        if activate {
                            play_songs = Some(songs.to_vec());
                        }
                    } else {
                        self.dragging = None;
                        self.selected = current.min(songs.len().saturating_sub(1));
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some(from) = self.dragging {
                        drag_target = if self.queue_filter.is_empty() {
                            queue_index_at(event, layout.queue, self.scroll, songs.len())
                        } else {
                            queue_index_at_filtered(
                                event,
                                layout.queue,
                                self.scroll,
                                &self.filtered_indices(&songs),
                            )
                        }
                        .filter(|&target| target != from);
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.dragging = None;
                }
                _ => {}
            }
        }
        // 借用已释放，写操作不会与读锁互相等待。
        if let (Some(from), Some(target)) = (self.dragging, drag_target) {
            ctx.playlist.move_item(from, target);
            self.selected = target;
            self.dragging = Some(target);
        }
        if let Some(songs) = play_songs {
            return AppAction::PlaySong {
                songs,
                index: self.selected,
            };
        }
        AppAction::None
    }

    pub fn context_song_at(
        &mut self,
        event: MouseEvent,
        area: Rect,
        ctx: &AppContext,
    ) -> Option<(Vec<lx_core::model::song::SongInfo>, usize)> {
        let songs = ctx.playlist.borrow();
        let layout = self.compute_layout(area, ctx);
        let index = queue_index_at(event, layout.queue, self.scroll, songs.len())?;
        self.selected = index;
        self.dragging = None;
        Some((songs.to_vec(), index))
    }

    pub fn remove_at(&mut self, index: usize, ctx: &AppContext) -> AppAction {
        let songs = ctx.playlist.borrow();
        let current = ctx.playlist.current_index();
        if index >= songs.len() {
            return AppAction::None;
        }
        let removing_current = index == current;
        drop(songs);
        ctx.playlist.remove(index);
        let remaining = ctx.playlist.borrow();
        let next = ctx.playlist.current_index();
        self.selected = index.min(remaining.len().saturating_sub(1));
        self.scroll = self.scroll.min(remaining.len().saturating_sub(1));
        if !removing_current {
            return AppAction::None;
        }
        if remaining.is_empty() {
            ctx.stop_player();
            ctx.cover_service.clear();
            ctx.lyric_service.clear();
            *ctx.current_song.write().unwrap_or_else(|e| e.into_inner()) = None;
            AppAction::None
        } else {
            AppAction::PlaySong {
                songs: remaining.to_vec(),
                index: next,
            }
        }
    }

    fn resize_target_at(&self, event: MouseEvent, layout: &MainLayout) -> Option<ResizeTarget> {
        let x = event.column;
        let y = event.row;
        if layout.wide {
            let vertical = layout.left.right().saturating_sub(1);
            if x.abs_diff(vertical) <= RESIZE_GRAB_RADIUS
                && y >= layout.left.y
                && y < layout.left.bottom()
            {
                return Some(ResizeTarget::WideColumns);
            }
            let horizontal = layout.cover.bottom().saturating_sub(1);
            if layout.cover.height > 0
                && layout.lyric.height > 0
                && y.abs_diff(horizontal) <= RESIZE_GRAB_RADIUS
                && x >= layout.left.x
                && x < layout.left.right()
            {
                return Some(ResizeTarget::WideCoverLyrics);
            }
        } else {
            let horizontal = layout.queue.bottom().saturating_sub(1);
            if layout.queue.height > 0
                && layout.lyric.height > 0
                && y.abs_diff(horizontal) <= RESIZE_GRAB_RADIUS
                && x >= layout.queue.x
                && x < layout.queue.right()
            {
                return Some(ResizeTarget::NarrowQueueLyrics);
            }
        }
        None
    }

    fn committed_ratio(&self, target: ResizeTarget) -> f32 {
        match target {
            ResizeTarget::WideColumns => self.wide_columns_ratio,
            ResizeTarget::WideCoverLyrics => self.wide_cover_ratio,
            ResizeTarget::NarrowQueueLyrics => self.narrow_queue_ratio,
        }
    }

    fn clamp_resize_ratio(target: ResizeTarget, ratio: f32) -> f32 {
        match target {
            ResizeTarget::WideColumns => clamp_ratio(ratio, 0.22, 0.78),
            ResizeTarget::WideCoverLyrics => clamp_ratio(ratio, 0.15, 0.85),
            ResizeTarget::NarrowQueueLyrics => clamp_ratio(ratio, 0.20, 0.80),
        }
    }

    fn update_resize_preview(
        &mut self,
        target: ResizeTarget,
        event: MouseEvent,
        area: Rect,
        layout: &MainLayout,
    ) {
        let raw_ratio = match target {
            ResizeTarget::WideColumns if area.width > 0 => {
                (event.column.saturating_sub(area.x) as f32) / area.width as f32
            }
            ResizeTarget::WideCoverLyrics if layout.left.height > 0 => {
                (event.row.saturating_sub(layout.left.y) as f32) / layout.left.height as f32
            }
            ResizeTarget::NarrowQueueLyrics if area.height > 0 => {
                (event.row.saturating_sub(area.y) as f32) / area.height as f32
            }
            _ => return,
        };
        self.resize_preview = Some((target, Self::clamp_resize_ratio(target, raw_ratio)));
    }

    fn commit_resize(&mut self) {
        if let Some((target, ratio)) = self.resize_preview.take() {
            match target {
                ResizeTarget::WideColumns => self.wide_columns_ratio = ratio,
                ResizeTarget::WideCoverLyrics => self.wide_cover_ratio = ratio,
                ResizeTarget::NarrowQueueLyrics => self.narrow_queue_ratio = ratio,
            }
        }
        self.resize_target = None;
    }

    fn render_queue(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let accent = crate::theme::accent(ctx);
        // 借用队列快照，每帧渲染不再复制整张播放列表。
        let songs = ctx.playlist.borrow();
        let current = ctx.playlist.current_index();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::border(ctx)))
            .title(
                if self.queue_filter_active || !self.queue_filter.is_empty() {
                    format!(" 队列 · {} 歌曲 · /{} ", songs.len(), self.queue_filter)
                } else {
                    format!(" 队列 · {} 歌曲 · /筛选 ", songs.len())
                },
            );
        let inner = block.inner(area);
        block.render(area, buf);
        if songs.is_empty() {
            Paragraph::new("队列为空")
                .style(Style::new().fg(crate::theme::muted(ctx)))
                .render(inner, buf);
            return;
        }
        self.selected = self.selected.min(songs.len().saturating_sub(1));
        if inner.height == 0 {
            return;
        }

        Paragraph::new(Line::from(Span::styled(
            super::components::song_table::header(inner.width),
            Style::new()
                .fg(crate::theme::muted(ctx))
                .add_modifier(Modifier::BOLD),
        )))
        .render(Rect::new(inner.x, inner.y, inner.width, 1), buf);
        let list = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        let visible = list.height as usize;
        if visible == 0 {
            return;
        }
        if self.selected >= self.scroll + visible {
            self.scroll = self.selected.saturating_sub(visible - 1);
        } else if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        let filtered = self.filtered_indices(&songs);
        self.scroll = self.scroll.min(filtered.len().saturating_sub(visible));

        for (row, index) in filtered
            .iter()
            .copied()
            .skip(self.scroll)
            .take(visible)
            .enumerate()
        {
            let mut style = if index == current {
                Style::new().fg(accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            if index == self.selected {
                style = Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(accent)
                    .add_modifier(Modifier::BOLD);
            }
            Paragraph::new(Line::from(Span::styled(
                super::components::song_table::row(&songs[index], index, list.width),
                style,
            )))
            .render(Rect::new(list.x, list.y + row as u16, list.width, 1), buf);
        }
    }
}

fn queue_index_at(event: MouseEvent, area: Rect, scroll: usize, len: usize) -> Option<usize> {
    crate::pages::components::hit_test::row_at(
        area,
        Position::new(event.column, event.row),
        scroll,
        len,
        1,
    )
}

fn queue_index_at_filtered(
    event: MouseEvent,
    area: Rect,
    scroll: usize,
    indices: &[usize],
) -> Option<usize> {
    let pos = crate::pages::components::hit_test::row_at(
        area,
        Position::new(event.column, event.row),
        scroll,
        indices.len(),
        1,
    )?;
    indices.get(pos).copied()
}

impl MainPage {
    /// 绘制封面框，无法绘制封面时退回文字占位
    fn render_cover(
        &mut self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &AppContext,
        geometry: CoverGeometry,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::border(ctx)))
            .title(" 封面 ");
        let inner = block.inner(area);
        block.render(area, buf);

        self.cover.sync(ctx.cover_service.image_path().as_deref());
        if self.cover.render(geometry.image_rect(inner), buf) {
            return;
        }
        render_cover_text(inner, buf, ctx);
    }
}

fn render_cover_text(inner: Rect, buf: &mut Buffer, ctx: &AppContext) {
    let cover_state = ctx.cover_service.state();
    let song = ctx.current_song.read().unwrap_or_else(|e| e.into_inner());
    let lines = song.as_ref().map_or_else(
        || {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "等待播放",
                    Style::new().fg(crate::theme::muted(ctx)),
                )),
            ]
        },
        |song| {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    &song.name,
                    Style::new()
                        .fg(crate::theme::text(ctx))
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    &song.singer,
                    Style::new().fg(crate::theme::muted(ctx)),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    match &cover_state {
                        CoverState::Loading => "封面加载中...",
                        CoverState::Unavailable(_) => "封面不可用",
                        // current_song 非 None 但无封面 <-> 封面被禁用
                        CoverState::Empty => "",
                        // 封面就绪但是终端无法显示
                        CoverState::Ready => "封面无法显示",
                    },
                    Style::new().fg(crate::theme::muted(ctx)),
                )),
                match &cover_state {
                    CoverState::Unavailable(error) => Line::from(Span::styled(
                        error.chars().take(inner.width as usize).collect::<String>(),
                        Style::new().fg(crate::theme::overlay0(ctx)),
                    )),
                    _ => Line::from(""),
                },
            ]
        },
    );
    Paragraph::new(lines)
        .alignment(ratatui::layout::Alignment::Center)
        .render(inner, buf);
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{QueueEditCommand, queue_edit_command, queue_index_at};

    #[test]
    fn queue_click_rows_align_with_rendered_rows() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        use ratatui::layout::Rect;

        let area = Rect::new(0, 0, 100, 20);
        let click = |row: u16, scroll: usize, len: usize| {
            queue_index_at(
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 50,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
                area,
                scroll,
                len,
            )
        };

        assert_eq!(click(0, 0, 100), None);
        assert_eq!(click(1, 0, 100), None);
        assert_eq!(click(2, 0, 100), Some(0));
        assert_eq!(click(11, 0, 100), Some(9));
        assert_eq!(click(2, 7, 100), Some(7));
        assert_eq!(click(19, 0, 100), None);
    }

    #[test]
    fn queue_reorder_shortcuts_accept_terminal_shift_variants() {
        for key in [
            KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('K'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('K'), KeyModifiers::NONE),
        ] {
            assert_eq!(queue_edit_command(&key), Some(QueueEditCommand::MoveUp));
        }

        for key in [
            KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('J'), KeyModifiers::NONE),
        ] {
            assert_eq!(queue_edit_command(&key), Some(QueueEditCommand::MoveDown));
        }
    }

    #[test]
    fn queue_delete_shortcuts_distinguish_one_song_from_the_whole_queue() {
        for key in [
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE),
        ] {
            assert_eq!(
                queue_edit_command(&key),
                Some(QueueEditCommand::RemoveSelected)
            );
        }

        for key in [
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('D'), KeyModifiers::NONE),
        ] {
            assert_eq!(queue_edit_command(&key), Some(QueueEditCommand::Clear));
        }
    }
}
