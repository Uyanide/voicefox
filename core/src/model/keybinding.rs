//! 自定义键位映射系统（Phase 1）
//!
//! 设计目标：
//! - 单键映射，零学习成本
//! - 全局 + 页面级两层作用域
//! - 完全兼容现有硬编码行为（默认配置 = 当前键位）
//! - 可扩展至多键序列（Phase 2）

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// Action 枚举：所有可绑定的动作
// =============================================================================

/// 可绑定的用户动作。
///
/// 命名约定：`<domain>_<verb>`，如 `global_quit`、`search_select_down`。
/// 这样即使不同页面有同名动作（如上下导航），也可以通过域名区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // --- 全局动作 ---
    /// 退出应用
    GlobalQuit,
    /// 播放/暂停
    GlobalPlayPause,
    /// 下一首
    GlobalNextTrack,
    /// 上一首
    GlobalPrevTrack,
    /// 切换播放模式（列表循环 / 单曲循环 / 随机）
    GlobalCycleMode,
    /// 快进 5 秒
    GlobalSeekForward,
    /// 后退 5 秒
    GlobalSeekBackward,
    /// 音量增加
    GlobalVolumeUp,
    /// 音量减少
    GlobalVolumeDown,
    /// 下一个标签页
    GlobalNextTab,
    /// 上一个标签页
    GlobalPrevTab,
    /// 返回主页面（Esc）
    GlobalGoToMain,
    /// 收藏/取消收藏当前歌曲（Ctrl+L）
    GlobalToggleFavorite,
    /// 下载当前播放中的歌曲（Ctrl+S）
    GlobalDownloadCurrent,
    /// 打开下载面板（Ctrl+O）
    GlobalDownloadsPanel,
    /// 强制重绘，并把封面重新传输给终端（Ctrl+R）
    GlobalRedraw,

    // --- 通用列表动作（多个页面共用） ---
    /// 选择上一项
    ListSelectUp,
    /// 选择下一项
    ListSelectDown,
    /// 跳到第一项
    ListSelectFirst,
    /// 跳到最后一项
    ListSelectLast,
    /// 向上翻页
    ListPageUp,
    /// 向下翻页
    ListPageDown,
    /// 激活选中项（播放 / 进入）
    ListActivate,
    /// 收藏或取消收藏当前页面选中的歌曲/歌单
    ListToggleFavorite,
    /// 返回/退出（Esc）
    ListGoBack,
    /// 添加到队列尾部
    ListAddToQueue,
    /// 添加到队列下一首
    ListAddToQueueNext,
    /// 下载当前选中的歌曲
    ListDownload,
    /// 循环切换当前列表的排序方式
    ListCycleSort,

    // --- 搜索页面专用 ---
    /// 进入搜索输入模式
    SearchInputMode,
    /// 开始搜索 / 播放结果（Enter 的复合语义）
    SearchStart,
    /// 切换聚合/单音源搜索
    SearchToggleAggregate,
    /// 切换上一个音源
    SearchCycleSourcePrev,
    /// 切换下一个音源
    SearchCycleSourceNext,

    // --- 本地音乐页面专用 ---
    /// 重新扫描本地音乐目录
    LocalRescan,
    /// 删除选中的本地文件（弹出确认）
    LocalDelete,
    /// 进入本地音乐过滤模式
    LocalFilter,

    // --- 历史页面专用 ---
    /// 进入历史页面过滤模式
    HistoryFilter,

    // --- 收藏页面专用 ---
    /// 进入过滤模式
    FavoritesFilter,
    /// 取消收藏
    FavoritesRemove,

    // --- 设置页面专用 ---
    /// 切换设置项的值
    SettingsToggle,
    /// 循环切换播放速度
    SettingsCyclePlaybackSpeed,
    /// 编辑音频输出设备
    SettingsEditAudioDevice,
    /// 循环切换 ReplayGain 模式
    SettingsCycleReplayGainMode,
    /// 循环切换 ReplayGain 预放大
    SettingsCycleReplayGainPreamp,
    /// 循环切换声道模式
    SettingsCycleChannelMode,
    /// 循环调整左右平衡
    SettingsCycleBalance,
    /// 切换 ReplayGain 削波保护
    SettingsToggleReplayGainClip,
    /// 循环切换自动淡入时长
    SettingsCycleFadeInDuration,
    /// 循环切换自动淡出时长
    SettingsCycleFadeOutDuration,
    /// 循环切换均衡器预设
    SettingsCycleEqualizerPreset,
    /// 对当前歌曲执行淡入
    SettingsRunFadeIn,
    /// 对当前歌曲执行淡出
    SettingsRunFadeOut,
    /// 把当前位置设为 A-B 循环起点
    SettingsSetAbLoopStart,
    /// 把当前位置设为 A-B 循环终点
    SettingsSetAbLoopEnd,
    /// 清除 A-B 循环
    SettingsClearAbLoop,
    /// 导出用户数据
    SettingsExportData,
    /// 导入用户数据
    SettingsImportData,
    /// 导入外部歌单
    SettingsImportPlaylist,
}

