//! 设置页面：支持 JS 音源 URL 或本地路径导入/删除

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use lx_core::events::AppAction;
use lx_core::keybinding::{Action, KeybindingResolver};
use lx_core::model::source::{Quality, SourceId};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::context::AppContext;

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
    /// 内置音源开关当前指向的音源
    pub enabled_source_index: usize,
    /// 当前聚焦区域: "js" 或 "local"
    pub focus: String,
}

impl SettingsPage {
    /// 检查是否有任何输入模式激活（JS 源输入或本地路径输入）
    pub fn any_input_active(&self) -> bool {
        self.input_mode || self.local_path_mode || self.proxy_input_mode
    }

    /// 判断按键是否由设置页独占。设置页把整个字母表当作选项开关，
    /// 与用户可自定义的全局快捷键必然重叠，因此这些键不再交给全局分发。
    /// 带 Ctrl/Alt 的组合键以及 Space、Tab、Esc 仍归全局。
    pub fn consumes_key(key: &KeyEvent, resolver: &KeybindingResolver) -> bool {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        // 列表导航键可被用户重绑，按当前配置解析而不是写死 j/k
        if matches!(
            resolver.resolve_page("settings", key),
            Some(Action::ListSelectUp | Action::ListSelectDown)
        ) {
            return true;
        }
        match key.code {
            KeyCode::Up | KeyCode::Down => true,
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
            enabled_source_index: 0,
            focus: "js".to_string(),
        }
    }

    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> AppAction {
        if self.proxy_input_mode {
            return self.handle_proxy_input(key, ctx);
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
            // 先判断焦点区域：local 区域的按键优先处理
            if self.focus == "local"
                && let Some(action) = self.handle_local_keys(key, ctx, resolver)
            {
                return action;
            }

            let sources = ctx.config.read().unwrap().source.js_sources.clone();

            if let Some(action) = resolver.resolve_page("settings", &key) {
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
                        let url = sources[self.selected_source].clone();
                        self.status_msg = Some("已移除音源".to_string());
                        if self.selected_source >= sources.len().saturating_sub(1) {
                            self.selected_source = self.selected_source.saturating_sub(1);
                        }
                        return AppAction::RemoveSource(url);
                    }
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
                    if !ctx.config.read().unwrap().ui.show_cover {
                        ctx.cover_service.clear();
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('e')) => {
                    let enabled = {
                        let mut config = ctx.config.write().unwrap();
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
                        let mut config = ctx.config.write().unwrap();
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
                        let mut config = ctx.config.write().unwrap();
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
                        let mut config = ctx.config.write().unwrap();
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
                    self.proxy_input = ctx.config.read().unwrap().network.proxy_url.clone();
                    self.proxy_input_mode = true;
                    self.status_msg = None;
                }
                (KeyModifiers::SHIFT, KeyCode::Char('N' | 'n'))
                | (KeyModifiers::NONE, KeyCode::Char('N')) => {
                    let (proxy, timeout) = {
                        let mut config = ctx.config.write().unwrap();
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
                (KeyModifiers::NONE, KeyCode::Char('m')) => {
                    let mode = ctx.playlist.cycle_mode();
                    let result = {
                        let mut config = ctx.config.write().unwrap();
                        config.player.play_mode = mode.as_config().to_string();
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    self.status_msg = Some(match result {
                        Ok(()) => format!("播放模式: {}", mode.label()),
                        Err(error) => format!("播放模式已切换，但保存失败: {}", error),
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('p')) => {
                    self.update_config(ctx, |config| {
                        config.theme.accent = match config.theme.accent.as_str() {
                            "#cba6f7" => "#89b4fa",
                            "#89b4fa" => "#94e2d5",
                            "#94e2d5" => "#f5c2e7",
                            "#f5c2e7" => "#fab387",
                            _ => "#cba6f7",
                        }
                        .to_string();
                    });
                }
                (KeyModifiers::SHIFT, KeyCode::Char('D' | 'd'))
                | (KeyModifiers::NONE, KeyCode::Char('D')) => {
                    self.update_config(ctx, |config| {
                        config.local_music.max_depth =
                            next_scan_depth(config.local_music.max_depth);
                    });
                }
                (KeyModifiers::NONE, KeyCode::Char('s')) => {
                    self.focus = if self.focus == "js" { "local" } else { "js" }.to_string();
                }
                (KeyModifiers::NONE, KeyCode::Char('b')) => {
                    if ctx.bili_source.is_logged_in() {
                        return AppAction::BiliLogout;
                    } else {
                        return AppAction::BiliLogin;
                    }
                }
                _ => {}
            }
        }
        AppAction::None
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
                    let mut config = ctx.config.write().unwrap();
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

    fn cycle_default_source(&mut self, ctx: &AppContext) {
        let (default, enabled) = {
            let config = ctx.config.read().unwrap();
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
            let mut config = ctx.config.write().unwrap();
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
        let source = SourceId::all_online()[self.enabled_source_index];
        let (default, enabled, save_result) = {
            let mut config = ctx.config.write().unwrap();
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
            let mut config = ctx.config.write().unwrap();
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
                        let mut config = ctx.config.write().unwrap();
                        if !config.local_music.paths.contains(&path) {
                            config.local_music.paths.push(path.clone());
                            config.local_music.enabled = true;
                        }
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    let (paths, max_depth) = {
                        let config = ctx.config.read().unwrap();
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
                    return AppAction::ScanLocalMusic { paths, max_depth };
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
        let paths = ctx.config.read().unwrap().local_music.paths.clone();

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
            (KeyModifiers::NONE, KeyCode::Char('s')) => {
                self.focus = "js".to_string();
                Some(AppAction::None)
            }
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
                    let removed = paths[self.selected_local_path].clone();
                    let save_result = {
                        let mut config = ctx.config.write().unwrap();
                        config.local_music.paths.retain(|p| p != &removed);
                        config.local_music.enabled = !config.local_music.paths.is_empty();
                        crate::config::loader::save(&config, &ctx.config_path)
                    };
                    if self.selected_local_path >= paths.len().saturating_sub(1) {
                        self.selected_local_path = self.selected_local_path.saturating_sub(1);
                    }
                    let (remaining, max_depth) = {
                        let config = ctx.config.read().unwrap();
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
                    });
                }
                Some(AppAction::None)
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                let max_depth = ctx.config.read().unwrap().local_music.max_depth;
                self.status_msg = Some("正在扫描本地音乐...".to_string());
                Some(AppAction::ScanLocalMusic { paths, max_depth })
            }
            _ => None,
        }
    }

    fn update_config(
        &mut self,
        ctx: &AppContext,
        update: impl FnOnce(&mut lx_core::model::config::Config),
    ) {
        let result = {
            let mut config = ctx.config.write().unwrap();
            update(&mut config);
            crate::config::loader::save(&config, &ctx.config_path)
        };
        self.status_msg = Some(match result {
            Ok(()) => "设置已保存".to_string(),
            Err(error) => format!("保存设置失败: {}", error),
        });
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let config = ctx.config.read().unwrap();
        let sources = &config.source.js_sources;
        let local_paths = &config.local_music.paths;
        let accent = crate::theme::accent(ctx);
        let muted = crate::theme::muted(ctx);
        let chunks = settings_chunks(area);
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

        let options_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::border(ctx)))
            .title(" 设置选项 ");
        let options_inner = options_block.inner(chunks[0]);
        options_block.render(chunks[0], buf);
        let options = vec![
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
                quality_label(config.player.quality),
                "Q",
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
            {
                let logged_in = ctx.bili_source.is_logged_in();
                let account = if logged_in {
                    ctx.bili_source
                        .user()
                        .map(|user| user.name)
                        .unwrap_or_else(|| "已登录".to_string())
                } else {
                    "未登录".to_string()
                };
                setting_row(
                    "哔哩哔哩",
                    Span::styled(
                        account,
                        Style::new().fg(if logged_in { accent } else { muted }),
                    ),
                    "b",
                    muted,
                )
            },
        ];
        render_setting_options(options, options_inner, buf);

        let source_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == "js" {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" lx-music JS 音源 · a 添加 / d 删除 ");
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
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", source_state.0),
                Style::new().fg(source_state.1),
            )))
            .render(
                Rect::new(source_inner.x, source_inner.y, source_inner.width, 1),
                buf,
            );
        }

