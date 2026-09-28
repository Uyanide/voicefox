//! 设置页面：支持 JS 音源 URL 或本地路径导入/删除

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::AppAction;
use lx_core::keybinding::{Action, KeybindingConfig, KeybindingResolver};
use lx_core::model::config::StatusBarItem;
use lx_core::model::source::{Quality, SourceId};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::context::AppContext;
use crate::pages::components::splitter::{
    DividerHit, SplitAxis, Splitter, clamp_ratio, ratio_within,
};

/// 删除类操作（音源 / 本地目录）二次确认的窗口时长
const DELETE_CONFIRM_WINDOW: Duration = Duration::from_secs(5);

/// 检查 JS 音源是否已缓存到本地
fn is_source_cached(url: &str) -> bool {
    lx_source::js::loader::is_source_cached(url)
}

fn shorten_source(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let visible_chars = max_chars.saturating_sub(3);
    format!(
        "{}...",
        value.chars().take(visible_chars).collect::<String>()
    )
}

fn truncate_display(value: &str, max_chars: usize) -> String {
    let width = UnicodeWidthStr::width(value);
    if width <= max_chars {
        return format!("{value}{}", " ".repeat(max_chars - width));
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let available = max_chars - 1;
    let mut result = String::new();
    let mut used = 0;
    for character in value.chars() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > available {
            break;
        }
        result.push(character);
        used += character_width;
    }
    result.push('…');
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsFocus {
    JsSources,
    LocalPaths,
    StatusBar,
    QrLogin,
}

/// 下载设置里的文本输入目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadInputTarget {
    /// 下载目录
    Dir,
    /// 文件名模板
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsCategory {
    Interface,
    Playback,
    Sources,
    Accounts,
    Integration,
    Download,
    Data,
}

impl SettingsCategory {
    fn next(self) -> Self {
        match self {
            Self::Interface => Self::Playback,
            Self::Playback => Self::Sources,
            Self::Sources => Self::Accounts,
            Self::Accounts => Self::Integration,
            Self::Integration => Self::Download,
            Self::Download => Self::Data,
            Self::Data => Self::Interface,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Interface => Self::Data,
            Self::Playback => Self::Interface,
            Self::Sources => Self::Playback,
            Self::Accounts => Self::Sources,
            Self::Integration => Self::Accounts,
            Self::Download => Self::Integration,
            Self::Data => Self::Download,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Interface => "界面",
            Self::Playback => "播放",
            Self::Sources => "音源与歌词",
            Self::Accounts => "账号与扫码",
            Self::Integration => "通知与集成",
            Self::Download => "下载",
            Self::Data => "数据与本地库",
        }
    }

    fn option_indices(self) -> &'static [usize] {
        match self {
            Self::Interface => &[0, 1, 2, 3, 4, 31, 32, 33, 39],
            Self::Playback => &[
                5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            ],
            Self::Sources => &[23, 24, 25, 26, 27, 28, 29, 30],
            Self::Accounts => &[44, 58, 59],
            Self::Integration => &[34, 35, 36, 37, 38],
            Self::Download => &[45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57],
            Self::Data => &[40, 41, 42, 43],
        }
    }
}

impl SettingsFocus {
    fn next(self) -> Self {
        match self {
            Self::JsSources => Self::LocalPaths,
            Self::LocalPaths => Self::StatusBar,
            Self::StatusBar => Self::QrLogin,
            Self::QrLogin => Self::JsSources,
        }
    }
}

pub struct SettingsPage {
    /// 输入中的 JS 源 URL 或本地路径
    pub input_url: String,
    /// 是否在输入模式
    pub input_mode: bool,
    /// 导入状态消息
    pub status_msg: Option<String>,
    /// JS 源列表的选中索引
    pub selected_source: usize,
    /// 本地音乐路径输入
    pub local_path_input: String,
    /// 本地音乐路径输入模式
    pub local_path_mode: bool,
    /// 本地路径列表选中索引
    pub selected_local_path: usize,
    /// 代理地址输入
    pub proxy_input: String,
    /// 代理地址输入模式
    pub proxy_input_mode: bool,
    /// 音频输出设备输入模式
    pub audio_device_input: String,
    pub audio_device_input_mode: bool,
    /// 外部歌单文件输入模式。
    pub playlist_import_input: String,
    pub playlist_import_mode: bool,
    /// 下载目录 / 文件名模板输入模式。
    download_input: String,
    download_input_target: Option<DownloadInputTarget>,
    /// 内置音源开关当前指向的音源
    pub enabled_source_index: usize,
    /// 扫码登录入口当前指向的音源
    qr_login_source_index: usize,
    /// 状态栏字段列表的选中索引
    pub selected_status_item: usize,
    /// 状态栏字段列表的滚动位置
    status_item_scroll: usize,
    /// 状态栏拖拽当前所在的字段行，避免同一行重复触发重排。
    status_drag_target: Option<usize>,
    /// 面板尺寸参数（可被用户拖拽改变，并持久化到 Config）。
    layout: SettingsLayout,
    /// 面板分隔条拖拽状态机。
    splitter: Splitter<SettingsResizeTarget>,
    /// 拖拽开始前的布局快照，用于 Esc 取消还原。
    layout_before_drag: Option<SettingsLayout>,
    /// 当前聚焦区域
    focus: SettingsFocus,
    category: SettingsCategory,
    /// 删除 JS 音源的武装时刻：首次按 d 只武装，窗口内再按一次才删除
    delete_source_armed: Option<Instant>,
    /// 删除本地目录的武装时刻，机制同上
    delete_local_path_armed: Option<Instant>,
}

impl SettingsPage {
    /// 检查是否有任何输入模式激活（JS 源输入或本地路径输入）
    pub fn any_input_active(&self) -> bool {
        self.input_mode
            || self.local_path_mode
            || self.proxy_input_mode
            || self.audio_device_input_mode
            || self.playlist_import_mode
            || self.download_input_target.is_some()
    }

    /// 判断按键是否由设置页独占。设置页把整个字母表当作选项开关，
    /// 与用户可自定义的全局快捷键必然重叠，因此这些键不再交给全局分发。
    /// 未被 settings 页面级动作绑定的 Ctrl/Alt 组合键仍归全局。
    pub fn consumes_key(&self, key: &KeyEvent, resolver: &KeybindingResolver) -> bool {
        // Bare number keys are reserved for navigation (1-8 select sidebar
        // tabs). Even a stale or intentionally custom settings binding must
        // not make tab switching stop while the settings page is open.
        if key.modifiers == KeyModifiers::NONE && matches!(key.code, KeyCode::Char('0'..='9')) {
            return false;
        }
        if resolver
            .resolve_page("settings", key)
            .is_some_and(settings_action_is_page_owned)
        {
            return true;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        if self.focus == SettingsFocus::StatusBar
            && (matches!(
                (key.modifiers, key.code),
                (KeyModifiers::NONE, KeyCode::Enter | KeyCode::Char(' '))
            ) || matches!(
                (key.modifiers, key.code),
                (KeyModifiers::SHIFT, KeyCode::Left | KeyCode::Right)
            ))
        {
            return true;
        }
        match key.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => true,
            KeyCode::Char(character) => SETTINGS_PAGE_CHAR_KEYS.contains(&character),
            _ => false,
        }
    }

    pub fn new() -> Self {
        Self {
            input_url: String::new(),
            input_mode: false,
            status_msg: None,
            selected_source: 0,
            local_path_input: String::new(),
            local_path_mode: false,
            selected_local_path: 0,
            proxy_input: String::new(),
            proxy_input_mode: false,
            audio_device_input: String::new(),
            audio_device_input_mode: false,
            playlist_import_input: String::new(),
            playlist_import_mode: false,
            download_input: String::new(),
            download_input_target: None,
            enabled_source_index: 0,
            qr_login_source_index: 0,
            selected_status_item: 0,
            status_item_scroll: 0,
            status_drag_target: None,
            layout: SettingsLayout::default(),
            splitter: Splitter::default(),
            layout_before_drag: None,
            focus: SettingsFocus::JsSources,
            category: SettingsCategory::Interface,
            delete_source_armed: None,
            delete_local_path_armed: None,
        }
    }