// =============================================================================
// KeybindingConfig：序列化配置结构
// =============================================================================

impl Action {
    /// 动作的中文说明，用于帮助浮层与文档生成。
    pub fn label(self) -> &'static str {
        match self {
            Action::GlobalQuit => "退出应用",
            Action::GlobalPlayPause => "播放 / 暂停",
            Action::GlobalNextTrack => "下一首",
            Action::GlobalPrevTrack => "上一首",
            Action::GlobalCycleMode => "切换播放模式",
            Action::GlobalSeekForward => "快进 5 秒",
            Action::GlobalSeekBackward => "后退 5 秒",
            Action::GlobalVolumeUp => "音量增加",
            Action::GlobalVolumeDown => "音量减少",
            Action::GlobalNextTab => "下一个标签页",
            Action::GlobalPrevTab => "上一个标签页",
            Action::GlobalGoToMain => "返回主页面",
            Action::GlobalToggleFavorite => "收藏 / 取消收藏当前歌曲",
            Action::GlobalDownloadCurrent => "下载当前播放歌曲",
            Action::GlobalDownloadsPanel => "打开 / 关闭下载面板",
            Action::GlobalRedraw => "强制重绘界面",
            Action::ListSelectUp => "选择上一项",
            Action::ListSelectDown => "选择下一项",
            Action::ListSelectFirst => "跳到第一项",
            Action::ListSelectLast => "跳到最后一项",
            Action::ListPageUp => "向上翻页",
            Action::ListPageDown => "向下翻页",
            Action::ListActivate => "激活选中项（播放 / 进入）",
            Action::ListToggleFavorite => "收藏 / 取消收藏选中歌曲",
            Action::ListGoBack => "返回 / 退出",
            Action::ListAddToQueue => "添加到队列尾部",
            Action::ListAddToQueueNext => "添加到队列下一首",
            Action::ListDownload => "下载选中歌曲",
            Action::ListCycleSort => "循环切换列表排序",
            Action::SearchInputMode => "进入搜索输入模式",
            Action::SearchStart => "开始搜索 / 播放结果",
            Action::SearchToggleAggregate => "切换聚合 / 单音源搜索",
            Action::SearchCycleSourcePrev => "切换上一个音源",
            Action::SearchCycleSourceNext => "切换下一个音源",
            Action::LocalRescan => "重新扫描本地音乐目录",
            Action::LocalDelete => "删除选中的本地文件",
            Action::LocalFilter => "进入本地音乐过滤模式",
            Action::HistoryFilter => "进入历史页面过滤模式",
            Action::FavoritesFilter => "进入收藏过滤模式",
            Action::FavoritesRemove => "取消收藏",
            Action::SettingsToggle => "切换设置项的值",
            Action::SettingsCyclePlaybackSpeed => "循环切换播放速度",
            Action::SettingsEditAudioDevice => "编辑音频输出设备",
            Action::SettingsCycleReplayGainMode => "循环切换 ReplayGain 模式",
            Action::SettingsCycleReplayGainPreamp => "循环切换 ReplayGain 预放大",
            Action::SettingsCycleChannelMode => "循环切换声道模式",
            Action::SettingsCycleBalance => "循环调整左右平衡",
            Action::SettingsToggleReplayGainClip => "切换 ReplayGain 削波保护",
            Action::SettingsCycleFadeInDuration => "循环切换自动淡入时长",
            Action::SettingsCycleFadeOutDuration => "循环切换自动淡出时长",
            Action::SettingsCycleEqualizerPreset => "循环切换均衡器预设",
            Action::SettingsRunFadeIn => "对当前歌曲执行淡入",
            Action::SettingsRunFadeOut => "对当前歌曲执行淡出",
            Action::SettingsSetAbLoopStart => "设置 A-B 循环起点",
            Action::SettingsSetAbLoopEnd => "设置 A-B 循环终点",
            Action::SettingsClearAbLoop => "清除 A-B 循环",
            Action::SettingsExportData => "导出用户数据",
            Action::SettingsImportData => "导入用户数据",
            Action::SettingsImportPlaylist => "导入外部歌单",
        }
    }

    /// 动作的规范展示顺序，帮助浮层按此排序。
    pub const ALL: [Action; 59] = [
        Action::GlobalQuit,
        Action::GlobalPlayPause,
        Action::GlobalNextTrack,
        Action::GlobalPrevTrack,
        Action::GlobalCycleMode,
        Action::GlobalSeekForward,
        Action::GlobalSeekBackward,
        Action::GlobalVolumeUp,
        Action::GlobalVolumeDown,
        Action::GlobalNextTab,
        Action::GlobalPrevTab,
        Action::GlobalGoToMain,
        Action::GlobalToggleFavorite,
        Action::GlobalDownloadCurrent,
        Action::GlobalDownloadsPanel,
        Action::GlobalRedraw,
        Action::ListSelectUp,
        Action::ListSelectDown,
        Action::ListSelectFirst,
        Action::ListSelectLast,
        Action::ListPageUp,
        Action::ListPageDown,
        Action::ListActivate,
        Action::ListToggleFavorite,
        Action::ListGoBack,
        Action::ListAddToQueue,
        Action::ListAddToQueueNext,
        Action::ListDownload,
        Action::ListCycleSort,
        Action::SearchInputMode,
        Action::SearchStart,
        Action::SearchToggleAggregate,
        Action::SearchCycleSourcePrev,
        Action::SearchCycleSourceNext,
        Action::LocalRescan,
        Action::LocalDelete,
        Action::LocalFilter,
        Action::HistoryFilter,
        Action::FavoritesFilter,
        Action::FavoritesRemove,
        Action::SettingsToggle,
        Action::SettingsCyclePlaybackSpeed,
        Action::SettingsEditAudioDevice,
        Action::SettingsCycleReplayGainMode,
        Action::SettingsCycleReplayGainPreamp,
        Action::SettingsCycleChannelMode,
        Action::SettingsCycleBalance,
        Action::SettingsToggleReplayGainClip,
        Action::SettingsCycleFadeInDuration,
        Action::SettingsCycleFadeOutDuration,
        Action::SettingsCycleEqualizerPreset,
        Action::SettingsRunFadeIn,
        Action::SettingsRunFadeOut,
        Action::SettingsSetAbLoopStart,
        Action::SettingsSetAbLoopEnd,
        Action::SettingsClearAbLoop,
        Action::SettingsExportData,
        Action::SettingsImportData,
        Action::SettingsImportPlaylist,
    ];
}