        let source_rows = source_inner.height.saturating_sub(2) as usize;
        if sources.is_empty() {
            if source_inner.height > 2 {
                Paragraph::new(" (无)")
                    .style(Style::new().fg(muted))
                    .render(
                        Rect::new(source_inner.x, source_inner.y + 2, source_inner.width, 1),
                        buf,
                    );
            }
        } else {
            self.selected_source = self.selected_source.min(sources.len().saturating_sub(1));
            let max_chars = source_inner.width.saturating_sub(14) as usize;
            for (row, (index, url)) in sources.iter().enumerate().take(source_rows).enumerate() {
                let cached = is_source_cached(url);
                let status = if cached { "cached" } else { "download" };
                let style = if index == self.selected_source {
                    Style::new()
                        .fg(crate::theme::selection_fg(ctx))
                        .bg(accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(crate::theme::text(ctx))
                };
                let text = format!(" {:<8} {}", status, shorten_source(url, max_chars.max(8)));
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

        if let Some(ref msg) = self.status_msg
            && source_inner.height > 1
        {
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", msg),
                Style::new().fg(crate::theme::yellow(ctx)),
            )))
            .render(
                Rect::new(
                    source_inner.x,
                    source_inner.bottom().saturating_sub(1),
                    source_inner.width,
                    1,
                ),
                buf,
            );
        }