    fn qr_login_sources(&self, ctx: &AppContext) -> Vec<SourceId> {
        SourceId::all_online()
            .iter()
            .copied()
            .filter(|source| ctx.source_manager.capabilities(*source).qr_login)
            .collect()
    }

    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> AppAction {
        if self.splitter.is_dragging() && key.code == KeyCode::Esc {
            // Esc 还原拖拽前的面板布局（与其它页面的分割条行为一致）。
            self.cancel_resize();
            return AppAction::None;
        }
        if self.proxy_input_mode {
            return self.handle_proxy_input(key, ctx);
        }
        if self.audio_device_input_mode {
            return self.handle_audio_device_input(key, ctx);
        }
        if self.playlist_import_mode {
            return self.handle_playlist_import_input(key, ctx);
        }
        if self.download_input_target.is_some() {
            return self.handle_download_input(key, ctx);
        }
        if self.local_path_mode {
            return self.handle_local_path_input(key, ctx);
        }
        if self.input_mode {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    self.input_mode = false;
                    self.input_url.clear();
                    return AppAction::None;
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    if !self.input_url.trim().is_empty() {
                        let url = self.input_url.trim().to_string();
                        self.input_mode = false;
                        self.input_url.clear();
                        self.status_msg = Some("正在添加音源...".to_string());
                        return AppAction::ImportSource(url);
                    }
                    return AppAction::None;
                }
                (modifiers, KeyCode::Char(c))
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.input_url.push(c);
                }
                (KeyModifiers::NONE, KeyCode::Backspace) => {
                    self.input_url.pop();
                }
                _ => {}
            }
        } else {
            // 同一次按键只解析一次页面级键位，两处共用结果
            let bound_action = resolver.resolve_page("settings", &key);
            // P 在设置页统一进入“账号与扫码”面板；不要再与主题色等设置复用同一个键。
            // 进入面板后 P 才执行当前选中音源的扫码登录。
            if key.modifiers == KeyModifiers::NONE && matches!(key.code, KeyCode::Char('p' | 'P')) {
                if self.category == SettingsCategory::Accounts {
                    let login_sources = self.qr_login_sources(ctx);
                    if login_sources.is_empty() {
                        self.status_msg = Some("当前没有支持扫码登录的音源".to_string());
                    } else {
                        self.qr_login_source_index %= login_sources.len();
                        let source = login_sources[self.qr_login_source_index];
                        if ctx.source_manager.is_logged_in(source) {
                            self.status_msg =
                                Some(format!("{} 已登录，按 b 退出登录", source.display_name()));
                        } else {
                            return AppAction::QrLogin(source);
                        }
                    }
                } else {
                    self.category = SettingsCategory::Accounts;
                    self.status_msg = None;
                }
                return AppAction::None;
            }

            // s 是设置页管理区域焦点切换键，必须在全局/自定义绑定解析之前处理。
            // 否则用户若在 keybindings 中把 s 绑定到了其他 Action，焦点切换会被吞掉。
            if self.category == SettingsCategory::Accounts
                && key.modifiers == KeyModifiers::SHIFT
                && matches!(key.code, KeyCode::Char('S' | 's'))
            {
                return AppAction::SyncNetease;
            }
            // QQ 同步目前还没接入远程集合模式。这里必须要求 Shift：否则账号面板
            // 里的小写 q（全局退出键）会被它吃掉，用户在这个面板里退不出程序。
            if self.category == SettingsCategory::Accounts
                && key.modifiers == KeyModifiers::SHIFT
                && matches!(key.code, KeyCode::Char('Q' | 'q'))
            {
                return AppAction::SyncQq;
            }

            if matches!(
                (key.modifiers, key.code),
                (KeyModifiers::NONE, KeyCode::Char('s'))
            ) {
                self.focus = self.focus.next();
                return AppAction::None;
            }

            if let Some(action) = bound_action
                && let Some(result) = self.handle_bound_action(action, ctx)
            {
                return result;
            }

            if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Left {
                self.category = self.category.previous();
                self.status_msg = None;
                return AppAction::None;
            }
            if key.modifiers == KeyModifiers::NONE && key.code == KeyCode::Right {
                self.category = self.category.next();
                self.status_msg = None;
                return AppAction::None;
            }

            if self.category == SettingsCategory::Accounts {
                let login_sources = self.qr_login_sources(ctx);
                if !login_sources.is_empty() {
                    match (key.modifiers, key.code) {
                        (KeyModifiers::NONE, KeyCode::Up) => {
                            self.qr_login_source_index = self
                                .qr_login_source_index
                                .checked_sub(1)
                                .unwrap_or(login_sources.len() - 1);
                            return AppAction::None;
                        }
                        (KeyModifiers::NONE, KeyCode::Down) => {
                            self.qr_login_source_index =
                                (self.qr_login_source_index + 1) % login_sources.len();
                            return AppAction::None;
                        }
                        _ => {}
                    }
                }
            }

            // 当前列表区域的按键优先处理。
            if self.focus == SettingsFocus::LocalPaths
                && let Some(action) = self.handle_local_keys(key, ctx, resolver)
            {
                return action;
            }
            if self.focus == SettingsFocus::StatusBar
                && let Some(action) = self.handle_status_bar_keys(key, ctx, resolver)
            {
                return action;
            }

            if self.focus == SettingsFocus::QrLogin {
                let login_sources = self.qr_login_sources(ctx);
                if !login_sources.is_empty() {
                    match (key.modifiers, key.code) {
                        (KeyModifiers::NONE, KeyCode::Up) => {
                            self.qr_login_source_index = self
                                .qr_login_source_index
                                .checked_sub(1)
                                .unwrap_or(login_sources.len() - 1);
                            return AppAction::None;
                        }
                        (KeyModifiers::NONE, KeyCode::Down) => {
                            self.qr_login_source_index =
                                (self.qr_login_source_index + 1) % login_sources.len();
                            return AppAction::None;
                        }
                        (KeyModifiers::NONE, KeyCode::Enter)
                        | (KeyModifiers::NONE, KeyCode::Char('p' | 'P')) => {
                            let source =
                                login_sources[self.qr_login_source_index % login_sources.len()];
                            if ctx.source_manager.is_logged_in(source) {
                                self.status_msg = Some(format!(
                                    "{} 已登录，按 b 退出登录",
                                    source.display_name()
                                ));
                            } else {
                                return AppAction::QrLogin(source);
                            }
                            return AppAction::None;
                        }
                        _ => {}
                    }
                }
            }

            let sources = ctx
                .config
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .source
                .js_sources
                .clone();

            if let Some(action) = bound_action {
                match action {
                    Action::ListSelectUp => {
                        if self.selected_source > 0 {
                            self.selected_source -= 1;
                        }
                        return AppAction::None;
                    }
                    Action::ListSelectDown => {
                        if self.selected_source + 1 < sources.len() {
                            self.selected_source += 1;
                        }
                        return AppAction::None;
                    }
                    _ => {}
                }
            }

            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Char('a')) => {
                    self.input_mode = true;
                    self.status_msg = None;
                }
                (KeyModifiers::NONE, KeyCode::Up) => {
                    if self.selected_source > 0 {
                        self.selected_source -= 1;
                    }
                }
                (KeyModifiers::NONE, KeyCode::Down) => {
                    if self.selected_source + 1 < sources.len() {
                        self.selected_source += 1;
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('d')) => {
                    if !sources.is_empty() && self.selected_source < sources.len() {
                        // 二次确认：首次按 d 只武装并提示，窗口内再按一次才真正删除
                        let now = Instant::now();
                        let confirmed = matches!(
                            self.delete_source_armed,
                            Some(armed_at)
                                if now.duration_since(armed_at) <= DELETE_CONFIRM_WINDOW
                        );
                        if !confirmed {
                            self.delete_source_armed = Some(now);
                            self.status_msg =
                                Some("再按一次 d 确认删除该音源，Esc 取消".to_string());
                            return AppAction::None;
                        }
                        self.delete_source_armed = None;
                        let url = sources[self.selected_source].clone();
                        self.status_msg = Some("已移除音源".to_string());
                        if self.selected_source >= sources.len().saturating_sub(1) {
                            self.selected_source = self.selected_source.saturating_sub(1);
                        }
                        return AppAction::RemoveSource(url);
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('h'))
                    if self.focus == SettingsFocus::JsSources =>
                {
                    self.status_msg = Some("正在检测音源…".to_string());
                    return AppAction::CheckSourceHealth;
                }
                (KeyModifiers::NONE, KeyCode::Char('t')) => {
                    self.update_config(ctx, |config| {
                        config.ui.enable_mouse = !config.ui.enable_mouse;
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('g')) => {
                    self.update_config(ctx, |config| {
                        config.ui.aggregate_search = !config.ui.aggregate_search;
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('w')) => {
                    self.update_config(ctx, |config| {
                        config.ui.wrap_navigation = !config.ui.wrap_navigation;
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('c')) => {
                    self.update_config(ctx, |config| {
                        config.ui.show_cover = !config.ui.show_cover;
                    });
                    if !ctx
                        .config
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .ui
                        .show_cover
                    {
                        ctx.cover_service.clear();
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('e')) => {
                    let enabled = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.player.remember_playback_state =
                            !config.player.remember_playback_state;
                        let enabled = config.player.remember_playback_state;
                        let result = crate::config::loader::save(&config, &ctx.config_path);
                        self.status_msg = Some(match result {
                            Ok(()) => "设置已保存".to_string(),
                            Err(error) => format!("保存设置失败: {}", error),
                        });
                        enabled
                    };
                    let result = if enabled {
                        ctx.persist_playback_session()
                    } else {
                        ctx.storage.clear_playback_session()
                    };
                    if let Err(error) = result {
                        self.status_msg =
                            Some(format!("播放状态设置已更新，但会话保存失败: {error}"));
                    }
                }
                (KeyModifiers::SHIFT, KeyCode::Char('Q' | 'q'))
                | (KeyModifiers::NONE, KeyCode::Char('Q')) => {
                    self.update_config(ctx, |config| {
                        config.player.quality = next_quality(config.player.quality);
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('H' | 'h'))
                | (KeyModifiers::NONE, KeyCode::Char('H')) => {
                    let limit = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.player.history_limit =
                            next_history_limit(config.player.history_limit);
                        let limit = config.player.history_limit;
                        let result = crate::config::loader::save(&config, &ctx.config_path);
                        self.status_msg = Some(match result {
                            Ok(()) => format!("历史上限: {limit}"),
                            Err(error) => format!("保存设置失败: {error}"),
                        });
                        limit
                    };
                    ctx.storage.trim_history(limit);
                }
                (KeyModifiers::NONE, KeyCode::Char('v')) => {
                    self.cycle_default_source(ctx);
                }
                (KeyModifiers::NONE, KeyCode::Char('u')) => {
                    self.update_config(ctx, |config| {
                        config.source.auto_toggle = !config.source.auto_toggle;
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('y')) => {
                    self.enabled_source_index =
                        (self.enabled_source_index + 1) % SourceId::all_online().len();
                }
                (KeyModifiers::SHIFT, KeyCode::Char('K' | 'k'))
                | (KeyModifiers::NONE, KeyCode::Char('K')) => {
                    self.toggle_selected_source(ctx);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('T' | 't'))
                | (KeyModifiers::NONE, KeyCode::Char('T')) => {
                    let enabled = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.lyric.show_translation = !config.lyric.show_translation;
                        let enabled = config.lyric.show_translation;
                        self.status_msg =
                            save_status(crate::config::loader::save(&config, &ctx.config_path));
                        enabled
                    };
                    ctx.lyric_service.set_translation_enabled(enabled);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('Y' | 'y'))
                | (KeyModifiers::NONE, KeyCode::Char('Y')) => {
                    let enabled = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.lyric.show_yrc = !config.lyric.show_yrc;
                        let enabled = config.lyric.show_yrc;
                        self.status_msg =
                            save_status(crate::config::loader::save(&config, &ctx.config_path));
                        enabled
                    };
                    ctx.lyric_service.set_yrc_enabled(enabled);
                }
                (KeyModifiers::NONE, KeyCode::Char('[')) => {
                    self.adjust_lyric_offset(ctx, -100);
                }
                (KeyModifiers::NONE, KeyCode::Char(']')) => {
                    self.adjust_lyric_offset(ctx, 100);
                }
                (KeyModifiers::NONE, KeyCode::Char('n')) => {
                    self.proxy_input = ctx
                        .config
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .network
                        .proxy_url
                        .clone();
                    self.proxy_input_mode = true;
                    self.status_msg = None;
                }
                (KeyModifiers::SHIFT, KeyCode::Char('N' | 'n'))
                | (KeyModifiers::NONE, KeyCode::Char('N')) => {
                    let (proxy, timeout) = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.network.timeout = next_network_timeout(config.network.timeout);
                        let values = (config.network.proxy_url.clone(), config.network.timeout);
                        self.status_msg =
                            save_status(crate::config::loader::save(&config, &ctx.config_path));
                        values
                    };
                    lx_source::configure_network(&proxy, timeout);
                }
                (KeyModifiers::SHIFT, KeyCode::Char('P' | 'p'))
                | (KeyModifiers::NONE, KeyCode::Char('P')) => {
                    self.update_config(ctx, |config| {
                        config.ui.cover_protocol =
                            next_cover_protocol(&config.ui.cover_protocol).to_string();
                    });
                    if self.status_msg.as_deref() == Some("设置已保存") {
                        self.status_msg = Some("封面协议已保存，下次启动生效".to_string());
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('f')) => {
                    self.update_config(ctx, |config| {
                        config.ui.max_fps = next_fps(config.ui.max_fps);
                    });
                    if self.status_msg.as_deref() == Some("设置已保存") {
                        self.status_msg = Some("刷新率已保存，下次启动生效".to_string());
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('z')) => {
                    self.update_config(ctx, |config| {
                        config.ui.scroll_amount = next_scroll_amount(config.ui.scroll_amount);
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('i')) => {
                    self.update_config(ctx, |config| {
                        config.integration.mpris = !config.integration.mpris;
                    });
                    if self.status_msg.as_deref() == Some("设置已保存") {
                        self.status_msg = Some("MPRIS 设置已保存，下次启动生效".to_string());
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('o')) => {
                    self.update_config(ctx, |config| {
                        config.notification.in_app = !config.notification.in_app;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('O' | 'o'))
                | (KeyModifiers::NONE, KeyCode::Char('O')) => {
                    self.update_config(ctx, |config| {
                        config.notification.in_app_timeout =
                            match config.notification.in_app_timeout {
                                0..=2 => 4,
                                3..=4 => 6,
                                5..=6 => 8,
                                _ => 2,
                            };
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('x')) => {
                    self.update_config(ctx, |config| {
                        config.notification.enable = !config.notification.enable;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('X' | 'x'))
                | (KeyModifiers::NONE, KeyCode::Char('X')) => {
                    self.update_config(ctx, |config| {
                        config.notification.album_cover = !config.notification.album_cover;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('R' | 'r'))
                | (KeyModifiers::NONE, KeyCode::Char('R')) => {
                    self.update_config(ctx, |config| {
                        config.notification.track_change = !config.notification.track_change;
                    });
                }
                // --- 下载设置 ---
                (KeyModifiers::SHIFT, KeyCode::Char('S' | 's'))
                | (KeyModifiers::NONE, KeyCode::Char('S')) => {
                    self.download_input = ctx.downloads.download_dir().display().to_string();
                    self.download_input_target = Some(DownloadInputTarget::Dir);
                    self.status_msg = Some("输入下载目录，Enter 保存，Esc 取消".to_string());
                }
                (KeyModifiers::SHIFT, KeyCode::Char('M' | 'm'))
                | (KeyModifiers::NONE, KeyCode::Char('M')) => {
                    self.download_input = ctx
                        .config
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .download
                        .filename_template
                        .clone();
                    self.download_input_target = Some(DownloadInputTarget::Template);
                    self.status_msg = Some(
                        "输入文件名模板，支持 {name} {singer} {album} {source} {quality}"
                            .to_string(),
                    );
                }
                (KeyModifiers::SHIFT, KeyCode::Char('F' | 'f'))
                | (KeyModifiers::NONE, KeyCode::Char('F')) => {
                    self.update_config(ctx, |config| {
                        config.download.quality = match config.download.quality {
                            None => Some(config.player.quality),
                            Some(Quality::Low128) => Some(Quality::High320),
                            Some(Quality::High320) => Some(Quality::Flac),
                            Some(Quality::Flac) => Some(Quality::Flac24),
                            Some(Quality::Flac24) => None,
                        };
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('B' | 'b'))
                | (KeyModifiers::NONE, KeyCode::Char('B')) => {
                    self.update_config(ctx, |config| {
                        config.download.multipart = !config.download.multipart;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('V' | 'v'))
                | (KeyModifiers::NONE, KeyCode::Char('V')) => {
                    self.update_config(ctx, |config| {
                        config.download.multipart_min_size_mb = next_step(
                            &[1, 2, 5, 10, 20, 50],
                            config.download.multipart_min_size_mb,
                        );
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('W' | 'w'))
                | (KeyModifiers::NONE, KeyCode::Char('W')) => {
                    self.update_config(ctx, |config| {
                        config.download.concurrency =
                            next_step(&[1, 2, 4, 8, 16], config.download.concurrency as u64)
                                as usize;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('A' | 'a'))
                | (KeyModifiers::NONE, KeyCode::Char('A')) => {
                    self.update_config(ctx, |config| {
                        config.download.concurrent_songs =
                            next_step(&[1, 2, 3, 4], config.download.concurrent_songs as u64)
                                as usize;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('E' | 'e'))
                | (KeyModifiers::NONE, KeyCode::Char('E')) => {
                    self.update_config(ctx, |config| {
                        config.download.max_retries =
                            next_step(&[0, 1, 2, 3, 5], config.download.max_retries as u64) as u32;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('U' | 'u'))
                | (KeyModifiers::NONE, KeyCode::Char('U')) => {
                    self.update_config(ctx, |config| {
                        config.download.verify_size = !config.download.verify_size;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('L' | 'l'))
                | (KeyModifiers::NONE, KeyCode::Char('L')) => {
                    self.update_config(ctx, |config| {
                        config.download.skip_existing = !config.download.skip_existing;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('J' | 'j'))
                | (KeyModifiers::NONE, KeyCode::Char('J')) => {
                    self.update_config(ctx, |config| {
                        config.download.write_tags = !config.download.write_tags;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('G' | 'g'))
                | (KeyModifiers::NONE, KeyCode::Char('G')) => {
                    self.update_config(ctx, |config| {
                        config.download.embed_cover = !config.download.embed_cover;
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('I' | 'i'))
                | (KeyModifiers::NONE, KeyCode::Char('I')) => {
                    self.update_config(ctx, |config| {
                        config.download.save_lyric = !config.download.save_lyric;
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('m')) => {
                    let mode = ctx.playlist.cycle_mode();
                    let result = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.player.play_mode = mode.as_config().to_string();
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    self.status_msg = Some(match result {
                        Ok(()) => format!("播放模式: {}", mode.label()),
                        Err(error) => format!("播放模式已切换，但保存失败: {}", error),
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('D' | 'd'))
                | (KeyModifiers::NONE, KeyCode::Char('D')) => {
                    self.update_config(ctx, |config| {
                        config.local_music.max_depth =
                            next_scan_depth(config.local_music.max_depth);
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('b')) => {
                    let login_sources = self.qr_login_sources(ctx);
                    if login_sources.is_empty() {
                        self.status_msg = Some("当前没有可用的扫码登录音源".to_string());
                    } else {
                        self.qr_login_source_index %= login_sources.len();
                        let source = login_sources[self.qr_login_source_index];
                        self.qr_login_source_index =
                            (self.qr_login_source_index + 1) % login_sources.len();
                        if ctx.source_manager.is_logged_in(source) {
                            return AppAction::QrLogout(source);
                        }
                        return AppAction::QrLogin(source);
                    }
                }
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    // Esc 取消删除类操作（音源 / 本地目录）的武装状态
                    self.delete_source_armed = None;
                    self.delete_local_path_armed = None;
                }
                _ => {}
            }
        }
        AppAction::None
    }

    /// 执行通过 settings 页面级键位映射解析出的播放与数据动作。
    /// 返回 `None` 表示该动作由其他设置页逻辑处理。
    fn handle_bound_action(&mut self, action: Action, ctx: &AppContext) -> Option<AppAction> {
        match action {
            Action::SettingsCyclePlaybackSpeed => {
                self.status_msg = Some(ctx.cycle_playback_speed());
            }
            Action::SettingsEditAudioDevice => {
                self.audio_device_input = ctx
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .player
                    .audio_device
                    .clone();
                self.audio_device_input_mode = true;
                self.status_msg = Some("输入 libmpv 音频设备名，Enter 保存".to_string());
            }
            Action::SettingsCycleReplayGainMode => {
                self.status_msg = Some(ctx.cycle_replaygain_mode());
            }
            Action::SettingsCycleReplayGainPreamp => {
                self.status_msg = Some(ctx.cycle_replaygain_preamp());
            }
            Action::SettingsCycleChannelMode => {
                self.status_msg = Some(ctx.cycle_channel_mode());
            }
            Action::SettingsCycleBalance => {
                self.status_msg = Some(ctx.cycle_balance());
            }
            Action::SettingsToggleReplayGainClip => {
                self.status_msg = Some(ctx.toggle_replaygain_clip());
            }
            Action::SettingsCycleFadeInDuration => {
                let duration = {
                    let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                    config.player.fade_in_ms = next_fade_duration(config.player.fade_in_ms);
                    let value = config.player.fade_in_ms;
                    self.status_msg =
                        save_status(crate::config::loader::save(&config, &ctx.config_path));
                    value
                };
                self.status_msg = Some(format!("淡入: {}", fade_label(duration)));
            }
            Action::SettingsCycleFadeOutDuration => {
                let duration = {
                    let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                    config.player.fade_out_ms = next_fade_duration(config.player.fade_out_ms);
                    let value = config.player.fade_out_ms;
                    self.status_msg =
                        save_status(crate::config::loader::save(&config, &ctx.config_path));
                    value
                };
                self.status_msg = Some(format!("淡出: {}", fade_label(duration)));
            }
            Action::SettingsCycleEqualizerPreset => {
                self.status_msg = Some(ctx.cycle_equalizer_preset());
            }
            Action::SettingsRunFadeIn => {
                self.status_msg = Some(ctx.fade_in_now());
            }
            Action::SettingsRunFadeOut => {
                self.status_msg = Some(ctx.fade_out_now());
            }
            Action::SettingsSetAbLoopStart => {
                self.status_msg = Some(ctx.set_ab_loop_start_now());
            }
            Action::SettingsSetAbLoopEnd => {
                self.status_msg = Some(ctx.set_ab_loop_end_now());
            }
            Action::SettingsClearAbLoop => {
                self.status_msg = Some(ctx.clear_ab_loop());
            }
            Action::SettingsExportData => {
                self.status_msg = Some(match ctx.storage.export_default() {
                    Ok(path) => format!("数据已导出: {}", path.display()),
                    Err(error) => format!("数据导出失败: {error}"),
                });
            }
            Action::SettingsImportData => {
                self.status_msg = Some(match ctx.storage.import_default() {
                    Ok(path) => format!("数据已导入，原数据备份于: {}", path.display()),
                    Err(error) => format!("数据导入失败: {error}"),
                });
            }
            Action::SettingsImportPlaylist => {
                self.playlist_import_input.clear();
                self.playlist_import_mode = true;
                self.status_msg = Some("输入 M3U/LX Music/网易云歌单路径，Enter 导入".to_string());
            }
            _ => return None,
        }
        Some(AppAction::None)
    }

    fn handle_proxy_input(&mut self, key: KeyEvent, ctx: &AppContext) -> AppAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.proxy_input_mode = false;
                self.proxy_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let proxy = self.proxy_input.trim().to_string();
                let timeout = {
                    let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                    config.network.proxy_url = proxy.clone();
                    let timeout = config.network.timeout;
                    self.status_msg =
                        save_status(crate::config::loader::save(&config, &ctx.config_path));
                    timeout
                };
                lx_source::configure_network(&proxy, timeout);
                self.proxy_input_mode = false;
                self.proxy_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.proxy_input.pop();
            }
            (modifiers, KeyCode::Char(c))
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.proxy_input.push(c);
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_audio_device_input(&mut self, key: KeyEvent, ctx: &AppContext) -> AppAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.audio_device_input_mode = false;
                self.audio_device_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let device = self.audio_device_input.trim().to_string();
                if !device.is_empty() {
                    self.status_msg = Some(ctx.set_audio_output_device(&device));
                }
                self.audio_device_input_mode = false;
                self.audio_device_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.audio_device_input.pop();
            }
            (modifiers, KeyCode::Char(c))
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && c != '\0' =>
            {
                self.audio_device_input.push(c);
            }
            _ => {}
        }
        AppAction::None
    }

    /// 下载目录 / 文件名模板的文本输入。
    fn handle_download_input(&mut self, key: KeyEvent, ctx: &AppContext) -> AppAction {
        let Some(target) = self.download_input_target else {
            return AppAction::None;
        };
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.download_input_target = None;
                self.download_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let value = self.download_input.trim().to_string();
                self.download_input_target = None;
                self.download_input.clear();
                match target {
                    DownloadInputTarget::Dir => {
                        let dir = crate::download::naming::resolve_download_dir(&value);
                        self.update_config(ctx, |config| {
                            config.download.dir = value.clone();
                        });
                        self.status_msg = Some(format!("下载目录: {}", dir.display()));
                    }
                    DownloadInputTarget::Template => {
                        if value.is_empty() {
                            return AppAction::None;
                        }
                        self.update_config(ctx, |config| {
                            config.download.filename_template = value.clone();
                        });
                        self.status_msg = Some(format!("文件名模板: {value}"));
                    }
                }
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.download_input.pop();
            }
            (modifiers, KeyCode::Char(c))
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && c != '\0' =>
            {
                self.download_input.push(c);
            }
            _ => {}
        }
        AppAction::None
    }

    fn handle_playlist_import_input(&mut self, key: KeyEvent, _ctx: &AppContext) -> AppAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.playlist_import_mode = false;
                self.playlist_import_input.clear();
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                let path = self.playlist_import_input.trim().to_string();
                self.playlist_import_mode = false;
                self.playlist_import_input.clear();
                if path.is_empty() {
                    return AppAction::None;
                }
                // 导入在后台任务中完成（解析大歌单 + 一次性写盘），
                // 完成后通过通知汇报结果，避免阻塞 TUI 主循环。
                return AppAction::ImportExternalPlaylist(path);
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.playlist_import_input.pop();
            }
            (modifiers, KeyCode::Char(c))
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.playlist_import_input.push(c);
            }
            _ => {}
        }
        AppAction::None
    }

    fn cycle_default_source(&mut self, ctx: &AppContext) {
        let (default, enabled) = {
            let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
            (config.source.default, config.source.enabled.clone())
        };
        if enabled.is_empty() {
            self.status_msg = Some("请先启用至少一个在线音源".to_string());
            return;
        }
        let current = enabled
            .iter()
            .position(|source| *source == default)
            .unwrap_or(0);
        let default = enabled[(current + 1) % enabled.len()];
        let save_result = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            config.source.default = default;
            crate::config::loader::save(&config, &ctx.config_path)
        };
        ctx.source_manager
            .update_source_preferences(default, &enabled);
        self.status_msg = Some(match save_result {
            Ok(()) => format!("默认音源: {}", default.as_str()),
            Err(error) => format!("默认音源已切换，但保存失败: {error}"),
        });
    }

    fn toggle_selected_source(&mut self, ctx: &AppContext) {
        // 与渲染处一致对列表长度取模，防止索引越界 panic
        let sources = SourceId::all_online();
        self.enabled_source_index %= sources.len();
        let source = sources[self.enabled_source_index];
        let (default, enabled, save_result) = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            if config.source.enabled.contains(&source) {
                if config.source.enabled.len() == 1 {
                    self.status_msg = Some("至少需要保留一个在线音源".to_string());
                    return;
                }
                config.source.enabled.retain(|item| *item != source);
                if config.source.default == source {
                    config.source.default = config.source.enabled[0];
                }
            } else {
                config.source.enabled.push(source);
                config.source.enabled.sort_by_key(|item| {
                    SourceId::all_online()
                        .iter()
                        .position(|candidate| candidate == item)
                        .unwrap_or(usize::MAX)
                });
            }
            let default = config.source.default;
            let enabled = config.source.enabled.clone();
            let save_result = crate::config::loader::save(&config, &ctx.config_path);
            (default, enabled, save_result)
        };
        ctx.source_manager
            .update_source_preferences(default, &enabled);
        self.status_msg = Some(match save_result {
            Ok(()) => format!(
                "{}音源 {}",
                source.as_str(),
                if enabled.contains(&source) {
                    "已启用"
                } else {
                    "已禁用"
                }
            ),
            Err(error) => format!("音源设置已更新，但保存失败: {error}"),
        });
    }

    fn adjust_lyric_offset(&mut self, ctx: &AppContext, delta: i32) {
        let offset = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            config.lyric.offset = config
                .lyric
                .offset
                .saturating_add(delta)
                .clamp(-5_000, 5_000);
            let offset = config.lyric.offset;
            self.status_msg = save_status(crate::config::loader::save(&config, &ctx.config_path));
            offset
        };
        ctx.lyric_service.set_offset_ms(offset);
    }

    /// 处理本地音乐路径输入模式
    fn handle_local_path_input(&mut self, key: KeyEvent, ctx: &AppContext) -> AppAction {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.local_path_mode = false;
                self.local_path_input.clear();
                AppAction::None
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                if !self.local_path_input.trim().is_empty() {
                    let path = self.local_path_input.trim().to_string();
                    let save_result = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        if !config.local_music.paths.contains(&path) {
                            config.local_music.paths.push(path.clone());
                            config.local_music.enabled = true;
                        }
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    let (paths, max_depth) = {
                        let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
                        (
                            config.local_music.paths.clone(),
                            config.local_music.max_depth,
                        )
                    };
                    self.local_path_mode = false;
                    self.local_path_input.clear();
                    if let Err(error) = save_result {
                        self.status_msg = Some(format!("目录已添加，但保存失败: {}", error));
                    } else {
                        self.status_msg = Some("正在扫描本地音乐...".to_string());
                    }
                    return AppAction::ScanLocalMusic {
                        paths,
                        max_depth,
                        force: true,
                    };
                }
                AppAction::None
            }
            (modifiers, KeyCode::Char(c))
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.local_path_input.push(c);
                AppAction::None
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.local_path_input.pop();
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    /// 处理本地音乐区域的按键（非输入模式）
    fn handle_local_keys(
        &mut self,
        key: KeyEvent,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> Option<AppAction> {
        let paths = ctx
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .local_music
            .paths
            .clone();

        if let Some(action) = resolver.resolve_page("settings", &key) {
            match action {
                Action::ListSelectUp => {
                    if self.selected_local_path > 0 {
                        self.selected_local_path -= 1;
                    }
                    return Some(AppAction::None);
                }
                Action::ListSelectDown => {
                    if self.selected_local_path + 1 < paths.len() {
                        self.selected_local_path += 1;
                    }
                    return Some(AppAction::None);
                }
                _ => {}
            }
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('a')) => {
                self.local_path_mode = true;
                self.local_path_input.clear();
                self.status_msg = None;
                Some(AppAction::None)
            }
            (KeyModifiers::NONE, KeyCode::Up) => {
                if self.selected_local_path > 0 {
                    self.selected_local_path -= 1;
                }
                Some(AppAction::None)
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                if self.selected_local_path + 1 < paths.len() {
                    self.selected_local_path += 1;
                }
                Some(AppAction::None)
            }
            (KeyModifiers::NONE, KeyCode::Char('d')) => {
                if !paths.is_empty() && self.selected_local_path < paths.len() {
                    // 二次确认：首次按 d 只武装并提示，窗口内再按一次才真正删除
                    let now = Instant::now();
                    let confirmed = matches!(
                        self.delete_local_path_armed,
                        Some(armed_at) if now.duration_since(armed_at) <= DELETE_CONFIRM_WINDOW
                    );
                    if !confirmed {
                        self.delete_local_path_armed = Some(now);
                        self.status_msg =
                            Some("再按一次 d 确认删除该本地目录，Esc 取消".to_string());
                        return Some(AppAction::None);
                    }
                    self.delete_local_path_armed = None;
                    let removed = paths[self.selected_local_path].clone();
                    let save_result = {
                        let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
                        config.local_music.paths.retain(|p| p != &removed);
                        config.local_music.enabled = !config.local_music.paths.is_empty();
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    if self.selected_local_path >= paths.len().saturating_sub(1) {
                        self.selected_local_path = self.selected_local_path.saturating_sub(1);
                    }
                    let (remaining, max_depth) = {
                        let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
                        (
                            config.local_music.paths.clone(),
                            config.local_music.max_depth,
                        )
                    };
                    self.status_msg = Some(match save_result {
                        Ok(()) => format!("已移除 {}，正在重新扫描...", removed),
                        Err(error) => format!("已移除，但保存失败: {}", error),
                    });
                    return Some(AppAction::ScanLocalMusic {
                        paths: remaining,
                        max_depth,
                        force: true,
                    });
                }
                Some(AppAction::None)
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                let max_depth = ctx
                    .config
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .local_music
                    .max_depth;
                self.status_msg = Some("正在扫描本地音乐...".to_string());
                Some(AppAction::ScanLocalMusic {
                    paths,
                    max_depth,
                    force: true,
                })
            }
            _ => None,
        }
    }

    fn handle_status_bar_keys(
        &mut self,
        key: KeyEvent,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> Option<AppAction> {
        let item_count = StatusBarItem::ALL.len();
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Enter | KeyCode::Char(' ')) => {
                self.toggle_status_bar_item(ctx);
                return Some(AppAction::None);
            }
            (KeyModifiers::SHIFT, KeyCode::Left | KeyCode::Up) => {
                self.move_status_bar_item(ctx, -1);
                return Some(AppAction::None);
            }
            (KeyModifiers::SHIFT, KeyCode::Right | KeyCode::Down) => {
                self.move_status_bar_item(ctx, 1);
                return Some(AppAction::None);
            }
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.selected_status_item = self.selected_status_item.saturating_sub(1);
                return Some(AppAction::None);
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.selected_status_item =
                    (self.selected_status_item + 1).min(item_count.saturating_sub(1));
                return Some(AppAction::None);
            }
            (KeyModifiers::NONE, KeyCode::Char('a' | 'd' | 'r')) => {
                return Some(AppAction::None);
            }
            _ => {}
        }

        if let Some(action) = resolver.resolve_page("settings", &key) {
            match action {
                Action::ListSelectUp => {
                    self.selected_status_item = self.selected_status_item.saturating_sub(1);
                    return Some(AppAction::None);
                }
                Action::ListSelectDown => {
                    self.selected_status_item =
                        (self.selected_status_item + 1).min(item_count.saturating_sub(1));
                    return Some(AppAction::None);
                }
                _ => {}
            }
        }
        None
    }

    fn toggle_status_bar_item(&mut self, ctx: &AppContext) {
        let item = StatusBarItem::ALL[self.selected_status_item % StatusBarItem::ALL.len()];
        let (enabled, result) = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            if config.ui.status_bar_items.contains(&item) {
                config
                    .ui
                    .status_bar_items
                    .retain(|candidate| *candidate != item);
                let result = crate::config::loader::save(&config, &ctx.config_path);
                (false, result)
            } else {
                config.ui.status_bar_items.push(item);
                let result = crate::config::loader::save(&config, &ctx.config_path);
                (true, result)
            }
        };
        self.status_msg = Some(match result {
            Ok(()) => format!(
                "状态栏“{}”已{}",
                status_bar_item_label(item),
                if enabled { "显示" } else { "隐藏" }
            ),
            Err(error) => format!("状态栏已更新，但保存失败: {error}"),
        });
    }

    fn move_status_bar_item(&mut self, ctx: &AppContext, direction: isize) {
        let item = StatusBarItem::ALL[self.selected_status_item % StatusBarItem::ALL.len()];
        let (position, result) = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            let Some(index) = config
                .ui
                .status_bar_items
                .iter()
                .position(|candidate| *candidate == item)
            else {
                self.status_msg = Some("请先启用这个状态栏字段".to_string());
                return;
            };
            let new_index = if direction < 0 {
                index.saturating_sub(1)
            } else {
                (index + 1).min(config.ui.status_bar_items.len().saturating_sub(1))
            };
            if new_index == index {
                return;
            }
            config.ui.status_bar_items.swap(index, new_index);
            let result = crate::config::loader::save(&config, &ctx.config_path);
            (new_index + 1, result)
        };
        self.status_msg = Some(match result {
            Ok(()) => format!(
                "状态栏“{}”已移到第 {position} 位",
                status_bar_item_label(item)
            ),
            Err(error) => format!("状态栏顺序已更新，但保存失败: {error}"),
        });
    }

    fn move_status_bar_item_to(&mut self, ctx: &AppContext, target_index: usize) {
        let item = StatusBarItem::ALL[self.selected_status_item % StatusBarItem::ALL.len()];
        let Some(&target) = StatusBarItem::ALL.get(target_index) else {
            return;
        };
        if item == target {
            return;
        }

        let (position, result) = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            if !config.ui.status_bar_items.contains(&item) {
                self.status_msg = Some("请先启用这个状态栏字段".to_string());
                return;
            }
            let Some(position) =
                reorder_status_bar_items(&mut config.ui.status_bar_items, item, target)
            else {
                // Disabled fields have no display-order position to drop on.
                return;
            };
            let result = crate::config::loader::save(&config, &ctx.config_path);
            (position + 1, result)
        };
        self.status_msg = Some(match result {
            Ok(()) => format!(
                "状态栏“{}”已移到第 {position} 位",
                status_bar_item_label(item)
            ),
            Err(error) => format!("状态栏顺序已更新，但保存失败: {error}"),
        });
    }

    fn render_qr_login_panel(
        &self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &AppContext,
        accent: Color,
        muted: Color,
        sources: &[SourceId],
    ) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == SettingsFocus::QrLogin {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" 账号与网易云 · ↑/↓选择 · P扫码 · B退出 · Shift+S刷新远程歌单 ");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }
        let all_qr = self.qr_login_sources(ctx);
        let selected_qr = all_qr
            .get(self.qr_login_source_index % all_qr.len().max(1))
            .copied();
        let mut row = inner.y;
        let summary = format!(
            " 支持扫码: {}  · 已登录: {}",
            sources
                .iter()
                .filter(|source| ctx.source_manager.capabilities(**source).qr_login)
                .count(),
            sources
                .iter()
                .filter(|source| ctx.source_manager.is_logged_in(**source))
                .count(),
        );
        Paragraph::new(Line::from(Span::styled(summary, Style::new().fg(muted))))
            .render(Rect::new(inner.x, row, inner.width, 1), buf);
        row = row.saturating_add(1);

        for source in sources {
            if row >= inner.bottom() {
                break;
            }
            let capabilities = ctx.source_manager.capabilities(*source);
            let logged_in = ctx.source_manager.is_logged_in(*source);
            let qr_selected = selected_qr == Some(*source);
            let (status, status_color) = if !capabilities.qr_login {
                ("— 不支持扫码", muted)
            } else if logged_in {
                ("✓ 已登录", crate::theme::green(ctx))
            } else {
                ("○ 可扫码 / 未登录", crate::theme::yellow(ctx))
            };
            let style = if qr_selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::text(ctx))
            };
            let line = Line::from(vec![
                Span::styled(format!(" {:<12} ", source.display_name()), style),
                Span::styled(status, Style::new().fg(status_color)),
            ]);
            Paragraph::new(line).render(Rect::new(inner.x, row, inner.width, 1), buf);
            row = row.saturating_add(1);
        }

        // 网易云远程集合是播放数据，不写入本地歌单；必须在设置里明确可见。
        // 渲染路径不拷贝整个缓存，只取计数与当前可见行的文本。
        let (cached_total, cached_normal, cached_favorites) = crate::remote_cache::summary_counts();
        if row < inner.bottom() {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    " 网易云远程歌单",
                    Style::new().fg(accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  · 已缓存 {cached_total} 个（普通 {cached_normal} / 红心 {cached_favorites}）  · S 刷新"
                    ),
                    Style::new().fg(muted),
                ),
            ]))
            .render(Rect::new(inner.x, row, inner.width, 1), buf);
            row = row.saturating_add(1);
        }

        if row < inner.bottom() {
            if cached_total == 0 {
                Paragraph::new(" 尚未读取网易云远程歌单。登录后按 S 刷新。")
                    .style(Style::new().fg(crate::theme::yellow(ctx)))
                    .render(Rect::new(inner.x, row, inner.width, 1), buf);
            } else {
                let room = inner.bottom().saturating_sub(row) as usize;
                let width = inner.width.saturating_sub(20) as usize;
                let lines = crate::remote_cache::with_netease(|collections| {
                    collections
                        .iter()
                        .take(room)
                        .map(|collection| {
                            let kind = if matches!(
                                collection.kind,
                                lx_core::sync::SyncCollectionKind::Favorites
                            ) {
                                "红心"
                            } else {
                                "歌单"
                            };
                            format!(
                                " {}  {}  · {} 首",
                                kind,
                                truncate_display(&collection.name, width),
                                collection.songs.len()
                            )
                        })
                        .collect::<Vec<_>>()
                });
                for line in lines {
                    Paragraph::new(Line::from(Span::styled(
                        line,
                        Style::new().fg(crate::theme::text(ctx)),
                    )))
                    .render(Rect::new(inner.x, row, inner.width, 1), buf);
                    row = row.saturating_add(1);
                }
            }
        }
    }

    fn update_config(
        &mut self,
        ctx: &AppContext,
        update: impl FnOnce(&mut lx_core::model::config::Config),
    ) {
        let result = {
            let mut config = ctx.config.write().unwrap_or_else(|e| e.into_inner());
            update(&mut config);
            // 下载目录/分片等参数变化后立即生效，无需重启。
            ctx.downloads.sync_config(&config);
            crate::config::loader::save(&config, &ctx.config_path)
        };
        self.status_msg = Some(match result {
            Ok(()) => "设置已保存".to_string(),
            Err(error) => format!("保存设置失败: {}", error),
        });
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
        let sources = &config.source.js_sources;
        let local_paths = &config.local_music.paths;
        let accent = crate::theme::accent(ctx);
        let muted = crate::theme::muted(ctx);
        let chunks = settings_chunks_with(area, self.focus, self.category, self.layout);
        let divider_lines = settings_dividers(area, self.focus, self.category, self.layout);
        let proxy_label = if config.network.proxy_url.is_empty() {
            "未设置".to_string()
        } else {
            shorten_source(&config.network.proxy_url, 18)
        };
        let scan_depth_label = if config.local_music.max_depth == 0 {
            "不限".to_string()
        } else {
            config.local_music.max_depth.to_string()
        };
        let ab_loop = ctx.player.ab_loop();
        let ab_start_label = ab_loop
            .map(|points| format_duration(points.start))
            .unwrap_or_else(|| "未设置".to_string());
        let ab_end_label = ab_loop
            .map(|points| format_duration(points.end))
            .unwrap_or_else(|| "未设置".to_string());

        let options_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::border(ctx)))
            .title(format!(" 设置 · {}  [←/→切换分类] ", self.category.label()));
        let options_inner = options_block.inner(chunks[0]);
        options_block.render(chunks[0], buf);
        let mut options = vec![
            setting_line("鼠标控制", config.ui.enable_mouse, "t", accent, muted),
            setting_line("聚合搜索", config.ui.aggregate_search, "g", accent, muted),
            setting_line("循环导航", config.ui.wrap_navigation, "w", accent, muted),
            setting_line("封面显示", config.ui.show_cover, "c", accent, muted),
            setting_line(
                "保留播放状态",
                config.player.remember_playback_state,
                "e",
                accent,
                muted,
            ),
            setting_value_line(
                "播放音质",
                config.player.quality.label(),
                "Q",
                accent,
                muted,
            ),
            setting_value_line(
                "播放速度",
                &format!("{:.2}x", config.player.playback_speed),
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCyclePlaybackSpeed,
                    "F1",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "音频设备",
                &config.player.audio_device,
                settings_binding(&config.keybindings, Action::SettingsEditAudioDevice, "F2"),
                accent,
                muted,
            ),
            setting_value_line(
                "ReplayGain",
                &config.player.replaygain_mode,
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCycleReplayGainMode,
                    "F3",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "RG 预放大",
                &format!("{:+.1} dB", config.player.replaygain_preamp),
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCycleReplayGainPreamp,
                    "F5",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "声道模式",
                &config.player.channel_mode,
                settings_binding(&config.keybindings, Action::SettingsCycleChannelMode, "F4"),
                accent,
                muted,
            ),
            setting_value_line(
                "左右平衡",
                &format!("{:+.2}", config.player.balance),
                settings_binding(&config.keybindings, Action::SettingsCycleBalance, "F6"),
                accent,
                muted,
            ),
            setting_line(
                "ReplayGain 削波保护",
                config.player.replaygain_clip,
                settings_binding(
                    &config.keybindings,
                    Action::SettingsToggleReplayGainClip,
                    "F7",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "淡入时长",
                &fade_label(config.player.fade_in_ms),
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCycleFadeInDuration,
                    "F8",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "淡出时长",
                &fade_label(config.player.fade_out_ms),
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCycleFadeOutDuration,
                    "F9",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "均衡器",
                crate::context::equalizer_label(&config.player.equalizer_bands),
                settings_binding(
                    &config.keybindings,
                    Action::SettingsCycleEqualizerPreset,
                    "F10",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "淡入当前歌曲",
                "执行",
                settings_binding(&config.keybindings, Action::SettingsRunFadeIn, "Shift+F1"),
                accent,
                muted,
            ),
            setting_value_line(
                "淡出当前歌曲",
                "执行",
                settings_binding(&config.keybindings, Action::SettingsRunFadeOut, "Shift+F2"),
                accent,
                muted,
            ),
            setting_value_line(
                "A-B 循环起点",
                &ab_start_label,
                settings_binding(
                    &config.keybindings,
                    Action::SettingsSetAbLoopStart,
                    "Shift+F3",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "A-B 循环终点",
                &ab_end_label,
                settings_binding(
                    &config.keybindings,
                    Action::SettingsSetAbLoopEnd,
                    "Shift+F4",
                ),
                accent,
                muted,
            ),
            setting_value_line(
                "清除 A-B",
                if ab_loop.is_some() {
                    "执行"
                } else {
                    "未设置"
                },
                settings_binding(&config.keybindings, Action::SettingsClearAbLoop, "Shift+F5"),
                accent,
                muted,
            ),
            setting_value_line("播放模式", ctx.playlist.mode().label(), "m", accent, muted),
            setting_value_line(
                "历史上限",
                &config.player.history_limit.to_string(),
                "H",
                accent,
                muted,
            ),
            setting_value_line(
                "默认音源",
                config.source.default.as_str(),
                "v",
                accent,
                muted,
            ),
            setting_line("自动换源", config.source.auto_toggle, "u", accent, muted),
            {
                let source = SourceId::all_online()
                    [self.enabled_source_index % SourceId::all_online().len()];
                setting_value_line(
                    "音源开关",
                    &format!(
                        "{} {}",
                        source.as_str(),
                        enabled(config.source.enabled.contains(&source))
                    ),
                    "y/K",
                    accent,
                    muted,
                )
            },
            setting_line(
                "歌词翻译",
                config.lyric.show_translation,
                "T",
                accent,
                muted,
            ),
            setting_line("逐字歌词", config.lyric.show_yrc, "Y", accent, muted),
            setting_value_line(
                "歌词偏移",
                &format!("{:+} ms", config.lyric.offset),
                "[/]",
                accent,
                muted,
            ),
            setting_value_line("网络代理", &proxy_label, "n", accent, muted),
            setting_value_line(
                "网络超时",
                &format!("{} 秒", config.network.timeout),
                "N",
                accent,
                muted,
            ),
            setting_value_line("封面协议", &config.ui.cover_protocol, "P", accent, muted),
            setting_value_line(
                "最大 FPS",
                &config.ui.max_fps.to_string(),
                "f",
                accent,
                muted,
            ),
            setting_value_line(
                "滚动步长",
                &config.ui.scroll_amount.to_string(),
                "z",
                accent,
                muted,
            ),
            setting_line("MPRIS", config.integration.mpris, "i", accent, muted),
            setting_value_line(
                "TUI 通知",
                &format!(
                    "{} · {} 秒",
                    enabled(config.notification.in_app),
                    config.notification.in_app_timeout.clamp(1, 60)
                ),
                "o/O",
                accent,
                muted,
            ),
            setting_line("桌面通知", config.notification.enable, "x", accent, muted),
            setting_line(
                "通知封面",
                config.notification.album_cover,
                "X",
                accent,
                muted,
            ),
            setting_line(
                "切歌通知",
                config.notification.track_change,
                "R",
                accent,
                muted,
            ),
            setting_value_line("主题强调色", &config.theme.accent, "p", accent, muted),
            setting_value_line("扫描深度", &scan_depth_label, "D", accent, muted),
            setting_value_line(
                "导出数据",
                "voicefox-export.json",
                settings_binding(&config.keybindings, Action::SettingsExportData, "Shift+F6"),
                accent,
                muted,
            ),
            setting_value_line(
                "导入数据",
                "voicefox-export.json",
                settings_binding(&config.keybindings, Action::SettingsImportData, "Shift+F7"),
                accent,
                muted,
            ),
            setting_value_line(
                "导入外部歌单",
                "M3U/JSON",
                settings_binding(
                    &config.keybindings,
                    Action::SettingsImportPlaylist,
                    "Shift+F8",
                ),
                accent,
                muted,
            ),
            {
                let login_sources: Vec<SourceId> = SourceId::all_online()
                    .iter()
                    .copied()
                    .filter(|source| ctx.source_manager.capabilities(*source).qr_login)
                    .collect();
                let login_label = if login_sources.is_empty() {
                    "无可用音源".to_string()
                } else {
                    login_sources
                        .iter()
                        .map(|source| {
                            let status = if ctx.source_manager.is_logged_in(*source) {
                                "✓"
                            } else {
                                "○"
                            };
                            format!("{}{}", status, source.display_name())
                        })
                        .collect::<Vec<_>>()
                        .join("  ")
                };
                setting_row(
                    "扫码登录",
                    Span::styled(login_label, Style::new().fg(accent)),
                    "b",
                    muted,
                )
            },
            // --- 下载（索引 45 起，与 SETTING_OPTION_KEYS 保持一致）---
            setting_value_line(
                "下载目录",
                &shorten_source(&ctx.downloads.download_dir().display().to_string(), 22),
                "S",
                accent,
                muted,
            ),
            setting_value_line(
                "下载音质",
                &config
                    .download
                    .quality
                    .map(|quality| quality.label().to_string())
                    .unwrap_or_else(|| format!("跟随播放 ({})", config.player.quality.label())),
                "F",
                accent,
                muted,
            ),
            setting_value_line(
                "文件名模板",
                &shorten_source(&config.download.filename_template, 22),
                "M",
                accent,
                muted,
            ),
            setting_line("多线程分片", config.download.multipart, "B", accent, muted),
            setting_value_line(
                "分片阈值",
                &format!("{} MB", config.download.multipart_min_size_mb),
                "V",
                accent,
                muted,
            ),
            setting_value_line(
                "分片并发",
                &config.download.concurrency.to_string(),
                "W",
                accent,
                muted,
            ),
            setting_value_line(
                "同时下载",
                &format!("{} 首", config.download.concurrent_songs),
                "A",
                accent,
                muted,
            ),
            setting_value_line(
                "失败重试",
                &format!("{} 次", config.download.max_retries),
                "E",
                accent,
                muted,
            ),
            setting_line(
                "校验文件大小",
                config.download.verify_size,
                "U",
                accent,
                muted,
            ),
            setting_line(
                "跳过已下载",
                config.download.skip_existing,
                "L",
                accent,
                muted,
            ),
            setting_line("写入标签", config.download.write_tags, "J", accent, muted),
            setting_line("嵌入封面", config.download.embed_cover, "G", accent, muted),
            setting_line("保存歌词", config.download.save_lyric, "I", accent, muted),
        ];
        options.push(setting_value_line(
            "网易云同步",
            "双向增量",
            "S",
            accent,
            muted,
        ));
        options.push(setting_value_line(
            "QQ 音乐同步",
            "双向增量",
            "Q",
            accent,
            muted,
        ));

        let option_indices = self.category.option_indices();
        let options = options
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| option_indices.contains(&index).then_some(line))
            .collect();
        // 选项个数必须与鼠标点击的键位表长度一致，否则新增设置项后点击会错位。
        debug_assert_eq!(
            SETTING_OPTION_KEYS.len(),
            SETTING_OPTION_ACTIONS.len(),
            "设置项键位表长度必须一致"
        );
        render_setting_options(options, options_inner, buf);

        let source_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == SettingsFocus::JsSources {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" JS 音源 [s/a/d/h] ");
        let source_inner = source_block.inner(chunks[1]);
        source_block.render(chunks[1], buf);
        if source_inner.height > 0 {
            let loaded_sources = ctx.source_manager.js_source_count();
            let source_state = if loaded_sources > 0 {
                (
                    format!("{loaded_sources} 个音源已就绪，按列表顺序解析"),
                    crate::theme::green(ctx),
                )
            } else if sources.is_empty() {
                ("尚未导入 JS 音源".to_string(), crate::theme::yellow(ctx))
            } else {
                ("加载中或加载失败".to_string(), crate::theme::yellow(ctx))
            };
            // Keep command labels at a stable left-hand position so the
            // mouse hit targets match what is rendered even when the source
            // status text changes length.
            Paragraph::new(Line::from(vec![
                Span::styled(" [a] 添加  [d] 删除  [h] 检测  ", Style::new().fg(muted)),
                Span::styled(source_state.0, Style::new().fg(source_state.1)),
            ]))
            .render(
                Rect::new(source_inner.x, source_inner.y, source_inner.width, 1),
                buf,
            );
            let checking = ctx
                .source_health_checking
                .load(std::sync::atomic::Ordering::Relaxed);
            let health_line = {
                let health = ctx.source_health.read().unwrap_or_else(|e| e.into_inner());
                if checking {
                    " 音源检测中…".to_string()
                } else if health.is_empty() {
                    " 尚未检测音源".to_string()
                } else {
                    let summary = health
                        .iter()
                        .map(|item| {
                            format!(
                                "{} {}{}ms",
                                item.name,
                                if item.ok { "✓" } else { "✗" },
                                item.latency_ms
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("  ");
                    format!(" 检测结果: {summary}")
                }
            };
            Paragraph::new(Line::from(Span::styled(
                truncate_display(&health_line, source_inner.width as usize),
                Style::new().fg(if checking {
                    crate::theme::yellow(ctx)
                } else {
                    muted
                }),
            )))
            .render(
                Rect::new(source_inner.x, source_inner.y + 1, source_inner.width, 1),
                buf,
            );
        }

        // Reserve one row for the command hint and one for the status message.
        let source_rows = source_inner.height.saturating_sub(3) as usize;
        if sources.is_empty() {
            if source_inner.height > 3 {
                Paragraph::new(" (无)")
                    .style(Style::new().fg(muted))
                    .render(
                        Rect::new(source_inner.x, source_inner.y + 2, source_inner.width, 1),
                        buf,
                    );
            }
        } else {
            self.selected_source = self.selected_source.min(sources.len().saturating_sub(1));
            let source_start = list_window_start(self.selected_source, sources.len(), source_rows);
            let max_url_chars = source_inner.width.saturating_sub(28) as usize;
            for (row, (index, url)) in sources
                .iter()
                .enumerate()
                .skip(source_start)
                .take(source_rows)
                .enumerate()
            {
                let cached = is_source_cached(url);
                let status = if cached { "cached" } else { "download" };
                let name = ctx
                    .source_manager
                    .js_source_name_for_origin(url)
                    .unwrap_or_else(|| "未加载".to_string());
                let style = if index == self.selected_source {
                    Style::new()
                        .fg(crate::theme::selection_fg(ctx))
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(crate::theme::text(ctx))
                };
                let text = format!(
                    " {:<8} {} {}",
                    status,
                    truncate_display(&name, 12),
                    shorten_source(url, max_url_chars.max(8))
                );
                Paragraph::new(Line::from(Span::styled(text, style))).render(
                    Rect::new(
                        source_inner.x,
                        source_inner.y + 2 + row as u16,
                        source_inner.width,
                        1,
                    ),
                    buf,
                );
            }
        }

        // 登录状态常驻在设置页右侧，不再要求按 P 才展开。
        // 分隔条画在面板之上（坐标与命中共用 settings_dividers）。
        let divider_style = {
            let base = Style::new().fg(crate::theme::accent(ctx));
            if self.splitter.is_dragging() {
                base.add_modifier(Modifier::BOLD)
            } else {
                base
            }
        };
        for (_, hit) in divider_lines {
            match hit.axis {
                SplitAxis::Vertical => {
                    for y in hit.span.0..hit.span.1 {
                        buf.set_string(hit.divider, y, "│", divider_style);
                    }
                }
                SplitAxis::Horizontal => {
                    for x in hit.span.0..hit.span.1 {
                        buf.set_string(x, hit.divider, "─", divider_style);
                    }
                }
            }
        }

        // Accounts 分类只是把焦点切到登录区域；实际登录面板始终可见。
        let login_area = chunks[3];
        let all = SourceId::all_online();
        self.render_qr_login_panel(login_area, buf, ctx, accent, muted, &all);

        // ── 本地目录 + 状态栏共享一个紧凑管理区 ──
        let management_area = chunks[2];
        let management = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(management_area);

        let local_area = management[0];
        let status_area = management[1];

        // ── 本地音乐目录列表 ──
        let local_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == SettingsFocus::LocalPaths {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" 本地目录 [s/a/d/r] ");
        let local_inner = local_block.inner(local_area);
        local_block.render(local_area, buf);

        if local_inner.height > 1 {
            Paragraph::new(Line::from(Span::styled(
                " [a] 添加目录  [d] 移除  [r] 重新扫描",
                Style::new().fg(muted),
            )))
            .render(
                Rect::new(local_inner.x, local_inner.y, local_inner.width, 1),
                buf,
            );
        }

        // 只为显示“共 N 首”，取计数即可，避免每帧全量克隆本地曲库
        let local_song_count = ctx.source_manager.local_source().song_count();
        // Reserve one row for commands and the final row for the status/count
        // footer so list entries are never overwritten by those lines.
        let local_rows = local_inner.height.saturating_sub(3) as usize;
        if local_paths.is_empty() {
            if local_inner.height > 3 {
                Paragraph::new(Line::from(Span::styled(
                    " (无，按 a 添加音乐目录)",
                    Style::new().fg(crate::theme::yellow(ctx)),
                )))
                .render(
                    Rect::new(local_inner.x, local_inner.y + 2, local_inner.width, 1),
                    buf,
                );
            }
        } else {
            self.selected_local_path = self
                .selected_local_path
                .min(local_paths.len().saturating_sub(1));
            let local_start =
                list_window_start(self.selected_local_path, local_paths.len(), local_rows);
            for (row, (index, path)) in local_paths
                .iter()
                .enumerate()
                .skip(local_start)
                .take(local_rows)
                .enumerate()
            {
                let style = if index == self.selected_local_path {
                    Style::new()
                        .fg(crate::theme::selection_fg(ctx))
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(crate::theme::text(ctx))
                };
                Paragraph::new(Line::from(Span::styled(format!(" {}", path), style))).render(
                    Rect::new(
                        local_inner.x,
                        local_inner.y + 2 + row as u16,
                        local_inner.width,
                        1,
                    ),
                    buf,
                );
            }
        }
        // 底部显示歌曲数
        if local_inner.height > 1 {
            let footer_y = local_inner.bottom().saturating_sub(1);
            if footer_y > local_inner.y {
                Paragraph::new(Line::from(Span::styled(
                    format!(" 共 {} 首歌曲", local_song_count),
                    Style::new().fg(muted),
                )))
                .render(
                    Rect::new(local_inner.x, footer_y, local_inner.width, 1),
                    buf,
                );
            }
        }

        // ── 状态栏字段 ──
        let status_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == SettingsFocus::StatusBar {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" 状态栏 [s/Space/Shift+方向键] ");
        let status_inner = status_block.inner(status_area);
        status_block.render(status_area, buf);
        let status_rows = status_inner.height.saturating_sub(1) as usize;
        self.selected_status_item = self
            .selected_status_item
            .min(StatusBarItem::ALL.len().saturating_sub(1));
        if self.selected_status_item < self.status_item_scroll {
            self.status_item_scroll = self.selected_status_item;
        } else if status_rows > 0
            && self.selected_status_item >= self.status_item_scroll + status_rows
        {
            self.status_item_scroll = self.selected_status_item + 1 - status_rows;
        }
        self.status_item_scroll = self
            .status_item_scroll
            .min(StatusBarItem::ALL.len().saturating_sub(status_rows.max(1)));

        for (row, (index, item)) in StatusBarItem::ALL
            .iter()
            .enumerate()
            .skip(self.status_item_scroll)
            .take(status_rows)
            .enumerate()
        {
            let order = config
                .ui
                .status_bar_items
                .iter()
                .position(|candidate| candidate == item)
                .map(|position| position + 1);
            let selected = index == self.selected_status_item;
            let style = if selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::text(ctx))
            };
            let text = format!(
                " [{}] {:>2}  {}",
                if order.is_some() { "x" } else { " " },
                order.map_or_else(|| "-".to_string(), |value| value.to_string()),
                status_bar_item_label(*item)
            );
            Paragraph::new(Line::from(Span::styled(text, style))).render(
                Rect::new(
                    status_inner.x,
                    status_inner.y + row as u16,
                    status_inner.width,
                    1,
                ),
                buf,
            );
        }

        let focused_inner = match self.focus {
            SettingsFocus::JsSources => source_inner,
            SettingsFocus::LocalPaths => local_inner,
            SettingsFocus::StatusBar => status_inner,
            SettingsFocus::QrLogin => Block::default().borders(Borders::ALL).inner(login_area),
        };
        if let Some(ref msg) = self.status_msg
            && focused_inner.height > 1
        {
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", msg),
                Style::new().fg(crate::theme::yellow(ctx)),
            )))
            .render(
                Rect::new(
                    focused_inner.x,
                    focused_inner.bottom().saturating_sub(1),
                    focused_inner.width,
                    1,
                ),
                buf,
            );
        }

        // ── 输入浮层 ──
        //
        // 六个单行输入共用同一个渲染函数：既去掉重复代码，也保证光标（输入法
        // 候选框的定位依据）在每一处都落在同一个位置——文本插入点。
        if self.local_path_mode {
            render_input_overlay(
                area,
                buf,
                ctx,
                "输入本地音乐目录路径",
                &self.local_path_input,
            );
        }

        if self.input_mode {
            render_input_overlay(
                area,
                buf,
                ctx,
                "输入 JS 音源 URL 或本地路径",
                &self.input_url,
            );
        }

        if self.proxy_input_mode {
            render_input_overlay(
                area,
                buf,
                ctx,
                "输入代理地址，留空表示关闭",
                &self.proxy_input,
            );
        }

        if self.audio_device_input_mode {
            render_input_overlay(
                area,
                buf,
                ctx,
                "输入 libmpv 音频设备名，Enter 保存",
                &self.audio_device_input,
            );
        }

        if self.playlist_import_mode {
            render_input_overlay(
                area,
                buf,
                ctx,
                "输入 M3U/LX Music/网易云歌单路径，Enter 导入",
                &self.playlist_import_input,
            );
        }

        // 下载目录 / 文件名模板此前只写 status_msg、没有可见输入框：
        // 用户看不到自己打了什么，输入法的候选框也无处可依附。
        if let Some(target) = self.download_input_target {
            let title = match target {
                DownloadInputTarget::Dir => "输入下载目录，Enter 保存",
                DownloadInputTarget::Template => "输入文件名模板，Enter 保存",
            };
            render_input_overlay(area, buf, ctx, title, &self.download_input);
        }
    }

    /// 兜底取消进行中的状态栏条目拖拽。
    pub fn abort_drag_sessions(&mut self) {
        self.status_drag_target = None;
        self.splitter.cancel();
        self.layout_before_drag = None;
    }

    /// 页面在 `ui.pane_ratios` 里的 key。
    pub fn pane_page_key(&self) -> &'static str {
        SETTINGS_PAGE_KEY
    }

    /// 从 Config 恢复用户拖拽过的面板尺寸（页面构造后调用一次）。
    pub fn apply_pane_ratios(&mut self, ratios: &std::collections::HashMap<String, f32>) {
        if let Some(value) = ratios.get("options_panels").copied() {
            self.layout.options_ratio = clamp_ratio(value, 0.0, 0.5);
        }
        if let Some(value) = ratios.get("wide_left").copied() {
            self.layout.wide_left = clamp_ratio(value, 0.15, 0.55);
        }
        if let Some(value) = ratios.get("wide_middle_end").copied() {
            self.layout.wide_middle_end = clamp_ratio(value, 0.45, 0.85);
        }
        if let Some(value) = ratios.get("narrow_left").copied() {
            self.layout.narrow_left = clamp_ratio(value, 0.25, 0.80);
        }
        self.layout.clamp_all();
    }

    /// 该分隔线当前的比例（拖拽起点）。
    fn committed_ratio(&self, target: SettingsResizeTarget) -> f32 {
        match target {
            SettingsResizeTarget::OptionsPanels => self.layout.options_ratio,
            SettingsResizeTarget::WidePanelsLeft => self.layout.wide_left,
            SettingsResizeTarget::WidePanelsRight => self.layout.wide_middle_end,
            SettingsResizeTarget::NarrowPanels => self.layout.narrow_left,
        }
    }

    /// 拖拽中实时写入布局（渲染与命中读同一份，预览即时可见）。
    fn update_resize_preview(
        &mut self,
        target: SettingsResizeTarget,
        event: MouseEvent,
        area: Rect,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match target {
            SettingsResizeTarget::OptionsPanels => {
                let raw = ratio_within(area.y, area.height, event.row);
                self.layout.options_ratio = clamp_ratio(raw, 0.05, 0.5);
            }
            SettingsResizeTarget::WidePanelsLeft => {
                let raw = ratio_within(area.x, area.width, event.column);
                self.layout.wide_left = clamp_ratio(raw, 0.15, 0.55);
            }
            SettingsResizeTarget::WidePanelsRight => {
                let raw = ratio_within(area.x, area.width, event.column);
                self.layout.wide_middle_end = clamp_ratio(raw, 0.45, 0.85);
            }
            SettingsResizeTarget::NarrowPanels => {
                let raw = ratio_within(area.x, area.width, event.column);
                self.layout.narrow_left = clamp_ratio(raw, 0.25, 0.80);
            }
        }
        self.layout.clamp_all();
    }

    /// 鼠标抬起：结束拖拽并返回要持久化的 `(ratio_key, ratio)`。
    fn commit_resize(&mut self) -> Option<(&'static str, f32)> {
        let (target, _) = self.splitter.commit()?;
        self.layout_before_drag = None;
        let key = match target {
            SettingsResizeTarget::OptionsPanels => "options_panels",
            SettingsResizeTarget::WidePanelsLeft => "wide_left",
            SettingsResizeTarget::WidePanelsRight => "wide_middle_end",
            SettingsResizeTarget::NarrowPanels => "narrow_left",
        };
        Some((key, self.committed_ratio(target)))
    }

    /// Esc 取消：还原拖拽前的布局。
    fn cancel_resize(&mut self) {
        self.splitter.cancel();
        if let Some(before) = self.layout_before_drag.take() {
            self.layout = before;
        }
    }

    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        area: Rect,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> AppAction {
        // 只有"按下"才算用户主动离开输入态：否则鼠标一移动就会退出输入模式，
        // 后续按键转入全局/选项键位分发（曾经因此误触"保留播放状态"开关）。
        if self.any_input_active() && matches!(event.kind, MouseEventKind::Down(_)) {
            self.input_mode = false;
            self.local_path_mode = false;
            self.proxy_input_mode = false;
            self.audio_device_input_mode = false;
            self.playlist_import_mode = false;
            self.download_input_target = None;
        }
        let chunks = settings_chunks_with(area, self.focus, self.category, self.layout);
        // 分隔条拖拽会话优先于一切：拖拽期间其余鼠标事件不能穿透。
        let dividers = settings_dividers(area, self.focus, self.category, self.layout);
        if let Some(target) = self.splitter.dragging().copied() {
            match event.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    self.update_resize_preview(target, event, area);
                    return AppAction::None;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    return match self.commit_resize() {
                        Some((ratio_key, ratio)) => AppAction::CommitPaneRatio {
                            page_key: SETTINGS_PAGE_KEY.to_string(),
                            ratio_key: ratio_key.to_string(),
                            ratio,
                        },
                        None => AppAction::None,
                    };
                }
                _ => return AppAction::None,
            }
        }
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && let Some((target, _)) = dividers
                .iter()
                .find(|(_, hit)| hit.matches(event.column, event.row))
        {
            self.layout_before_drag = Some(self.layout);
            let committed = self.committed_ratio(*target);
            self.splitter.begin(*target, committed);
            return AppAction::None;
        }

        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollUp => {
                if chunks[3].contains(position) {
                    self.selected_status_item = self.selected_status_item.saturating_sub(1);
                    self.focus = SettingsFocus::StatusBar;
                } else if chunks[2].contains(position) {
                    self.selected_local_path = self.selected_local_path.saturating_sub(1);
                    self.focus = SettingsFocus::LocalPaths;
                } else if chunks[1].contains(position) {
                    self.selected_source = self.selected_source.saturating_sub(1);
                    self.focus = SettingsFocus::JsSources;
                }
            }
            MouseEventKind::ScrollDown => {
                if chunks[3].contains(position) {
                    self.selected_status_item = (self.selected_status_item + 1)
                        .min(StatusBarItem::ALL.len().saturating_sub(1));
                    self.focus = SettingsFocus::StatusBar;
                } else if chunks[2].contains(position) {
                    let len = ctx
                        .config
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .local_music
                        .paths
                        .len();
                    self.selected_local_path =
                        (self.selected_local_path + 1).min(len.saturating_sub(1));
                    self.focus = SettingsFocus::LocalPaths;
                } else if chunks[1].contains(position) {
                    let len = ctx
                        .config
                        .read()
                        .unwrap_or_else(|e| e.into_inner())
                        .source
                        .js_sources
                        .len();
                    self.selected_source = (self.selected_source + 1).min(len.saturating_sub(1));
                    self.focus = SettingsFocus::JsSources;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let status_inner = Block::default().borders(Borders::ALL).inner(chunks[3]);
                if let Some(index) = status_item_at(status_inner, position, self.status_item_scroll)
                {
                    self.focus = SettingsFocus::StatusBar;
                    if self.status_drag_target.is_some() && self.status_drag_target != Some(index) {
                        self.move_status_bar_item_to(ctx, index);
                        self.status_drag_target = Some(index);
                    }
                }
            }
            MouseEventKind::Down(button)
                if matches!(button, MouseButton::Left | MouseButton::Right) =>
            {
                let right_click = button == MouseButton::Right;
                self.status_drag_target = None;
                if !right_click && area.width < ALL_MANAGEMENT_PANELS_MIN_WIDTH {
                    let focused_panel = chunks[match self.focus {
                        SettingsFocus::JsSources => 1,
                        SettingsFocus::LocalPaths | SettingsFocus::StatusBar => 2,
                        SettingsFocus::QrLogin => 3,
                    }];
                    // Narrow layouts show only one management panel. Its
                    // title already advertises `[s]`; clicking that title
                    // provides the equivalent mouse-only way to cycle panels.
                    if focused_panel.contains(position) && position.y == focused_panel.y {
                        self.focus = self.focus.next();
                        return AppAction::None;
                    }
                }
                if !right_click {
                    let options_inner = Block::default().borders(Borders::ALL).inner(chunks[0]);
                    if options_inner.contains(position) {
                        let visible_index = setting_option_index(options_inner, position);
                        let option_index = self
                            .category
                            .option_indices()
                            .get(visible_index as usize)
                            .copied();
                        if let Some(Some(action)) =
                            option_index.and_then(|index| SETTING_OPTION_ACTIONS.get(index))
                            && let Some(result) = self.handle_bound_action(*action, ctx)
                        {
                            return result;
                        }
                        if let Some(&key) =
                            option_index.and_then(|index| SETTING_OPTION_KEYS.get(index))
                            && key != '\0'
                        {
                            return self.handle_input(
                                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                                ctx,
                                resolver,
                            );
                        }
                    }
                }

                let source_inner = Block::default().borders(Borders::ALL).inner(chunks[1]);
                if chunks[1].contains(position) {
                    self.focus = SettingsFocus::JsSources;
                    let command_y = source_inner.y;
                    if event.row == command_y {
                        let key = command_key_at(
                            source_inner,
                            position,
                            &[("[a] 添加", 'a'), ("[d] 删除", 'd'), ("[h] 检测", 'h')],
                        );
                        if let Some(key) = key {
                            return self.handle_input(
                                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                                ctx,
                                resolver,
                            );
                        }
                    }
                    let rows = source_inner.height.saturating_sub(3) as usize;
                    if let Some(row) = list_row_at(source_inner, position, rows) {
                        let len = ctx
                            .config
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .source
                            .js_sources
                            .len();
                        let start = list_window_start(self.selected_source, len, rows);
                        let index = start + row;
                        if index < len {
                            self.selected_source = index;
                            if right_click {
                                return self.handle_input(
                                    KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                                    ctx,
                                    resolver,
                                );
                            }
                        }
                    }
                    return AppAction::None;
                }

                let management = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
                    .split(chunks[2]);
                let local_area = management[0];
                let status_area = management[1];

                let local_inner = Block::default().borders(Borders::ALL).inner(local_area);
                if local_area.contains(position) {
                    self.focus = SettingsFocus::LocalPaths;
                    let command_y = local_inner.y;
                    if event.row == command_y {
                        let key = command_key_at(
                            local_inner,
                            position,
                            &[
                                ("[a] 添加目录", 'a'),
                                ("[d] 移除", 'd'),
                                ("[r] 重新扫描", 'r'),
                            ],
                        );
                        if let Some(key) = key {
                            return self.handle_input(
                                KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                                ctx,
                                resolver,
                            );
                        }
                    }
                    let rows = local_inner.height.saturating_sub(3) as usize;
                    if let Some(row) = list_row_at(local_inner, position, rows) {
                        let len = ctx
                            .config
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .local_music
                            .paths
                            .len();
                        let start = list_window_start(self.selected_local_path, len, rows);
                        let index = start + row;
                        if index < len {
                            self.selected_local_path = index;
                            if right_click {
                                return self.handle_input(
                                    KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
                                    ctx,
                                    resolver,
                                );
                            }
                        }
                    }
                    return AppAction::None;
                }

                let status_inner = Block::default().borders(Borders::ALL).inner(status_area);
                if status_area.contains(position) {
                    if let Some(index) =
                        status_item_at(status_inner, position, self.status_item_scroll)
                    {
                        self.selected_status_item = index;
                        self.focus = SettingsFocus::StatusBar;
                        let checkbox = event.column < status_inner.x.saturating_add(5);
                        if !right_click && !checkbox {
                            self.status_drag_target = Some(index);
                        }
                        if right_click || checkbox {
                            self.toggle_status_bar_item(ctx);
                        }
                    }
                    return AppAction::None;
                }

                if chunks[3].contains(position) {
                    self.focus = SettingsFocus::QrLogin;
                    return AppAction::None;
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.status_drag_target = None;
            }
            _ => {}
        }
        AppAction::None
    }
}

/// 渲染一个居中的单行输入浮层，并把终端光标钉在文本插入点。
///
/// 输入法的候选框跟随**终端光标**。ratatui 差分渲染下若不显式设置
/// `Frame::cursor_position`，光标只会被隐藏、停在"本帧最后一个变化的单元格"上，
/// 候选框就会在输入框和状态栏之间来回跳（issue #42）。所以每个输入浮层
/// 都要登记插入点，主循环据此设置光标位置（见 `ui_cursor`）。
fn render_input_overlay(
    area: Rect,
    buf: &mut ratatui::buffer::Buffer,
    ctx: &AppContext,
    title: &str,
    text: &str,
) {
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

    let width = area.width.saturating_sub(4).min(74);
    let input_area = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(3) / 2,
        width,
        3.min(area.height),
    );
    Clear.render(input_area, buf);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(crate::theme::green(ctx)))
        .title(title);
    let inner = block.inner(input_area);
    block.render(input_area, buf);
    Paragraph::new(Line::from(text)).render(inner, buf);
    // 光标由终端绘制，因此这里不再拼软件光标字符。
    crate::ui_cursor::request_after(inner, "", text);
}

fn list_window_start(selected: usize, len: usize, rows: usize) -> usize {
    if len == 0 || rows == 0 {
        0
    } else {
        selected.min(len - 1).saturating_sub(rows.saturating_sub(1))
    }
}

/// Return the command key under a mouse position on a panel's command row.
///
/// The labels are rendered from the panel's left edge with two spaces between
/// commands. Keeping this calculation in one place prevents the hit regions
/// from drifting when labels are changed or localized.
fn command_key_at(area: Rect, position: Position, commands: &[(&str, char)]) -> Option<char> {
    if position.y != area.y || position.x < area.x || position.x >= area.right() {
        return None;
    }

    let mut x = area.x;
    // The rendered line starts with one leading space before the first label.
    x = x.saturating_add(1);
    for (label, key) in commands {
        let width = UnicodeWidthStr::width(*label) as u16;
        if position.x >= x && position.x < x.saturating_add(width) {
            return Some(*key);
        }
        x = x.saturating_add(width).saturating_add(2);
    }
    None
}

/// Return the zero-based visible row for a source/directory list item.
/// The first two inner rows are reserved for the panel title/commands and the
/// final row is reserved for the status/count footer.
fn list_row_at(area: Rect, position: Position, rows: usize) -> Option<usize> {
    if rows == 0
        || !area.contains(position)
        || position.y < area.y.saturating_add(2)
        || position.y >= area.y.saturating_add(2 + rows as u16)
    {
        return None;
    }
    Some(position.y.saturating_sub(area.y + 2) as usize)
}

fn status_item_at(area: Rect, position: Position, scroll: usize) -> Option<usize> {
    if !area.contains(position) || position.y >= area.bottom().saturating_sub(1) {
        return None;
    }
    let index = scroll + position.y.saturating_sub(area.y) as usize;
    (index < StatusBarItem::ALL.len()).then_some(index)
}

fn enabled(value: bool) -> &'static str {
    if value { "开启" } else { "关闭" }
}

fn settings_binding<'a>(
    config: &'a KeybindingConfig,
    action: Action,
    fallback: &'a str,
) -> &'a str {
    config
        .pages
        .get("settings")
        .and_then(|bindings| bindings.get(&action))
        .map(String::as_str)
        .unwrap_or(fallback)
}

fn settings_action_is_page_owned(action: Action) -> bool {
    matches!(
        action,
        Action::ListSelectUp
            | Action::ListSelectDown
            | Action::SettingsCyclePlaybackSpeed
            | Action::SettingsEditAudioDevice
            | Action::SettingsCycleReplayGainMode
            | Action::SettingsCycleReplayGainPreamp
            | Action::SettingsCycleChannelMode
            | Action::SettingsCycleBalance
            | Action::SettingsToggleReplayGainClip
            | Action::SettingsCycleFadeInDuration
            | Action::SettingsCycleFadeOutDuration
            | Action::SettingsCycleEqualizerPreset
            | Action::SettingsRunFadeIn
            | Action::SettingsRunFadeOut
            | Action::SettingsSetAbLoopStart
            | Action::SettingsSetAbLoopEnd
            | Action::SettingsClearAbLoop
            | Action::SettingsExportData
            | Action::SettingsImportData
            | Action::SettingsImportPlaylist
    )
}

fn status_bar_item_label(item: StatusBarItem) -> &'static str {
    match item {
        StatusBarItem::State => "播放状态",
        StatusBarItem::Source => "当前音源",
        StatusBarItem::Sort => "页面排序",
        StatusBarItem::Song => "歌曲名称",
        StatusBarItem::Time => "播放时间",
        StatusBarItem::Volume => "音量",
        StatusBarItem::PlayMode => "播放模式",
        StatusBarItem::Quality => "音质",
        StatusBarItem::Queue => "队列位置",
        StatusBarItem::JsSourceState => "JS 音源状态",
    }
}

fn reorder_status_bar_items(
    items: &mut Vec<StatusBarItem>,
    item: StatusBarItem,
    target: StatusBarItem,
) -> Option<usize> {
    let item_position = items.iter().position(|candidate| *candidate == item)?;
    let target_position = items.iter().position(|candidate| *candidate == target)?;
    if item_position == target_position {
        return Some(item_position);
    }
    items.remove(item_position);
    let insertion = target_position.min(items.len());
    items.insert(insertion, item);
    Some(insertion)
}

/// 在更新配置项后更新这些常量!
///
/// 最长按键提示的显示宽度 (组合键在界面中使用 C/S/A 缩写)
const KEY_COLUMN_WIDTH: usize = 7;
/// 最长标签的显示宽度 (当前为 保留播放状态)
const LABEL_COLUMN_WIDTH: usize = 12;

/// 在右侧补空格至指定显示宽度，宽字符按两列计算
fn pad_display(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(UnicodeWidthStr::width(value));
    format!("{}{}", value, " ".repeat(padding))
}

/// 组装一行设置项：按键提示、标签与取值分别占固定宽度的列
fn setting_row(label: &str, value: Span<'static>, key: &str, muted: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(
                " {} ",
                pad_display(&format!("[{}]", compact_key_label(key)), KEY_COLUMN_WIDTH)
            ),
            Style::new().fg(muted),
        ),
        Span::raw(pad_display(label, LABEL_COLUMN_WIDTH)),
        Span::raw(" "),
        value,
    ])
}

fn compact_key_label(key: &str) -> String {
    key.replace("Ctrl+", "C+")
        .replace("Shift+", "S+")
        .replace("Alt+", "A+")
}

fn setting_line(label: &str, value: bool, key: &str, accent: Color, muted: Color) -> Line<'static> {
    setting_row(
        label,
        Span::styled(
            if value { "[x]" } else { "[ ]" },
            Style::new().fg(if value { accent } else { muted }),
        ),
        key,
        muted,
    )
}

fn setting_value_line(
    label: &str,
    value: &str,
    key: &str,
    accent: Color,
    muted: Color,
) -> Line<'static> {
    setting_row(
        label,
        Span::styled(value.to_string(), Style::new().fg(accent)),
        key,
        muted,
    )
}

fn save_status(result: anyhow::Result<()>) -> Option<String> {
    Some(match result {
        Ok(()) => "设置已保存".to_string(),
        Err(error) => format!("保存设置失败: {error}"),
    })
}

fn next_quality(quality: Quality) -> Quality {
    match quality {
        Quality::Low128 => Quality::High320,
        Quality::High320 => Quality::Flac,
        Quality::Flac => Quality::Flac24,
        Quality::Flac24 => Quality::Low128,
    }
}

/// 在一组候选值里循环取值；当前值不在候选里时回到第一个。
fn next_step(values: &[u64], current: u64) -> u64 {
    match values.iter().position(|value| *value == current) {
        Some(index) => values[(index + 1) % values.len()],
        None => values[0],
    }
}

fn next_history_limit(limit: usize) -> usize {
    match limit {
        0..=25 => 50,
        26..=50 => 100,
        51..=100 => 200,
        101..=200 => 500,
        _ => 25,
    }
}

fn next_network_timeout(timeout: u64) -> u64 {
    match timeout {
        0..=5 => 10,
        6..=10 => 15,
        11..=15 => 30,
        16..=30 => 60,
        _ => 5,
    }
}

fn next_cover_protocol(protocol: &str) -> &'static str {
    match protocol {
        "auto" => "kitty",
        "kitty" => "sixel",
        "sixel" => "iterm2",
        "iterm2" => "halfblocks",
        _ => "auto",
    }
}