/// 页面标识在帮助浮层中的显示名与顺序。
pub const PAGE_ORDER: [(&str, &str); 10] = [
    ("main", "队列（主页）"),
    ("search", "搜索"),
    ("leaderboard", "排行榜"),
    ("playlists", "热门歌单"),
    ("favorites", "收藏"),
    ("history", "播放历史"),
    ("local", "本地音乐"),
    ("settings", "设置"),
    ("details", "歌手/专辑详情"),
    ("bili_login", "B 站登录"),
];

/// 键位配置根结构。
///
/// TOML 示例：
/// ```toml
/// [keybindings.global]
/// quit = "q"
/// play_pause = "Space"
/// next_track = "n"
/// prev_track = "b"
///
/// [keybindings.pages.search]
/// select_up = "k"
/// select_down = "j"
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct KeybindingConfig {
    /// 全局键位映射：动作 -> 键位字符串
    #[serde(default = "default_global_bindings")]
    pub global: HashMap<Action, String>,
    /// 页面级键位映射：页面名 -> (动作 -> 键位字符串)
    #[serde(default = "default_page_bindings")]
    pub pages: HashMap<String, HashMap<Action, String>>,
}

impl Default for KeybindingConfig {
    fn default() -> Self {
        Self {
            global: default_global_bindings(),
            pages: default_page_bindings(),
        }
    }
}