        // ── 本地音乐目录列表 ──
        let local_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(if self.focus == "local" {
                accent
            } else {
                crate::theme::border(ctx)
            }))
            .title(" 本地音乐目录 · s 切换 / a 添加 / d 删除 / r 扫描 ");
        let local_inner = local_block.inner(chunks[2]);
        local_block.render(chunks[2], buf);

        let local_songs = ctx.source_manager.local_source().all_songs();
        let local_rows = local_inner.height.saturating_sub(2) as usize;
        if local_paths.is_empty() {
            if local_inner.height > 2 {
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
            for (row, (index, path)) in local_paths.iter().enumerate().take(local_rows).enumerate()
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
                    format!(" 共 {} 首歌曲", local_songs.len()),
                    Style::new().fg(muted),
                )))
                .render(
                    Rect::new(local_inner.x, footer_y, local_inner.width, 1),
                    buf,
                );
            }
        }

        // ── 本地音乐路径输入弹窗 ──
        if self.local_path_mode {
            let width = area.width.saturating_sub(4).min(74);
            let input_area = Rect::new(
                area.x + area.width.saturating_sub(width) / 2,
                area.y + area.height.saturating_sub(3) / 2,
                width,
                3.min(area.height),
            );
            Clear.render(input_area, buf);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(crate::theme::green(ctx)))
                .title("输入本地音乐目录路径");
            let inner = input_block.inner(input_area);
            input_block.render(input_area, buf);
            let cursor = if (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                / 500)
                .is_multiple_of(2)
            {
                "█"
            } else {
                " "
            };
            Paragraph::new(Line::from(format!("{}{}", self.local_path_input, cursor)))
                .render(inner, buf);
        }

        if self.input_mode {
            let width = area.width.saturating_sub(4).min(74);
            let input_area = Rect::new(
                area.x + area.width.saturating_sub(width) / 2,
                area.y + area.height.saturating_sub(3) / 2,
                width,
                3.min(area.height),
            );
            Clear.render(input_area, buf);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(crate::theme::green(ctx)))
                .title("输入 JS 音源 URL 或本地路径");

            let inner = input_block.inner(input_area);
            input_block.render(input_area, buf);
            let cursor = if (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                / 500)
                .is_multiple_of(2)
            {
                "█"
            } else {
                " "
            };

            Paragraph::new(Line::from(format!("{}{}", self.input_url, cursor))).render(inner, buf);
        }

        if self.proxy_input_mode {
            let width = area.width.saturating_sub(4).min(74);
            let input_area = Rect::new(
                area.x + area.width.saturating_sub(width) / 2,
                area.y + area.height.saturating_sub(3) / 2,
                width,
                3.min(area.height),
            );
            Clear.render(input_area, buf);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(crate::theme::green(ctx)))
                .title("输入代理地址，留空表示关闭");
            let inner = input_block.inner(input_area);
            input_block.render(input_area, buf);
            Paragraph::new(Line::from(self.proxy_input.as_str())).render(inner, buf);
        }
    }

    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        area: Rect,
        ctx: &AppContext,
        resolver: &KeybindingResolver,
    ) -> AppAction {
        if self.input_mode {
            return AppAction::None;
        }
        let chunks = settings_chunks(area);
        match event.kind {
            MouseEventKind::ScrollUp => {
                self.selected_source = self.selected_source.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                let len = ctx.config.read().unwrap().source.js_sources.len();
                self.selected_source = (self.selected_source + 1).min(len.saturating_sub(1));
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let options_inner = Block::default().borders(Borders::ALL).inner(chunks[0]);
                if options_inner.contains((event.column, event.row).into()) {
                    let index =
                        setting_option_index(options_inner, Position::new(event.column, event.row));
                    if let Some(&key) = SETTING_OPTION_KEYS.get(index as usize) {
                        return self.handle_input(
                            KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE),
                            ctx,
                            resolver,
                        );
                    }
                }

                let source_inner = Block::default().borders(Borders::ALL).inner(chunks[1]);
                if source_inner.contains((event.column, event.row).into())
                    && event.row >= source_inner.y.saturating_add(2)
                {
                    let index = event.row.saturating_sub(source_inner.y + 2) as usize;
                    let len = ctx.config.read().unwrap().source.js_sources.len();
                    if index < len {
                        self.selected_source = index;
                        self.focus = "js".to_string();
                    }
                }

                // 本地音乐目录列表点击
                let local_inner = Block::default().borders(Borders::ALL).inner(chunks[2]);
                if local_inner.contains((event.column, event.row).into())
                    && event.row >= local_inner.y.saturating_add(2)
                {
                    let index = event.row.saturating_sub(local_inner.y + 2) as usize;
                    let len = ctx.config.read().unwrap().local_music.paths.len();
                    if index < len {
                        self.selected_local_path = index;
                        self.focus = "local".to_string();
                    }
                }
            }
            _ => {}
        }
        AppAction::None
    }
}