fn next_fps(fps: u32) -> u32 {
    match fps {
        0..=10 => 20,
        11..=20 => 30,
        21..=30 => 60,
        _ => 10,
    }
}

fn next_scroll_amount(amount: usize) -> usize {
    match amount {
        0..=1 => 3,
        2..=3 => 5,
        4..=5 => 10,
        _ => 1,
    }
}

fn next_scan_depth(depth: u32) -> u32 {
    match depth {
        0 => 1,
        1 => 2,
        2 => 4,
        3..=4 => 8,
        5..=8 => 16,
        _ => 0,
    }
}

fn next_fade_duration(value: u64) -> u64 {
    match value {
        0 => 250,
        1..=250 => 500,
        251..=500 => 1_000,
        501..=1_000 => 2_000,
        _ => 0,
    }
}

fn fade_label(value: u64) -> String {
    if value == 0 {
        "关闭".to_string()
    } else if value.is_multiple_of(1_000) {
        format!("{} 秒", value / 1_000)
    } else {
        format!("{} ms", value)
    }
}

fn format_duration(value: std::time::Duration) -> String {
    let total = value.as_secs();
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// 在更新配置项后更新这些常量!
///
/// 鼠标点击时触发的按键，顺序必须与 render 中的选项列表一致
const SETTING_OPTION_KEYS: [char; 60] = [
    't', 'g', 'w', 'c', 'e', 'Q', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0',
    '\0', '\0', '\0', '\0', 'm', 'H', 'v', 'u', 'K', 'T', 'Y', ']', 'n', 'N', 'P', 'f', 'z', 'i',
    'o', 'x', 'X', 'R', 'p', 'D', '\0', '\0', '\0', 'b', 'S', 'F', 'M', 'B', 'V', 'W', 'A', 'E',
    'U', 'L', 'J', 'G', 'I', 'S', 'Q',
];
const SETTING_OPTION_ACTIONS: [Option<Action>; 60] = [
    None,
    None,
    None,
    None,
    None,
    None,
    Some(Action::SettingsCyclePlaybackSpeed),
    Some(Action::SettingsEditAudioDevice),
    Some(Action::SettingsCycleReplayGainMode),
    Some(Action::SettingsCycleReplayGainPreamp),
    Some(Action::SettingsCycleChannelMode),
    Some(Action::SettingsCycleBalance),
    Some(Action::SettingsToggleReplayGainClip),
    Some(Action::SettingsCycleFadeInDuration),
    Some(Action::SettingsCycleFadeOutDuration),
    Some(Action::SettingsCycleEqualizerPreset),
    Some(Action::SettingsRunFadeIn),
    Some(Action::SettingsRunFadeOut),
    Some(Action::SettingsSetAbLoopStart),
    Some(Action::SettingsSetAbLoopEnd),
    Some(Action::SettingsClearAbLoop),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some(Action::SettingsExportData),
    Some(Action::SettingsImportData),
    Some(Action::SettingsImportPlaylist),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];
const TWO_COLUMN_OPTIONS_MIN_WIDTH: u16 = 36;
const THREE_COLUMN_OPTIONS_MIN_WIDTH: u16 = 72;
const ALL_MANAGEMENT_PANELS_MIN_WIDTH: u16 = 108;
/// 页面在 `ui.pane_ratios` 里的 key。
const SETTINGS_PAGE_KEY: &str = "settings";

/// 设置页在非输入模式下响应的字符键：选项键之外还有列表操作键
/// （a 添加 / d 删除 / h 检测 / s 切换焦点 / r 扫描 / y 与 [ 见 `handle_input`）。
/// 列表导航键来自页面级绑定，由 `consumes_key` 查表解析，不列在这里。
const SETTINGS_PAGE_CHAR_KEYS: &[char] = &[
    'a', 'd', 'h', 'r', 's', 'y', '[', 'm', 'Q', 'v', 'p', 'b', 'n', 'o', 'c', 'e', 'f', 'g', 'i',
    't', 'u', 'w', 'x', 'z', 'D', 'H', 'K', 'N', 'O', 'P', 'R', 'T', 'X', 'Y', ']', 'S', 'F', 'M',
    'B', 'V', 'W', 'A', 'E', 'U', 'L', 'J', 'G', 'I', 'S',
];

fn render_setting_options<'a>(options: Vec<Line<'a>>, area: Rect, buf: &mut Buffer) {
    let column_count = setting_option_column_count(area.width);
    if column_count == 1 {
        Paragraph::new(options).render(area, buf);
        return;
    }

    let columns = setting_option_columns(area);
    let mut lines = (0..column_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for (index, line) in options.into_iter().enumerate() {
        lines[index % column_count].push(line);
    }
    for (column, lines) in columns.iter().zip(lines) {
        Paragraph::new(lines).render(*column, buf);
    }
}

fn setting_option_index(area: Rect, position: Position) -> u16 {
    let row = position.y.saturating_sub(area.y);
    let column_count = setting_option_column_count(area.width);
    if column_count == 1 {
        return row;
    }
    let columns = setting_option_columns(area);
    let column = columns
        .iter()
        .position(|column| column.contains(position))
        .unwrap_or(0) as u16;
    row.saturating_mul(column_count as u16)
        .saturating_add(column)
}

fn setting_option_column_count(width: u16) -> usize {
    if width >= THREE_COLUMN_OPTIONS_MIN_WIDTH {
        3
    } else if width >= TWO_COLUMN_OPTIONS_MIN_WIDTH {
        2
    } else {
        1
    }
}

fn setting_option_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    let count = setting_option_column_count(area.width);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints((0..count).map(|_| Constraint::Ratio(1, count as u32)))
        .split(area)
}