/// 将旧版设置页快捷键迁移到不占用标签页数字键的默认绑定。
///
/// 裸数字键在设置页始终保留给侧边栏；带 Ctrl/Alt/Shift 的用户绑定不受影响。
pub fn migrate_legacy_settings_bindings(config: &mut KeybindingConfig) -> bool {
    let Some(settings) = config.pages.get_mut("settings") else {
        return false;
    };
    let defaults = default_page_bindings()
        .remove("settings")
        .expect("default settings bindings must exist");
    let legacy_defaults = [
        (Action::SettingsCyclePlaybackSpeed, "1"),
        (Action::SettingsEditAudioDevice, "2"),
        (Action::SettingsCycleReplayGainMode, "3"),
        (Action::SettingsCycleChannelMode, "4"),
        (Action::SettingsCycleReplayGainPreamp, "5"),
        (Action::SettingsCycleBalance, "6"),
        (Action::SettingsToggleReplayGainClip, "7"),
        (Action::SettingsCycleFadeInDuration, "8"),
        (Action::SettingsCycleFadeOutDuration, "9"),
        (Action::SettingsCycleEqualizerPreset, "0"),
        (Action::SettingsRunFadeIn, "F"),
        (Action::SettingsRunFadeOut, "G"),
        (Action::SettingsSetAbLoopStart, "L"),
        (Action::SettingsSetAbLoopEnd, "U"),
        (Action::SettingsClearAbLoop, "C"),
        (Action::SettingsExportData, "E"),
        (Action::SettingsImportData, "I"),
        (Action::SettingsImportPlaylist, "J"),
    ];

    let mut changed = false;
    for (action, binding) in settings.iter_mut() {
        let is_bare_digit = binding.len() == 1 && binding.as_bytes()[0].is_ascii_digit();
        let is_legacy_default = legacy_defaults
            .iter()
            .any(|(legacy_action, legacy_key)| action == legacy_action && binding == legacy_key);
        if (is_bare_digit || is_legacy_default)
            && let Some(default) = defaults.get(action)
            && binding != default
        {
            binding.clone_from(default);
            changed = true;
        }
    }
    changed
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PartialKeybindingConfig {
    global: HashMap<Action, String>,
    pages: HashMap<String, HashMap<Action, String>>,
}

impl<'de> Deserialize<'de> for KeybindingConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let partial = PartialKeybindingConfig::deserialize(deserializer)?;
        let mut config = Self::default();
        config.global.extend(partial.global);
        for (page, bindings) in partial.pages {
            config.pages.entry(page).or_default().extend(bindings);
        }
        Ok(config)
    }
}