fn enabled(value: bool) -> &'static str {
    if value { "开启" } else { "关闭" }
}

/// 在更新配置项后更新这些常量!
///
/// 最长按键提示的显示宽度 (当前为 [./.] [[/]])
const KEY_COLUMN_WIDTH: usize = 5;
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
            format!(" {} ", pad_display(&format!("[{key}]"), KEY_COLUMN_WIDTH)),
            Style::new().fg(muted),
        ),
        Span::raw(pad_display(label, LABEL_COLUMN_WIDTH)),
        Span::raw(" "),
        value,
    ])
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

fn quality_label(quality: Quality) -> &'static str {
    match quality {
        Quality::Low128 => "128k",
        Quality::High320 => "320k",
        Quality::Flac => "FLAC",
        Quality::Flac24 => "Hi-Res",
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

/// 在更新配置项后更新这些常量!
///
/// 鼠标点击时触发的按键，顺序必须与 render 中的选项列表一致
const SETTING_OPTION_KEYS: [char; 27] = [
    't', 'g', 'w', 'c', 'e', 'Q', 'm', 'H', 'v', 'u', 'K', 'T', 'Y', ']', 'n', 'N', 'P', 'f', 'z',
    'i', 'o', 'x', 'X', 'R', 'p', 'D', 'b',
];
const SETTINGS_OPTION_COUNT: u16 = SETTING_OPTION_KEYS.len() as u16;
const TWO_COLUMN_OPTIONS_MIN_WIDTH: u16 = 36;

/// 设置页在非输入模式下响应的字符键：选项键之外还有列表操作键
/// （a 添加 / d 删除 / s 切换焦点 / r 扫描 / y 与 [ 见 `handle_input`）。
/// 列表导航键来自页面级绑定，由 `consumes_key` 查表解析，不列在这里。
const SETTINGS_PAGE_CHAR_KEYS: &[char] = &[
    'a', 'd', 'r', 's', 'y', '[', 'm', 'Q', 'v', 'p', 'b', 'n', 'o', 'c', 'e', 'f', 'g', 'i', 't',
    'u', 'w', 'x', 'z', 'D', 'H', 'K', 'N', 'O', 'P', 'R', 'T', 'X', 'Y', ']',
];

fn render_setting_options<'a>(options: Vec<Line<'a>>, area: Rect, buf: &mut Buffer) {
    if !setting_options_use_two_columns(area) {
        Paragraph::new(options).render(area, buf);
        return;
    }

    let columns = setting_option_columns(area);
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (index, line) in options.into_iter().enumerate() {
        if index.is_multiple_of(2) {
            left.push(line);
        } else {
            right.push(line);
        }
    }
    Paragraph::new(left).render(columns[0], buf);
    Paragraph::new(right).render(columns[1], buf);
}

