use crossterm::event::{KeyEvent, MouseEvent};
use std::time::Duration;

use crate::model::song::SongInfo;
use crate::model::source::SourceId;

/// 终端输入事件
#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    Tick,
}

/// 页面标识
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageId {
    Main,
    Search,
    PlayQueue,
    Settings,
}

/// 应用操作（页面返回的结果）
#[derive(Debug, Clone)]
pub enum AppAction {
    Navigate(PageId),
    GoBack,
    Quit,
    PlaySong {
        songs: Vec<SongInfo>,
        index: usize,
    },
    /// Resolve a search-result Bilibili video before playback; multi-part videos open a picker.
    ResolveBiliParts {
        songs: Vec<SongInfo>,
        index: usize,
        request_id: u64,
    },
    RestorePlayback {
        songs: Vec<SongInfo>,
        index: usize,
        position: Duration,
        start_playback: bool,
        paused: bool,
    },
    AddToQueue {
        song: Box<SongInfo>,
        position: InsertPosition,
    },
    RetrySong {
        song: Box<SongInfo>,
    },
    Search {
        keyword: String,
        source: Option<SourceId>,
    },
    SearchMore {
        keyword: String,
        page: u32,
        source: Option<SourceId>,
    },
    ShowNotification(Notification),
    ImportSource(String),
    SourceImported {
        url: String,
        generation: u64,
    },
    SourceImportFailed {
        error: String,
        generation: u64,
    },
    RemoveSource(String),
    RemoveHistory(Box<SongInfo>),
    ClearHistory,
    ScanLocalMusic {
        paths: Vec<String>,
        max_depth: u32,
    },
    BiliLogin,
    BiliLogout,
    BiliLoginSuccess,
    None,
}

/// 通知消息
#[derive(Debug, Clone)]
pub struct Notification {
    pub level: NotificationLevel,
    pub title: Option<String>,
    pub message: String,
    pub icon: Option<String>,
    pub in_app: bool,
    pub desktop: bool,
    pub replace_previous: bool,
    pub action_label: Option<String>,
    pub action_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Local>,
}

impl Notification {
    fn new(level: NotificationLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            title: None,
            message: message.into(),
            icon: None,
            in_app: true,
            desktop: true,
            replace_previous: false,
            action_label: None,
            action_url: None,
            created_at: chrono::Local::now(),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self::new(NotificationLevel::Error, msg)
    }

    pub fn info(msg: impl Into<String>) -> Self {
        Self::new(NotificationLevel::Info, msg)
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self::new(NotificationLevel::Success, msg)
    }

    pub fn warning(msg: impl Into<String>) -> Self {
        Self::new(NotificationLevel::Warn, msg)
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn replacing_previous(mut self) -> Self {
        self.replace_previous = true;
        self
    }

    pub fn tui_only(mut self) -> Self {
        self.desktop = false;
        self
    }

    pub fn desktop_only(mut self) -> Self {
        self.in_app = false;
        self
    }

    pub fn with_action(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.action_label = Some(label.into());
        self.action_url = Some(url.into());
        self
    }

    pub fn timestamp(&self) -> String {
        self.created_at.format("%H:%M:%S").to_string()
    }

    pub fn age(&self) -> Duration {
        chrono::Local::now()
            .signed_duration_since(self.created_at)
            .to_std()
            .unwrap_or_default()
    }

    pub fn is_expired(&self, lifetime: Duration) -> bool {
        self.age() >= lifetime
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warn,
    Error,
}

/// 插入位置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    Next,
    End,
}