/// 默认全局键位（与现有硬编码完全一致）
fn default_global_bindings() -> HashMap<Action, String> {
    let mut m = HashMap::new();
    m.insert(Action::GlobalQuit, "q".to_string());
    m.insert(Action::GlobalPlayPause, "Space".to_string());
    m.insert(Action::GlobalNextTrack, "n".to_string());
    m.insert(Action::GlobalPrevTrack, "b".to_string());
    m.insert(Action::GlobalCycleMode, "m".to_string());
    // The application gates these global actions to the queue page; search
    // reuses the bracket keys for source switching.
    m.insert(Action::GlobalSeekForward, "]".to_string());
    m.insert(Action::GlobalSeekBackward, "[".to_string());
    m.insert(Action::GlobalVolumeUp, ".".to_string());
    m.insert(Action::GlobalVolumeDown, ",".to_string());
    m.insert(Action::GlobalNextTab, "Tab".to_string());
    m.insert(Action::GlobalPrevTab, "Shift+Tab".to_string());
    m.insert(Action::GlobalGoToMain, "Esc".to_string());
    m.insert(Action::GlobalToggleFavorite, "Ctrl+l".to_string());
    // Ctrl+S / Ctrl+O 不与页面内的小写 s / o 冲突，也不占用列表翻页键。
    m.insert(Action::GlobalDownloadCurrent, "Ctrl+s".to_string());
    m.insert(Action::GlobalDownloadsPanel, "Ctrl+o".to_string());
    m.insert(Action::GlobalRedraw, "Ctrl+r".to_string());
    m
}