fn setting_option_index(area: Rect, position: Position) -> u16 {
    let row = position.y.saturating_sub(area.y);
    if !setting_options_use_two_columns(area) {
        return row;
    }
    let columns = setting_option_columns(area);
    let column = u16::from(columns[1].contains(position));
    row.saturating_mul(2).saturating_add(column)
}

fn setting_options_use_two_columns(area: Rect) -> bool {
    area.width >= TWO_COLUMN_OPTIONS_MIN_WIDTH
}

fn setting_option_columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area)
}

fn setting_options_height(panel_width: u16) -> u16 {
    let inner_width = panel_width.saturating_sub(2);
    let rows = if inner_width >= TWO_COLUMN_OPTIONS_MIN_WIDTH {
        SETTINGS_OPTION_COUNT.div_ceil(2)
    } else {
        SETTINGS_OPTION_COUNT
    };
    rows.saturating_add(2)
}

fn settings_chunks(area: Rect) -> std::rc::Rc<[Rect]> {
    if area.width >= 72 {
        // 宽屏：选项占满上排，JS 音源与本地目录共享下排。
        let option_height = setting_options_height(area.width).min(area.height);
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(option_height), Constraint::Min(0)])
            .split(area);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(vertical[1]);
        std::rc::Rc::new([vertical[0], bottom[0], bottom[1]])
    } else {
        // 三块垂直布局
        let option_height = setting_options_height(area.width).min(area.height);
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(option_height),
                Constraint::Ratio(1, 2),
                Constraint::Ratio(1, 2),
            ])
            .split(area)
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

    use super::{
        KEY_COLUMN_WIDTH, LABEL_COLUMN_WIDTH, SETTING_OPTION_KEYS, SettingsPage, setting_line,
        setting_option_index, setting_value_line, settings_chunks, shorten_source,
    };

    /// 各设置项取值统一起始的列号
    const VALUE_COLUMN: usize = 1 + KEY_COLUMN_WIDTH + 1 + LABEL_COLUMN_WIDTH + 1;

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

        for key in SETTING_OPTION_KEYS {
            let event = KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE);

            assert!(
                SettingsPage::consumes_key(&event, &resolver),
                "选项键 {key} 不应被全局快捷键抢先处理"
            );
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

        let rebound = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let released = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);

        assert!(SettingsPage::consumes_key(&rebound, &resolver));
        // 'k' 不再是导航键，也不是选项键，应交还给全局
        assert!(!SettingsPage::consumes_key(&released, &resolver));
    }

    #[test]
    fn settings_page_leaves_playback_and_navigation_keys_global() {
        let resolver = KeybindingResolver::from_config(&KeybindingConfig::default());
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
                !SettingsPage::consumes_key(&event, &resolver),
                "{:?} 不应被设置页独占",
                event.code
            );
        }
    }

    #[test]
    fn narrow_settings_use_two_columns_when_the_width_allows_it() {
        let chunks = settings_chunks(Rect::new(0, 0, 60, 24));

        assert_eq!(chunks[0].height, 16);
        assert!(chunks[1].height > 0);
        assert!(chunks[2].height > 0);
        assert_eq!(chunks[2].bottom(), 24);
    }

    #[test]
    fn wide_settings_keep_both_source_lists_visible() {
        let chunks = settings_chunks(Rect::new(0, 0, 120, 30));

        assert_eq!(chunks[0].height, 16);
        assert_eq!(chunks[1].y, chunks[0].bottom());
        assert_eq!(chunks[2].y, chunks[0].bottom());
        assert_eq!(chunks[1].width + chunks[2].width, 120);
    }

    #[test]
    fn setting_mouse_rows_follow_the_two_column_layout() {
        let panel = settings_chunks(Rect::new(0, 0, 60, 24))[0];
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
            setting_option_index(inner, Position::new(right_column_x, inner.y + 5)),
            11
        );
    }
}