fn setting_options_height(panel_width: u16, option_count: usize) -> u16 {
    let inner_width = panel_width.saturating_sub(2);
    let columns = setting_option_column_count(inner_width) as u16;
    let rows = (option_count as u16).div_ceil(columns);
    rows.saturating_add(2)
}

/// 设置页可拖拽的分隔线。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsResizeTarget {
    /// 选项区与下排面板之间（水平线）。
    OptionsPanels,
    /// 宽屏下排三个面板之间的两条竖线。
    WidePanelsLeft,
    WidePanelsRight,
    /// 窄屏下排两个面板之间（竖线）。
    NarrowPanels,
}

/// 设置页的面板尺寸参数。默认值 = 以前的硬编码百分比，用户拖过之后生效。
#[derive(Debug, Clone, Copy)]
struct SettingsLayout {
    /// 选项区占整页高度（上限由选项行数决定，见 `setting_options_height`）。
    options_ratio: f32,
    /// 宽屏下排：第一个面板的宽度占比。
    wide_left: f32,
    /// 宽屏下排：前两个面板的宽度占比（第三个 = 1 - 它）。
    wide_middle_end: f32,
    /// 窄屏下排：左侧面板宽度占比。
    narrow_left: f32,
}

impl Default for SettingsLayout {
    fn default() -> Self {
        Self {
            options_ratio: 0.0, // 0 = 用选项行数推导（与旧行为一致）
            wide_left: 0.34,
            wide_middle_end: 0.66,
            narrow_left: 0.60,
        }
    }
}