/// 默认页面级键位（与现有硬编码完全一致）
fn default_page_bindings() -> HashMap<String, HashMap<Action, String>> {
    let mut pages = HashMap::new();

    // --- 搜索页面 ---
    let mut search = HashMap::new();
    search.insert(Action::SearchInputMode, "i".to_string());
    search.insert(Action::SearchStart, "Enter".to_string());
    search.insert(Action::SearchToggleAggregate, "v".to_string());
    search.insert(Action::ListSelectUp, "k".to_string());
    search.insert(Action::ListSelectDown, "j".to_string());
    search.insert(Action::ListSelectFirst, "g".to_string());
    search.insert(Action::ListSelectLast, "G".to_string());
    search.insert(Action::ListPageUp, "Ctrl+u".to_string());
    search.insert(Action::ListPageDown, "Ctrl+d".to_string());
    search.insert(Action::ListActivate, "l".to_string());
    search.insert(Action::ListToggleFavorite, "f".to_string());
    search.insert(Action::ListAddToQueue, "a".to_string());
    search.insert(Action::ListAddToQueueNext, "A".to_string());
    search.insert(Action::ListDownload, "D".to_string());
    search.insert(Action::SearchCycleSourcePrev, "Left".to_string());
    search.insert(Action::SearchCycleSourceNext, "Right".to_string());
    search.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("search".to_string(), search);

    // --- 主页（队列） ---
    let mut main = HashMap::new();
    main.insert(Action::ListSelectUp, "k".to_string());
    main.insert(Action::ListSelectDown, "j".to_string());
    main.insert(Action::ListSelectFirst, "g".to_string());
    main.insert(Action::ListSelectLast, "G".to_string());
    main.insert(Action::ListPageUp, "Ctrl+u".to_string());
    main.insert(Action::ListPageDown, "Ctrl+d".to_string());
    main.insert(Action::ListActivate, "Enter".to_string());
    main.insert(Action::ListToggleFavorite, "f".to_string());
    // 队列页的 D 已经用于「清空队列」，下载沿用 Ctrl+S 的语义，落在选中的队列项上。
    main.insert(Action::ListDownload, "Ctrl+s".to_string());
    pages.insert("main".to_string(), main);

    // --- 排行榜 ---
    let mut leaderboard = HashMap::new();
    leaderboard.insert(Action::ListSelectUp, "k".to_string());
    leaderboard.insert(Action::ListSelectDown, "j".to_string());
    leaderboard.insert(Action::ListSelectFirst, "g".to_string());
    leaderboard.insert(Action::ListSelectLast, "G".to_string());
    leaderboard.insert(Action::ListPageUp, "Ctrl+u".to_string());
    leaderboard.insert(Action::ListPageDown, "Ctrl+d".to_string());
    leaderboard.insert(Action::ListActivate, "Enter".to_string());
    leaderboard.insert(Action::ListToggleFavorite, "f".to_string());
    leaderboard.insert(Action::ListAddToQueue, "a".to_string());
    leaderboard.insert(Action::ListAddToQueueNext, "A".to_string());
    leaderboard.insert(Action::ListDownload, "D".to_string());
    leaderboard.insert(Action::SearchCycleSourcePrev, "Left".to_string());
    leaderboard.insert(Action::SearchCycleSourceNext, "Right".to_string());
    leaderboard.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("leaderboard".to_string(), leaderboard);

    // --- 歌单 ---
    let mut playlists = HashMap::new();
    playlists.insert(Action::ListSelectUp, "k".to_string());
    playlists.insert(Action::ListSelectDown, "j".to_string());
    playlists.insert(Action::ListSelectFirst, "g".to_string());
    playlists.insert(Action::ListSelectLast, "G".to_string());
    playlists.insert(Action::ListPageUp, "Ctrl+u".to_string());
    playlists.insert(Action::ListPageDown, "Ctrl+d".to_string());
    playlists.insert(Action::ListActivate, "Enter".to_string());
    playlists.insert(Action::ListToggleFavorite, "f".to_string());
    playlists.insert(Action::ListAddToQueue, "a".to_string());
    playlists.insert(Action::ListAddToQueueNext, "A".to_string());
    playlists.insert(Action::ListDownload, "D".to_string());
    playlists.insert(Action::SearchCycleSourcePrev, "Left".to_string());
    playlists.insert(Action::SearchCycleSourceNext, "Right".to_string());
    playlists.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("playlists".to_string(), playlists);

    // --- 收藏 ---
    let mut favorites = HashMap::new();
    favorites.insert(Action::FavoritesFilter, "/".to_string());
    favorites.insert(Action::ListSelectUp, "k".to_string());
    favorites.insert(Action::ListSelectDown, "j".to_string());
    favorites.insert(Action::ListSelectFirst, "g".to_string());
    favorites.insert(Action::ListSelectLast, "G".to_string());
    favorites.insert(Action::ListPageUp, "Ctrl+u".to_string());
    favorites.insert(Action::ListPageDown, "Ctrl+d".to_string());
    favorites.insert(Action::ListActivate, "Enter".to_string());
    favorites.insert(Action::ListToggleFavorite, "f".to_string());
    favorites.insert(Action::ListAddToQueue, "a".to_string());
    favorites.insert(Action::ListAddToQueueNext, "A".to_string());
    favorites.insert(Action::ListCycleSort, "s".to_string());
    favorites.insert(Action::FavoritesRemove, "d".to_string());
    favorites.insert(Action::ListDownload, "D".to_string());
    favorites.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("favorites".to_string(), favorites);

    // --- 历史 ---
    let mut history = HashMap::new();
    history.insert(Action::ListSelectUp, "k".to_string());
    history.insert(Action::ListSelectDown, "j".to_string());
    history.insert(Action::ListSelectFirst, "g".to_string());
    history.insert(Action::ListSelectLast, "G".to_string());
    history.insert(Action::ListPageUp, "Ctrl+u".to_string());
    history.insert(Action::ListPageDown, "Ctrl+d".to_string());
    history.insert(Action::ListActivate, "Enter".to_string());
    history.insert(Action::ListToggleFavorite, "f".to_string());
    history.insert(Action::ListAddToQueue, "a".to_string());
    history.insert(Action::ListAddToQueueNext, "A".to_string());
    history.insert(Action::ListDownload, "D".to_string());
    history.insert(Action::ListCycleSort, "s".to_string());
    history.insert(Action::HistoryFilter, "/".to_string());
    pages.insert("history".to_string(), history);

    // --- 本地音乐 ---
    let mut local = HashMap::new();
    local.insert(Action::ListSelectUp, "k".to_string());
    local.insert(Action::ListSelectDown, "j".to_string());
    local.insert(Action::ListSelectFirst, "g".to_string());
    local.insert(Action::ListSelectLast, "G".to_string());
    local.insert(Action::ListPageUp, "Ctrl+u".to_string());
    local.insert(Action::ListPageDown, "Ctrl+d".to_string());
    local.insert(Action::ListActivate, "Enter".to_string());
    local.insert(Action::ListToggleFavorite, "f".to_string());
    local.insert(Action::ListAddToQueue, "a".to_string());
    local.insert(Action::ListAddToQueueNext, "A".to_string());
    local.insert(Action::ListCycleSort, "s".to_string());
    local.insert(Action::LocalRescan, "r".to_string());
    local.insert(Action::LocalDelete, "d".to_string());
    local.insert(Action::LocalFilter, "/".to_string());
    pages.insert("local".to_string(), local);

    // --- 设置 ---
    let mut settings = HashMap::new();
    settings.insert(Action::ListSelectUp, "k".to_string());
    settings.insert(Action::ListSelectDown, "j".to_string());
    settings.insert(Action::ListGoBack, "Esc".to_string());
    // 数字键留给侧边栏的 1-8 标签页快捷键；设置动作统一使用功能键。
    settings.insert(Action::SettingsCyclePlaybackSpeed, "F1".to_string());
    settings.insert(Action::SettingsEditAudioDevice, "F2".to_string());
    settings.insert(Action::SettingsCycleReplayGainMode, "F3".to_string());
    settings.insert(Action::SettingsCycleChannelMode, "F4".to_string());
    settings.insert(Action::SettingsCycleReplayGainPreamp, "F5".to_string());
    settings.insert(Action::SettingsCycleBalance, "F6".to_string());
    settings.insert(Action::SettingsToggleReplayGainClip, "F7".to_string());
    settings.insert(Action::SettingsCycleFadeInDuration, "F8".to_string());
    settings.insert(Action::SettingsCycleFadeOutDuration, "F9".to_string());
    settings.insert(Action::SettingsCycleEqualizerPreset, "F10".to_string());
    settings.insert(Action::SettingsRunFadeIn, "Shift+F1".to_string());
    settings.insert(Action::SettingsRunFadeOut, "Shift+F2".to_string());
    settings.insert(Action::SettingsSetAbLoopStart, "Shift+F3".to_string());
    settings.insert(Action::SettingsSetAbLoopEnd, "Shift+F4".to_string());
    settings.insert(Action::SettingsClearAbLoop, "Shift+F5".to_string());
    settings.insert(Action::SettingsExportData, "Shift+F6".to_string());
    settings.insert(Action::SettingsImportData, "Shift+F7".to_string());
    settings.insert(Action::SettingsImportPlaylist, "Shift+F8".to_string());
    pages.insert("settings".to_string(), settings);

    // --- B站登录 ---
    let mut bili_login = HashMap::new();
    bili_login.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("bili_login".to_string(), bili_login);

    // --- 歌手/专辑详情 ---
    let mut details = HashMap::new();
    details.insert(Action::ListSelectUp, "k".to_string());
    details.insert(Action::ListSelectDown, "j".to_string());
    details.insert(Action::ListSelectFirst, "g".to_string());
    details.insert(Action::ListSelectLast, "G".to_string());
    details.insert(Action::ListPageUp, "Ctrl+u".to_string());
    details.insert(Action::ListPageDown, "Ctrl+d".to_string());
    details.insert(Action::ListActivate, "Enter".to_string());
    details.insert(Action::ListToggleFavorite, "f".to_string());
    details.insert(Action::ListAddToQueue, "a".to_string());
    details.insert(Action::ListAddToQueueNext, "A".to_string());
    details.insert(Action::ListDownload, "D".to_string());
    details.insert(Action::ListGoBack, "Esc".to_string());
    pages.insert("details".to_string(), details);

    pages
}
