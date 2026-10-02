//! 歌曲右键上下文菜单。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::keybinding::{Action, KeybindingResolver};
use lx_core::model::config::{SourcePolicy, TableColumnConfig};
use lx_core::model::song::SongInfo;
use lx_core::model::source::{Quality, SourceId};

use super::status_bar::StatusBarSlot;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::context::AppContext;
use crate::pages::sort::{SortMode, SortTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongMenuKind {
    Queue,
    Standard,
    History,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongMenuAction {
    Play,
    Download,
    PlayNext,
    AddToQueue,
    Playback(PlaybackMenuAction),
    AddToCustomPlaylist(String),
    ToggleFavorite,
    CycleSort(SortTarget),
    RemoveFromQueue,
    RemoveFromHistory,
    ClearHistory,
    DeleteLocal,
    RemoveFromCustomPlaylist(String),
    ViewArtist(String),
    ViewAlbum,
}

/// 表头右键菜单的动作。
///
/// 把"新的列配置"直接放进动作里（而不是只带一个列 key），
/// 是因为菜单在构造时已经知道完整列配置，这样处理端不需要回头去问页面，
/// 显示/隐藏切换、自动列宽两条路径共用同一个提交动作。
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnMenuAction {
    /// 应用一份新的列配置（显示隐藏切换、自动列宽都走这里）。
    Apply(Vec<TableColumnConfig>),
    /// 删除该页面的列配置，恢复默认档位与可见性。
    Reset,
    /// 删除该页面的面板分隔比例，恢复默认布局（队列的左右栏与封面/歌词分栏）。
    ResetLayout,
}

/// 菜单条目要执行的动作。
///
/// 菜单本身是**通用**的：歌曲动作、表头列动作、以及"打开子菜单"都走这一个
/// 枚举，所以队列、表头、歌单行等场景共用同一套渲染 / 命中 / 键盘导航，
/// 不会再出现每个场景各写一套菜单。
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAction {
    Song(SongMenuAction),
    Column(ColumnMenuAction),
    StatusBar(StatusBarMenuAction),
    /// 设置页的枚举取值：`row` 是设置行标识、`value` 是取值标识。
    ///
    /// 设置页自己构造、自己解释这个动作（它持有取值菜单并在模态输入里分派），
    /// 因此主循环不需要认识 `row` / `value` 的含义；两个字段都用字符串，
    /// 是为了让"任意设置行 + 任意取值"都能通过同一个通用菜单表达，
    /// 不必为每个枚举再扩一个 `StatusBarMenuAction` 变体。
    SettingChoice {
        row: String,
        value: String,
    },
    /// 打开子菜单（条目在构造时确定）。
    Submenu {
        title: String,
        items: Vec<MenuItem>,
    },
    /// 返回上一级（根层时关闭菜单）。
    Back,
}

impl From<SongMenuAction> for MenuAction {
    fn from(action: SongMenuAction) -> Self {
        Self::Song(action)
    }
}

impl From<ColumnMenuAction> for MenuAction {
    fn from(action: ColumnMenuAction) -> Self {
        Self::Column(action)
    }
}

impl From<StatusBarMenuAction> for MenuAction {
    fn from(action: StatusBarMenuAction) -> Self {
        Self::StatusBar(action)
    }
}

/// 底部状态栏（快速控制栏）菜单的动作。
///
/// 一律只描述"用户选了什么"，具体执行留在 app 侧 —— 与列菜单同样的分工。
#[derive(Debug, Clone, PartialEq)]
pub enum StatusBarMenuAction {
    /// 播放 / 暂停。
    TogglePlayPause,
    /// 上一首 / 下一首（队列手动导航）。
    PreviousTrack,
    NextTrack,
    /// 精确设置播放模式；取值是配置字符串（`list-loop` / `single-loop` / …）。
    SetPlayMode(String),
    /// 精确设置音质偏好。
    SetQuality(Quality),
    /// 用当前音质重新解析正在播放的歌（音质偏好只影响以后，这一项才立即生效）。
    ReparseCurrentSong,
    /// 音量增减（百分点）。
    VolumeDelta(i32),
    /// 直接把音量设为某个值。
    SetVolume(u32),
    /// 静音 / 恢复。
    ToggleMute,
    /// 跳到队列页并定位当前播放的那首。
    JumpToQueue,
    /// 清空队列。
    ClearQueue,
    /// 设置解析策略。
    SetSourcePolicy {
        policy: SourcePolicy,
        platform: Option<SourceId>,
    },
    /// 重新加载配置里的全部 JS 音源。
    ReloadJsSources,
    /// 打开设置页的音源面板。
    OpenSettingsSources,
    /// 打开下载面板。
    OpenDownloadsPanel,
    /// 设定 / 取消（`None`）睡眠定时器，取值是分钟数。
    SetSleepTimer(Option<u64>),
    /// 打开被「更多」收纳的某个段自己的菜单。
    OpenSlot(StatusBarSlot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackMenuAction {
    CycleSpeed,
    UseDefaultAudioDevice,
    CycleReplayGainMode,
    CycleReplayGainPreamp,
    ToggleReplayGainClip,
    CycleEqualizer,
    CycleChannelMode,
    CycleBalance,
    FadeIn,
    FadeOut,
    SetAbLoopStart,
    SetAbLoopEnd,
    ClearAbLoop,
}

/// 上下文菜单目标的来源：鼠标位置 or 键盘当前选中项。
///
/// 统一成一个入参，让"鼠标右键"和"键盘打开菜单（默认 `x`）"走
/// **完全同一条**目标解析路径，避免两条路径各写一份命中逻辑而跑偏。
#[derive(Debug, Clone, Copy)]
pub enum MenuHitSource {
    /// 鼠标事件：按行命中。
    Mouse(MouseEvent),
    /// 键盘：用列表当前选中项。
    Selected,
}

impl MenuHitSource {
    /// 解析出列表下标。
    ///
    /// `row_hit` 只在鼠标来源时调用；键盘来源取当前选中项并夹到合法范围。
    /// `len == 0` 时两者都返回 `None`（没有目标就不该弹菜单）。
    pub fn resolve_index(
        self,
        row_hit: impl FnOnce(MouseEvent) -> Option<usize>,
        selected: usize,
        len: usize,
    ) -> Option<usize> {
        if len == 0 {
            return None;
        }
        match self {
            Self::Mouse(event) => row_hit(event),
            Self::Selected => Some(selected.min(len - 1)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuOutcome {
    None,
    Close,
    /// 用户选中了一个动作。歌曲动作与列动作都从这里出来，
    /// 由 `main.rs` 按变体分派。
    Action(MenuAction),
}

#[derive(Debug, Clone, Default)]
pub struct SongContextMenuOptions {
    pub sort: Option<(SortTarget, SortMode)>,
    pub custom_playlists: Vec<(String, String)>,
    pub current_custom_playlist: Option<String>,
    pub playback: Option<PlaybackMenuState>,
}

#[derive(Debug, Clone, Default)]
pub struct PlaybackMenuState {
    pub speed: f64,
    pub audio_device: String,
    pub replaygain_mode: String,
    pub replaygain_preamp: f64,
    pub replaygain_clip: bool,
    pub equalizer: String,
    pub channel_mode: String,
    pub balance: f64,
    pub ab_loop: String,
}

/// 构造子菜单动作：自动补一条"返回上级"，避免每个子菜单各写一遍。
pub fn submenu(title: impl Into<String>, mut items: Vec<MenuItem>) -> MenuAction {
    items.push(MenuItem::new("← 返回上级", MenuAction::Back).with_hint("Esc"));
    MenuAction::Submenu {
        title: title.into(),
        items,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    label: String,
    action: MenuAction,
    /// 禁用项会被跳过（键盘选择、鼠标命中和渲染都跳过），不会误触。
    enabled: bool,
    /// 可选的快捷键提示，渲染在菜单行右端。
    hint: Option<String>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: impl Into<MenuAction>) -> Self {
        Self {
            label: label.into(),
            action: action.into(),
            enabled: true,
            hint: None,
        }
    }

    /// 占位/不可用项：可见但不可选，也不会触发动作。
    pub fn disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: MenuAction::Back,
            enabled: false,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[cfg(test)]
impl MenuItem {
    /// 测试辅助：菜单项显示文本。
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// 测试辅助：菜单项动作。
    pub(crate) fn action(&self) -> &MenuAction {
        &self.action
    }
}

/// 一层菜单（标题 + 条目）。菜单用 `Vec<MenuLevel>` 作栈，
/// 因此层级数量与子菜单内容都是运行期决定的通用能力。
#[derive(Debug, Clone)]
struct MenuLevel {
    title: String,
    items: Vec<MenuItem>,
}

impl MenuLevel {
    fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            title: title.into(),
            items,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SongContextMenu {
    origin: Position,
    songs: Vec<SongInfo>,
    index: usize,
    selected: usize,
    scroll_offset: usize,
    /// 层级栈：`stack[0]` 恒为根层。
    stack: Vec<MenuLevel>,
    /// 该菜单作用的页面 key（列菜单用它把新配置提交到正确的页面）。
    page_key: Option<String>,
}

impl SongContextMenu {
    pub fn new(
        origin: Position,
        songs: Vec<SongInfo>,
        index: usize,
        kind: SongMenuKind,
        is_favorite: bool,
        options: SongContextMenuOptions,
    ) -> Option<Self> {
        songs.get(index)?;
        let SongContextMenuOptions {
            sort,
            custom_playlists,
            current_custom_playlist,
            playback,
        } = options;
        let mut root_items = vec![MenuItem::new("播放".to_string(), SongMenuAction::Play)];
        // 本地文件已经在磁盘上，不提供下载入口。
        if kind != SongMenuKind::Local {
            root_items.push(MenuItem::new(
                "下载歌曲".to_string(),
                SongMenuAction::Download,
            ));
        }
        if kind != SongMenuKind::Queue {
            root_items.extend([
                MenuItem::new("设为下一首".to_string(), SongMenuAction::PlayNext),
                MenuItem::new("加入队尾".to_string(), SongMenuAction::AddToQueue),
            ]);
        }
        // 子菜单条目先建好，挂成 root 上的一个 Submenu 动作。
        let custom_playlist_items = if custom_playlists.is_empty() {
            vec![MenuItem::disabled("暂无自建歌单，请先创建")]
        } else {
            custom_playlists
                .into_iter()
                .map(|(id, name)| MenuItem::new(name, SongMenuAction::AddToCustomPlaylist(id)))
                .collect()
        };
        root_items.push(MenuItem::new(
            "加入自建歌单...".to_string(),
            submenu(" 选择自建歌单 ", custom_playlist_items),
        ));
        if let Some(playlist_id) = current_custom_playlist {
            root_items.push(MenuItem::new(
                "从当前歌单移除".to_string(),
                SongMenuAction::RemoveFromCustomPlaylist(playlist_id),
            ));
        }
        root_items.push(MenuItem::new(
            if is_favorite {
                "取消收藏".to_string()
            } else {
                "收藏歌曲".to_string()
            },
            SongMenuAction::ToggleFavorite,
        ));
        let song = &songs[index];
        let artists = song
            .singer
            .split([',', '，', '、', '&', '＆'])
            .map(str::trim)
            .filter(|artist| !artist.is_empty())
            .collect::<Vec<_>>();
        if artists.len() > 1 {
            for artist in artists {
                root_items.push(MenuItem::new(
                    format!("查看歌手：{artist}"),
                    SongMenuAction::ViewArtist(artist.to_string()),
                ));
            }
        } else if let Some(artist) = artists.first() {
            root_items.push(MenuItem::new(
                "查看歌手".to_string(),
                SongMenuAction::ViewArtist((*artist).to_string()),
            ));
        }
        if !song.album_name.trim().is_empty() {
            root_items.push(MenuItem::new(
                "查看专辑".to_string(),
                SongMenuAction::ViewAlbum,
            ));
        }
        if let Some(playback_state) = playback {
            root_items.push(MenuItem::new(
                "播放控制...".to_string(),
                submenu(" 播放控制 ", build_playback_control_items(playback_state)),
            ));
        }
        if let Some((target, mode)) = sort {
            root_items.push(MenuItem::new(
                format!("排序：{}（切换）", mode.label(target)),
                SongMenuAction::CycleSort(target),
            ));
        }
        match kind {
            SongMenuKind::Queue => root_items.push(MenuItem::new(
                "从队列移除".to_string(),
                SongMenuAction::RemoveFromQueue,
            )),
            SongMenuKind::History => root_items.extend([
                MenuItem::new(
                    "删除这条历史".to_string(),
                    SongMenuAction::RemoveFromHistory,
                ),
                MenuItem::new("清空播放历史".to_string(), SongMenuAction::ClearHistory),
            ]),
            SongMenuKind::Local => root_items.push(MenuItem::new(
                "删除本地文件".to_string(),
                SongMenuAction::DeleteLocal,
            )),
            SongMenuKind::Standard => {}
        }
        Some(Self {
            origin,
            songs,
            index,
            selected: 0,
            scroll_offset: 0,
            stack: vec![MenuLevel::new(" 歌曲操作 ", root_items)],
            page_key: None,
        })
    }

    pub fn songs(&self) -> &[SongInfo] {
        &self.songs
    }

    /// 用任意条目构造一个通用菜单。
    ///
    /// 表头列菜单、行级菜单等非歌曲场景复用它，因此渲染、命中、键盘导航、
    /// 子菜单与"返回上级"只有一份实现。
    pub fn from_entries(
        origin: Position,
        title: impl Into<String>,
        items: Vec<MenuItem>,
        page_key: impl Into<String>,
    ) -> Self {
        Self {
            origin,
            songs: Vec::new(),
            index: 0,
            selected: 0,
            scroll_offset: 0,
            stack: vec![MenuLevel::new(title, items)],
            page_key: Some(page_key.into()),
        }
    }

    /// 打开一个"不属于任何页面"的菜单（底部状态栏用）。
    pub fn from_status_items(
        origin: Position,
        title: impl Into<String>,
        items: Vec<MenuItem>,
    ) -> Self {
        Self {
            origin,
            songs: Vec::new(),
            index: 0,
            selected: 0,
            scroll_offset: 0,
            stack: vec![MenuLevel::new(title, items)],
            page_key: None,
        }
    }

    /// 该菜单作用的页面 key（歌曲菜单为 `None`）。
    pub fn page_key(&self) -> Option<&str> {
        self.page_key.as_deref()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn song(&self) -> &SongInfo {
        &self.songs[self.index]
    }

    pub fn handle_key(
        &mut self,
        key: &KeyEvent,
        resolver: &KeybindingResolver,
        page_scope: &str,
        bounds: Rect,
    ) -> MenuOutcome {
        let visible_items = self.visible_item_count(bounds);
        if matches!(
            (key.modifiers, key.code),
            (KeyModifiers::NONE, KeyCode::Esc | KeyCode::Char('q'))
        ) {
            return MenuOutcome::Close;
        }
        if matches!(
            (key.modifiers, key.code),
            (KeyModifiers::NONE, KeyCode::Enter)
        ) {
            return self.activate();
        }

        match resolver.resolve_page(page_scope, key) {
            Some(Action::ListSelectUp) => self.select_previous(visible_items),
            Some(Action::ListSelectDown) => self.select_next(visible_items),
            Some(Action::ListActivate) => return self.activate(),
            Some(Action::ListGoBack) => {
                // 子菜单里"返回"只回上一级，根层才关闭整个菜单。
                return if self.pop_level() {
                    MenuOutcome::None
                } else {
                    MenuOutcome::Close
                };
            }
            _ => match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Up) => self.select_previous(visible_items),
                (KeyModifiers::NONE, KeyCode::Down) => self.select_next(visible_items),
                _ => {}
            },
        }
        MenuOutcome::None
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, bounds: Rect) -> MenuOutcome {
        let area = self.area(bounds);
        let visible_items = Block::default().borders(Borders::ALL).inner(area).height as usize;
        match event.kind {
            MouseEventKind::ScrollUp => self.select_previous(visible_items),
            MouseEventKind::ScrollDown => self.select_next(visible_items),
            MouseEventKind::Moved => {
                if let Some(index) = item_at(
                    area,
                    Position::new(event.column, event.row),
                    self.items().len(),
                    self.scroll_offset,
                ) {
                    self.selected = index;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let position = Position::new(event.column, event.row);
                let Some(index) = item_at(area, position, self.items().len(), self.scroll_offset)
                else {
                    return MenuOutcome::Close;
                };
                self.selected = index;
                return self.activate();
            }
            MouseEventKind::Down(MouseButton::Right) => return MenuOutcome::Close,
            _ => {}
        }
        MenuOutcome::None
    }

    pub fn render(&self, bounds: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let area = self.area(bounds);
        if area.width == 0 || area.height == 0 {
            return;
        }
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .style(
                Style::new()
                    .bg(crate::theme::surface0(ctx))
                    .fg(crate::theme::text(ctx)),
            )
            .title(self.title());
        let inner = block.inner(area);
        block.render(area, buf);

        for (row, (index, item)) in self
            .items()
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(inner.height as usize)
            .enumerate()
        {
            let style = if !item.enabled {
                // 禁用项：不可选，用 muted 色区分。
                Style::new()
                    .bg(crate::theme::surface0(ctx))
                    .fg(crate::theme::muted(ctx))
            } else if index == self.selected {
                Style::new()
                    .bg(crate::theme::accent(ctx))
                    .fg(crate::theme::selection_fg(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new()
                    .bg(crate::theme::surface0(ctx))
                    .fg(crate::theme::text(ctx))
            };
            let row_rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
            let hint_width = item
                .hint
                .as_deref()
                .map(|hint| unicode_width::UnicodeWidthStr::width(hint) as u16 + 1)
                .unwrap_or(0)
                .min(inner.width);
            let label_width = inner.width.saturating_sub(hint_width);
            Paragraph::new(Line::from(Span::styled(format!(" {}", item.label), style)))
                .style(style)
                .render(Rect::new(row_rect.x, row_rect.y, label_width, 1), buf);
            if let Some(hint) = item.hint.as_deref()
                && hint_width > 0
            {
                Paragraph::new(Line::from(Span::styled(
                    format!("{hint} "),
                    style.add_modifier(Modifier::DIM),
                )))
                .alignment(ratatui::layout::Alignment::Right)
                .style(style)
                .render(
                    Rect::new(row_rect.x + label_width, row_rect.y, hint_width, 1),
                    buf,
                );
            }
        }
    }

    fn area(&self, bounds: Rect) -> Rect {
        menu_area(bounds, self.origin, self.items().len())
    }

    fn visible_item_count(&self, bounds: Rect) -> usize {
        Block::default()
            .borders(Borders::ALL)
            .inner(self.area(bounds))
            .height as usize
    }

    fn select_previous(&mut self, visible_items: usize) {
        self.step_selection(false, visible_items);
    }

    fn select_next(&mut self, visible_items: usize) {
        self.step_selection(true, visible_items);
    }

    /// 按方向移动选中，跳过禁用项；全部禁用时保持不动。
    fn step_selection(&mut self, forward: bool, visible_items: usize) {
        let len = self.items().len();
        if len == 0 {
            return;
        }
        let mut index = self.selected;
        for _ in 0..len {
            index = if forward {
                (index + 1) % len
            } else if index == 0 {
                len - 1
            } else {
                index - 1
            };
            if self.items()[index].enabled {
                self.selected = index;
                self.ensure_selected_visible(visible_items);
                return;
            }
        }
    }

    fn activate(&mut self) -> MenuOutcome {
        let item = self.items().get(self.selected);
        if item.is_some_and(|item| !item.enabled) {
            // 禁用项：不派发动作，也不关菜单。
            return MenuOutcome::None;
        }
        let action = item.map(|item| item.action.clone());
        match action {
            Some(MenuAction::Back) => {
                self.pop_level();
                MenuOutcome::None
            }
            Some(MenuAction::Submenu { title, items }) => {
                self.stack.push(MenuLevel::new(title, items));
                self.selected = 0;
                self.scroll_offset = 0;
                MenuOutcome::None
            }
            Some(action) => MenuOutcome::Action(action),
            None => MenuOutcome::Close,
        }
    }

    /// 返回上一级；已在根层时返回 `false`（调用方据此决定是否关闭菜单）。
    fn pop_level(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        self.selected = 0;
        self.scroll_offset = 0;
        true
    }

    /// 当前层级标题。
    fn title(&self) -> &str {
        self.stack
            .last()
            .map(|level| level.title.as_str())
            .unwrap_or("")
    }

    fn items(&self) -> &[MenuItem] {
        self.stack
            .last()
            .map(|level| level.items.as_slice())
            .unwrap_or(&[])
    }

    /// 测试辅助：取某一层的条目（按栈序，0 = 根层）。
    #[cfg(test)]
    fn items_at_level(&self, level: usize) -> &[MenuItem] {
        self.stack
            .get(level)
            .map(|level| level.items.as_slice())
            .unwrap_or(&[])
    }

    fn ensure_selected_visible(&mut self, visible_items: usize) {
        let visible_items = visible_items.max(1);
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + visible_items {
            self.scroll_offset = self.selected + 1 - visible_items;
        }
    }
}

fn menu_area(bounds: Rect, origin: Position, item_count: usize) -> Rect {
    if bounds.width == 0 || bounds.height == 0 {
        return Rect::default();
    }
    let width = 38.min(bounds.width);
    let height = (item_count as u16 + 2).min(bounds.height);
    let max_x = bounds.right().saturating_sub(width);
    let max_y = bounds.bottom().saturating_sub(height);
    Rect::new(
        origin.x.clamp(bounds.x, max_x),
        origin.y.clamp(bounds.y, max_y),
        width,
        height,
    )
}

fn build_playback_control_items(state: PlaybackMenuState) -> Vec<MenuItem> {
    use PlaybackMenuAction as Action;

    vec![
        playback_item(
            format!("播放速度: {:.2}x（切换）", state.speed),
            Action::CycleSpeed,
        ),
        playback_item(
            format!("音频设备: {}（恢复默认）", state.audio_device),
            Action::UseDefaultAudioDevice,
        ),
        playback_item(
            format!("ReplayGain: {}（切换）", state.replaygain_mode),
            Action::CycleReplayGainMode,
        ),
        playback_item(
            format!("ReplayGain 预放大: {:+.1} dB", state.replaygain_preamp),
            Action::CycleReplayGainPreamp,
        ),
        playback_item(
            format!(
                "ReplayGain 削波保护: {}",
                if state.replaygain_clip { "开" } else { "关" }
            ),
            Action::ToggleReplayGainClip,
        ),
        playback_item(
            format!("均衡器: {}（切换）", state.equalizer),
            Action::CycleEqualizer,
        ),
        playback_item(
            format!("声道模式: {}（切换）", state.channel_mode),
            Action::CycleChannelMode,
        ),
        playback_item(
            format!("左右平衡: {:+.2}（切换）", state.balance),
            Action::CycleBalance,
        ),
        playback_item("立即淡入".to_string(), Action::FadeIn),
        playback_item("立即淡出".to_string(), Action::FadeOut),
        playback_item("以当前进度设置 A 点".to_string(), Action::SetAbLoopStart),
        playback_item("以当前进度设置 B 点".to_string(), Action::SetAbLoopEnd),
        playback_item(
            format!("清除 A-B 循环: {}", state.ab_loop),
            Action::ClearAbLoop,
        ),
    ]
}

fn playback_item(label: String, action: PlaybackMenuAction) -> MenuItem {
    MenuItem::new(label, SongMenuAction::Playback(action))
}

fn item_at(
    area: Rect,
    position: Position,
    item_count: usize,
    scroll_offset: usize,
) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !inner.contains(position) {
        return None;
    }
    let index = scroll_offset + position.y.saturating_sub(inner.y) as usize;
    (index < item_count).then_some(index)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use lx_core::keybinding::{KeybindingConfig, KeybindingResolver};
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::SourceId;

    use super::{
        MenuAction, MenuOutcome, PlaybackMenuAction, PlaybackMenuState, SongContextMenu,
        SongContextMenuOptions, SongMenuAction, SongMenuKind, item_at, menu_area,
    };
    use crate::pages::sort::{SortMode, SortTarget};
    use ratatui::layout::{Position, Rect};

    fn menu_with_playlists() -> SongContextMenu {
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        SongContextMenu::new(
            Position::new(2, 2),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                custom_playlists: vec![("p1".to_string(), "歌单A".to_string())],
                ..SongContextMenuOptions::default()
            },
        )
        .expect("菜单应当能构造")
    }

    /// 子菜单必须能回到根层：以前 Esc / ListGoBack 都直接关掉整个菜单。
    #[test]
    fn back_item_returns_to_the_root_level() {
        let mut menu = menu_with_playlists();
        assert_eq!(menu.stack.len(), 1, "初始只有根层");

        // 进入"加入自建歌单"子菜单
        menu.selected = menu
            .items()
            .iter()
            .position(|item| matches!(item.action, MenuAction::Submenu { .. }))
            .expect("应当有进入自建歌单的入口");
        assert_eq!(menu.activate(), MenuOutcome::None);
        assert_eq!(menu.stack.len(), 2, "应当进入子菜单");

        // 最后一项是"返回上级"
        let back_index = menu.items().len().checked_sub(1).expect("子菜单非空");
        assert!(matches!(menu.items()[back_index].action, MenuAction::Back));

        menu.selected = back_index;
        assert_eq!(menu.activate(), MenuOutcome::None);
        assert_eq!(menu.stack.len(), 1, "应当回到根层而不是关掉菜单");
    }

    /// 禁用项既不能被选中，也不能被激活。
    #[test]
    fn disabled_items_are_skipped_and_never_activate() {
        let mut menu = menu_with_playlists();
        // 造一个"空自建歌单"菜单：唯一的子菜单项是禁用占位
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        let empty = SongContextMenu::new(
            Position::new(2, 2),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                custom_playlists: Vec::new(),
                ..SongContextMenuOptions::default()
            },
        )
        .expect("菜单应当能构造");
        assert!(
            empty
                .items_at_level(0)
                .iter()
                .find_map(|item| match &item.action {
                    MenuAction::Submenu { items, .. } => Some(items),
                    _ => None,
                })
                .expect("根层有自建歌单子菜单")
                .iter()
                .any(|item| !item.enabled)
        );

        // 根层全是可选项，逐个下移一圈都不会停在禁用项上
        for _ in 0..menu.items().len() {
            menu.select_next(10);
            assert!(menu.items()[menu.selected].enabled, "选中项不应是禁用项");
        }
    }

    #[test]
    fn context_menu_is_clamped_inside_content_area() {
        let bounds = Rect::new(10, 5, 40, 12);
        let area = menu_area(bounds, Position::new(48, 15), 5);

        assert!(bounds.contains(Position::new(area.x, area.y)));
        assert_eq!(area.right(), bounds.right());
        assert_eq!(area.bottom(), bounds.bottom());
    }

    #[test]
    fn menu_item_hit_test_ignores_border() {
        let area = Rect::new(10, 5, 24, 7);

        assert_eq!(item_at(area, Position::new(12, 6), 5, 0), Some(0));
        assert_eq!(item_at(area, Position::new(12, 10), 5, 0), Some(4));
        assert_eq!(item_at(area, Position::new(12, 6), 8, 3), Some(3));
        assert_eq!(item_at(area, Position::new(10, 6), 5, 0), None);
    }

    #[test]
    fn sortable_pages_append_a_sort_action() {
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        let menu = SongContextMenu::new(
            Position::new(1, 1),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                sort: Some((SortTarget::Favorites, SortMode::Newest)),
                ..SongContextMenuOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            menu.items().last().map(|item| item.action.clone()),
            Some(MenuAction::Song(SongMenuAction::CycleSort(
                SortTarget::Favorites
            )))
        );
    }

    #[test]
    fn escape_closes_the_custom_playlist_submenu_in_one_step() {
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        let mut menu = SongContextMenu::new(
            Position::new(1, 1),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                custom_playlists: vec![("custom-1".to_string(), "通勤".to_string())],
                ..SongContextMenuOptions::default()
            },
        )
        .unwrap();
        menu.selected = menu
            .items()
            .iter()
            .position(|item| matches!(item.action, MenuAction::Submenu { .. }))
            .unwrap();
        assert_eq!(menu.activate(), MenuOutcome::None);
        assert_eq!(menu.stack.len(), 2);

        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        assert_eq!(
            menu.handle_key(
                &KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &resolver,
                "playlists",
                Rect::new(0, 0, 40, 20),
            ),
            MenuOutcome::Close
        );
    }

    #[test]
    fn custom_playlist_menu_scrolls_with_the_actual_viewport_height() {
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        let custom_playlists = (0..8)
            .map(|index| (format!("custom-{index}"), format!("歌单 {index}")))
            .collect();
        let mut menu = SongContextMenu::new(
            Position::new(0, 0),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                custom_playlists,
                ..SongContextMenuOptions::default()
            },
        )
        .unwrap();
        if menu.stack.len() == 1 {
            let index = menu
                .items()
                .iter()
                .position(|item| matches!(item.action, MenuAction::Submenu { .. }))
                .expect("有子菜单入口");
            menu.selected = index;
            menu.activate();
        }
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        let bounds = Rect::new(0, 0, 40, 5);

        for _ in 0..4 {
            assert_eq!(
                menu.handle_key(
                    &KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                    &resolver,
                    "playlists",
                    bounds,
                ),
                MenuOutcome::None
            );
        }

        assert_eq!(menu.selected, 4);
        assert_eq!(menu.scroll_offset, 2);
    }

    #[test]
    fn playback_controls_open_as_a_submenu_and_dispatch_actions() {
        let song = SongInfo::new(
            "1".to_string(),
            SourceId::Kw,
            "Song".to_string(),
            "Artist".to_string(),
        );
        let mut menu = SongContextMenu::new(
            Position::new(0, 0),
            vec![song],
            0,
            SongMenuKind::Standard,
            false,
            SongContextMenuOptions {
                playback: Some(PlaybackMenuState {
                    speed: 1.25,
                    audio_device: "auto".to_string(),
                    replaygain_mode: "track".to_string(),
                    equalizer: "关闭".to_string(),
                    channel_mode: "stereo".to_string(),
                    ab_loop: "未设置".to_string(),
                    ..PlaybackMenuState::default()
                }),
                ..SongContextMenuOptions::default()
            },
        )
        .unwrap();
        // 根层有两个子菜单（自建歌单 / 播放控制），按标题精确定位后者。
        menu.selected = menu
            .items()
            .iter()
            .position(|item| {
                matches!(&item.action, MenuAction::Submenu { title, .. } if title.contains("播放控制"))
            })
            .unwrap();

        assert_eq!(menu.activate(), MenuOutcome::None);
        assert_eq!(menu.stack.len(), 2);
        assert_eq!(
            menu.activate(),
            MenuOutcome::Action(MenuAction::Song(SongMenuAction::Playback(
                PlaybackMenuAction::CycleSpeed
            )))
        );
    }
}

#[cfg(test)]
mod hit_source_tests {
    use super::MenuHitSource;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// 键盘来源用当前选中项，鼠标来源用行命中 —— 但两者共用同一条解析入口。
    #[test]
    fn selected_source_uses_the_current_selection() {
        let index = MenuHitSource::Selected.resolve_index(|_| None, 3, 10);
        assert_eq!(index, Some(3));
    }

    #[test]
    fn selected_source_clamps_to_the_last_row() {
        assert_eq!(
            MenuHitSource::Selected.resolve_index(|_| None, 99, 4),
            Some(3)
        );
    }

    #[test]
    fn mouse_source_uses_the_row_hit_test() {
        let index = MenuHitSource::Mouse(click(5, 5)).resolve_index(
            |event| Some(event.row as usize),
            0,
            10,
        );
        assert_eq!(index, Some(5));
    }

    /// 没有目标（空列表）时两种来源都不该弹菜单。
    #[test]
    fn empty_list_never_yields_a_target() {
        assert_eq!(MenuHitSource::Selected.resolve_index(|_| None, 0, 0), None);
        assert_eq!(
            MenuHitSource::Mouse(click(1, 1)).resolve_index(|_| Some(0), 0, 0),
            None
        );
    }
}