impl SettingsLayout {
    fn options_height(&self, width: u16, option_count: usize, total_height: u16) -> u16 {
        let by_rows = setting_options_height(width, option_count);
        let by_ratio = if self.options_ratio > 0.0 {
            (total_height as f32 * self.options_ratio).round() as u16
        } else {
            by_rows
        };
        by_ratio
            .clamp(3, total_height.saturating_sub(3).max(3))
            .min(by_rows.max(3))
    }

    fn clamp_all(&mut self) {
        self.options_ratio = clamp_ratio(self.options_ratio, 0.0, 0.5);
        self.wide_left = clamp_ratio(self.wide_left, 0.15, 0.55);
        self.wide_middle_end = clamp_ratio(self.wide_middle_end, self.wide_left + 0.15, 0.85);
        self.narrow_left = clamp_ratio(self.narrow_left, 0.25, 0.80);
    }
}

/// 分隔条的绘制跨度与命中（渲染与命中共用）。
fn settings_dividers(
    area: Rect,
    focus: SettingsFocus,
    category: SettingsCategory,
    layout: SettingsLayout,
) -> Vec<(SettingsResizeTarget, DividerHit)> {
    let chunks = settings_chunks_with(area, focus, category, layout);
    let mut out = Vec::new();

    // 选项区与面板区之间：只有下面还有空间时才画。
    let options_bottom = chunks[0].bottom();
    if options_bottom > chunks[0].y && options_bottom <= area.bottom() {
        let divider = options_bottom.saturating_sub(1);
        if divider >= area.y && divider < area.bottom() {
            out.push((
                SettingsResizeTarget::OptionsPanels,
                DividerHit::new(SplitAxis::Horizontal, divider, (area.x, area.right())),
            ));
        }
    }

    let panels_y = (chunks[0].bottom(), area.bottom());
    if panels_y.0 >= panels_y.1 {
        return out;
    }
    if area.width >= ALL_MANAGEMENT_PANELS_MIN_WIDTH {
        let left = chunks[1];
        let middle = chunks[2];
        if left.width > 0 && middle.width > 0 {
            out.push((
                SettingsResizeTarget::WidePanelsLeft,
                DividerHit::new(
                    SplitAxis::Vertical,
                    left.right().saturating_sub(1),
                    panels_y,
                ),
            ));
        }
        if middle.width > 0 && chunks[3].width > 0 {
            out.push((
                SettingsResizeTarget::WidePanelsRight,
                DividerHit::new(
                    SplitAxis::Vertical,
                    middle.right().saturating_sub(1),
                    panels_y,
                ),
            ));
        }
    } else {
        // 窄屏下排只有左侧那一个面板 + 常驻登录区
        let left = if chunks[1].width > 0 {
            chunks[1]
        } else {
            chunks[2]
        };
        if left.width > 0 && chunks[3].width > 0 {
            out.push((
                SettingsResizeTarget::NarrowPanels,
                DividerHit::new(
                    SplitAxis::Vertical,
                    left.right().saturating_sub(1),
                    panels_y,
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
fn settings_chunks(area: Rect, focus: SettingsFocus, category: SettingsCategory) -> [Rect; 4] {
    settings_chunks_with(area, focus, category, SettingsLayout::default())
}

fn settings_chunks_with(
    area: Rect,
    focus: SettingsFocus,
    category: SettingsCategory,
    layout: SettingsLayout,
) -> [Rect; 4] {
    let option_height = layout
        .options_height(area.width, category.option_indices().len(), area.height)
        .min(area.height);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(option_height), Constraint::Min(0)])
        .split(area);

    if area.width >= ALL_MANAGEMENT_PANELS_MIN_WIDTH {
        // 宽屏：下排只保留两个真正有价值的区域：
        // 左侧 JS 音源；中间本地目录 + 状态栏；右侧常驻扫码登录。
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((layout.wide_left * 100.0).round() as u16),
                Constraint::Percentage(
                    ((layout.wide_middle_end - layout.wide_left) * 100.0).round() as u16,
                ),
                Constraint::Min(0),
            ])
            .split(vertical[1]);
        [vertical[0], bottom[0], bottom[1], bottom[2]]
    } else {
        // 窄屏也保留常驻登录区；左侧管理区根据焦点显示 JS 音源或本地+状态栏。
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((layout.narrow_left * 100.0).round() as u16),
                Constraint::Min(0),
            ])
            .split(vertical[1]);
        let mut chunks = [vertical[0], Rect::default(), Rect::default(), bottom[1]];
        if matches!(focus, SettingsFocus::JsSources) {
            chunks[1] = bottom[0];
        } else {
            chunks[2] = bottom[0];
        }
        chunks
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::{Position, Rect};
    use ratatui::style::Color;
    use ratatui::widgets::{Block, Borders};
    use unicode_width::UnicodeWidthStr;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use lx_core::keybinding::{Action, KeybindingConfig, KeybindingResolver};
    use lx_core::model::config::StatusBarItem;

    use super::{
        KEY_COLUMN_WIDTH, LABEL_COLUMN_WIDTH, SETTING_OPTION_ACTIONS, SETTING_OPTION_KEYS,
        SettingsCategory, SettingsFocus, SettingsLayout, SettingsPage, command_key_at,
        reorder_status_bar_items, setting_line, setting_option_index, setting_value_line,
        settings_chunks, settings_chunks_with, settings_dividers, shorten_source,
    };
    use crate::pages::components::splitter::SplitAxis;

    /// 各设置项取值统一起始的列号
    const VALUE_COLUMN: usize = 1 + KEY_COLUMN_WIDTH + 1 + LABEL_COLUMN_WIDTH + 1;

    /// 默认布局必须与旧硬编码完全一致（34/32/34 宽屏、60/40 窄屏），
    /// 否则这次"可拖拽化"就顺手改了别人的界面。
    #[test]
    fn default_layout_keeps_the_previous_hardcoded_panel_widths() {
        let area = Rect::new(0, 0, 160, 40);
        let chunks = settings_chunks(area, SettingsFocus::JsSources, SettingsCategory::Interface);
        // ratatui 的 Percentage 取整结果：34% → 54、32% → 52，第三个吃剩余
        assert_eq!(chunks[1].width, 54, "160 * 34%");
        assert_eq!(chunks[2].width, 52, "160 * 32%");
        assert_eq!(
            chunks[1].width + chunks[2].width + chunks[3].width,
            160,
            "三个面板必须铺满下排"
        );

        let narrow = Rect::new(0, 0, 100, 40);
        let chunks = settings_chunks(
            narrow,
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
        );
        assert_eq!(chunks[1].width, 60, "窄屏 100 * 60%");
        assert_eq!(chunks[3].width, 40);
    }

    /// 分隔条必须"看得见就抓得住"：命中坐标与绘制坐标同源。
    #[test]
    fn every_divider_is_hittable_on_its_own_line() {
        let area = Rect::new(0, 0, 160, 40);
        let layout = SettingsLayout::default();
        let dividers = settings_dividers(
            area,
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
            layout,
        );
        assert!(
            dividers.len() >= 3,
            "宽屏应有 选项/面板 横线 + 两条竖线，实际 {}",
            dividers.len()
        );
        for (_, hit) in &dividers {
            match hit.axis {
                SplitAxis::Vertical => {
                    assert!(hit.matches(hit.divider, hit.span.0), "竖线首行应命中");
                    assert!(hit.matches(hit.divider + 1, hit.span.0), "容差 ±1");
                    assert!(!hit.matches(hit.divider + 2, hit.span.0));
                }
                SplitAxis::Horizontal => {
                    assert!(hit.matches(hit.span.0, hit.divider), "横线首列应命中");
                    assert!(hit.matches(hit.span.0, hit.divider + 1), "容差 ±1");
                    assert!(!hit.matches(hit.span.0, hit.divider + 2));
                }
            }
        }
    }

    /// 拖拽比例真的会改变面板尺寸，且被夹在合理范围内。
    #[test]
    fn dragging_ratios_resizes_the_panels_within_bounds() {
        let area = Rect::new(0, 0, 160, 40);
        let mut layout = SettingsLayout {
            options_ratio: 0.0,
            wide_left: 0.50,
            wide_middle_end: 0.70,
            narrow_left: 0.60,
        };
        layout.clamp_all();
        let chunks = settings_chunks_with(
            area,
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
            layout,
        );
        assert_eq!(chunks[1].width, 80, "50% 宽度");

        // 越界值会被夹回来，不会把面板压成 0
        layout.wide_left = 0.99;
        layout.clamp_all();
        assert!(layout.wide_left <= 0.55);
        assert!(layout.wide_middle_end > layout.wide_left);
    }

    #[test]
    fn every_setting_option_belongs_to_exactly_one_category() {
        assert_eq!(SETTING_OPTION_KEYS.len(), SETTING_OPTION_ACTIONS.len());
        let mut seen = std::collections::BTreeSet::new();
        for category in [
            SettingsCategory::Interface,
            SettingsCategory::Playback,
            SettingsCategory::Sources,
            SettingsCategory::Accounts,
            SettingsCategory::Integration,
            SettingsCategory::Download,
            SettingsCategory::Data,
        ] {
            for index in category.option_indices() {
                assert!(
                    *index < SETTING_OPTION_KEYS.len(),
                    "设置项下标 {index} 超出键位表长度"
                );
                assert!(seen.insert(*index), "设置项下标 {index} 归属了多个分类");
            }
        }
        assert_eq!(
            seen.len(),
            SETTING_OPTION_KEYS.len(),
            "每个设置项都应当出现在某个分类里"
        );
    }

    #[test]
    fn download_category_exposes_every_download_option() {
        let indices = SettingsCategory::Download.option_indices();

        assert_eq!(indices.len(), 13);
        let keys: Vec<char> = indices
            .iter()
            .map(|index| SETTING_OPTION_KEYS[*index])
            .collect();
        assert_eq!(
            keys,
            vec![
                'S', 'F', 'M', 'B', 'V', 'W', 'A', 'E', 'U', 'L', 'J', 'G', 'I'
            ]
        );
    }

    #[test]
    fn shortens_unicode_source_path_on_character_boundaries() {
        let path = "/home/user/音乐音源/这是一个很长的第三方音源脚本文件名/latest.js";
        let shortened = shorten_source(path, 24);

        assert_eq!(shortened.chars().count(), 24);
        assert!(shortened.ends_with("..."));
    }

    #[test]
    fn setting_rows_align_values_on_a_shared_column() {
        let accent = Color::Reset;
        let muted = Color::Reset;
        let rows = [
            setting_line("MPRIS", true, "i", accent, muted),
            setting_line("保留播放状态", false, "e", accent, muted),
            setting_value_line("最大 FPS", "30", "f", accent, muted),
            setting_value_line("歌词偏移", "+0 ms", "[/]", accent, muted),
            setting_value_line("音源开关", "kw 开启", "k/K", accent, muted),
        ];

        for row in rows {
            let prefix: String = row.spans[..row.spans.len() - 1]
                .iter()
                .map(|span| span.content.as_ref())
                .collect();

            assert_eq!(UnicodeWidthStr::width(prefix.as_str()), VALUE_COLUMN);
        }
    }

    #[test]
    fn settings_page_owns_every_option_key() {
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        let page = SettingsPage::new();

        for (index, key) in SETTING_OPTION_KEYS.into_iter().enumerate() {
            // 新播放/数据动作由页面级 Action 处理，不能再把它们的旧数字
            // 占位键视为设置页快捷键，否则会遮挡侧边栏的 1-8 切换。
            if SETTING_OPTION_ACTIONS[index].is_some() {
                continue;
            }
            let event = KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE);

            assert!(
                page.consumes_key(&event, &resolver),
                "选项键 {key} 不应被全局快捷键抢先处理"
            );
        }
    }

    #[test]
    fn tab_number_keys_remain_available_on_the_settings_page() {
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        let page = SettingsPage::new();

        for key in '0'..='9' {
            assert!(!page.consumes_key(
                &KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                &resolver
            ));
        }
    }

    #[test]
    fn tab_number_keys_remain_reserved_with_custom_settings_bindings() {
        let mut config = KeybindingConfig::default();
        config
            .pages
            .get_mut("settings")
            .unwrap()
            .insert(Action::SettingsCyclePlaybackSpeed, "1".to_string());
        let resolver = KeybindingResolver::from_config(&config);
        let page = SettingsPage::new();

        for key in '1'..='8' {
            assert!(!page.consumes_key(
                &KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                &resolver
            ));
        }
    }

    #[test]
    fn settings_page_owns_the_configured_list_navigation_keys() {
        let mut config = KeybindingConfig::default();
        config
            .pages
            .get_mut("settings")
            .unwrap()
            .insert(Action::ListSelectUp, "h".to_string());
        let resolver = KeybindingResolver::from_config(&config);
        let page = SettingsPage::new();

        let rebound = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let released = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);

        assert!(page.consumes_key(&rebound, &resolver));
        // 'k' 不再是导航键，也不是选项键，应交还给全局
        assert!(!page.consumes_key(&released, &resolver));
    }

    #[test]
    fn settings_page_owns_rebound_playback_action_even_with_ctrl() {
        let mut config = KeybindingConfig::default();
        config
            .pages
            .get_mut("settings")
            .unwrap()
            .insert(Action::SettingsCyclePlaybackSpeed, "Ctrl+1".to_string());
        let resolver = KeybindingResolver::from_config(&config);
        let page = SettingsPage::new();

        let rebound = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::CONTROL);
        let released = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);

        assert!(page.consumes_key(&rebound, &resolver));
        assert!(!page.consumes_key(&released, &resolver));
    }

    #[test]
    fn settings_page_leaves_playback_and_navigation_keys_global() {
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        let page = SettingsPage::new();
        let global = [
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE),
            KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE),
        ];

        for event in global {
            assert!(
                !page.consumes_key(&event, &resolver),
                "{:?} 不应被设置页独占",
                event.code
            );
        }
    }

    #[test]
    fn status_bar_focus_owns_toggle_and_reorder_keys() {
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
        let mut page = SettingsPage::new();
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let shift_left = KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT);

        assert!(!page.consumes_key(&space, &resolver));
        page.focus = SettingsFocus::StatusBar;
        assert!(page.consumes_key(&space, &resolver));
        assert!(page.consumes_key(&shift_left, &resolver));
    }

    #[test]
    fn narrow_settings_keep_login_panel_visible() {
        let chunks = settings_chunks(
            Rect::new(0, 0, 80, 24),
            SettingsFocus::StatusBar,
            SettingsCategory::Interface,
        );

        assert_eq!(chunks[0].height, 5);
        assert_eq!(chunks[1], Rect::default());
        assert!(chunks[2].width > 0);
        assert!(chunks[3].width > 0);
        assert_eq!(chunks[2].height, chunks[3].height);
        assert_eq!(chunks[2].bottom(), 24);
        assert_eq!(chunks[3].bottom(), 24);
    }

    #[test]
    fn wide_settings_keep_all_management_panels_visible() {
        let chunks = settings_chunks(
            Rect::new(0, 0, 120, 30),
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
        );

        assert_eq!(chunks[0].height, 5);
        assert_eq!(chunks[1].y, chunks[0].bottom());
        assert_eq!(chunks[2].y, chunks[0].bottom());
        assert_eq!(chunks[3].y, chunks[0].bottom());
        assert_eq!(chunks[1].width + chunks[2].width + chunks[3].width, 120);
    }

    #[test]
    fn setting_mouse_rows_follow_the_two_column_layout() {
        let panel = settings_chunks(
            Rect::new(0, 0, 60, 24),
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
        )[0];
        let inner = Block::default().borders(Borders::ALL).inner(panel);
        let right_column_x = inner.x + inner.width / 2 + 1;

        assert_eq!(
            setting_option_index(inner, Position::new(inner.x, inner.y)),
            0
        );
        assert_eq!(
            setting_option_index(inner, Position::new(right_column_x, inner.y)),
            1
        );
        assert_eq!(
            setting_option_index(inner, Position::new(right_column_x, inner.y + 1)),
            3
        );
    }

    #[test]
    fn setting_mouse_rows_follow_the_three_column_layout() {
        let panel = settings_chunks(
            Rect::new(0, 0, 80, 24),
            SettingsFocus::JsSources,
            SettingsCategory::Interface,
        )[0];
        let inner = Block::default().borders(Borders::ALL).inner(panel);
        let third_column_x = inner.x + inner.width * 5 / 6;

        assert_eq!(
            setting_option_index(inner, Position::new(third_column_x, inner.y)),
            2
        );
        assert_eq!(
            setting_option_index(inner, Position::new(third_column_x, inner.y + 1)),
            5
        );
    }

    #[test]
    fn bottom_panel_command_hit_targets_match_rendered_labels() {
        let area = Rect::new(10, 20, 50, 8);
        let source_commands = [("[a] 添加", 'a'), ("[d] 删除", 'd')];
        // One leading space is part of the rendered command row.
        assert_eq!(
            command_key_at(area, Position::new(area.x + 2, area.y), &source_commands),
            Some('a')
        );
        assert_eq!(
            command_key_at(area, Position::new(area.x + 12, area.y), &source_commands),
            Some('d')
        );
        assert_eq!(
            command_key_at(area, Position::new(area.x, area.y), &source_commands),
            None
        );
        assert_eq!(
            command_key_at(
                area,
                Position::new(area.x + 1, area.y + 1),
                &source_commands
            ),
            None
        );
    }

    #[test]
    fn status_bar_drag_reorders_once_at_the_target_position() {
        let mut items = vec![
            StatusBarItem::State,
            StatusBarItem::Source,
            StatusBarItem::Sort,
            StatusBarItem::Song,
        ];

        assert_eq!(
            reorder_status_bar_items(&mut items, StatusBarItem::State, StatusBarItem::Sort),
            Some(2)
        );
        assert_eq!(
            items,
            vec![
                StatusBarItem::Source,
                StatusBarItem::Sort,
                StatusBarItem::State,
                StatusBarItem::Song,
            ]
        );
        assert_eq!(
            reorder_status_bar_items(&mut items, StatusBarItem::Song, StatusBarItem::Source),
            Some(0)
        );
        assert_eq!(items[0], StatusBarItem::Song);
    }
}
