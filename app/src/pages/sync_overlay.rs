use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

use crate::storage::Storage;
use crate::sync::{self, NeteaseSyncPreview, NeteaseSyncReport, SyncControl};
use lx_core::model::source::SourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    Preparing,
    Preview,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct SyncUiState {
    pub phase: SyncPhase,
    pub preview: Option<NeteaseSyncPreview>,
    pub report: Option<NeteaseSyncReport>,
    pub error: Option<String>,
}
impl Default for SyncUiState {
    fn default() -> Self {
        Self {
            phase: SyncPhase::Preparing,
            preview: None,
            report: None,
            error: None,
        }
    }
}

pub struct SyncOverlay {
    pub state: Arc<Mutex<SyncUiState>>,
    pub control: SyncControl,
    task: Option<JoinHandle<()>>,
    pub selected: usize,
    pub source: SourceId,
}
impl SyncOverlay {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SyncUiState::default())),
            control: SyncControl::default(),
            task: None,
            selected: 0,
            source: SourceId::Wy,
        }
    }
    pub fn start_for(
        &mut self,
        source: SourceId,
        storage: Arc<Storage>,
        rt: &tokio::runtime::Runtime,
    ) {
        self.source = source;
        self.control = SyncControl::default();
        self.selected = 0;
        let state = Arc::clone(&self.state);
        let control = self.control.clone();
        *state.lock().unwrap_or_else(|e| e.into_inner()) = SyncUiState::default();
        self.task = Some(rt.spawn(async move {
            match sync::preview_source(&storage, source).await {
                Ok(preview) => {
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    s.preview = Some(preview);
                    s.phase = SyncPhase::Preview;
                }
                Err(error) => {
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    s.error = Some(error);
                    s.phase = SyncPhase::Failed;
                }
            }
            let _ = control;
        }));
    }
    pub fn confirm(&mut self, storage: Arc<Storage>, rt: &tokio::runtime::Runtime) {
        self.confirm_for(self.source, storage, rt);
    }
    pub fn confirm_for(
        &mut self,
        source: SourceId,
        storage: Arc<Storage>,
        rt: &tokio::runtime::Runtime,
    ) {
        let phase = self.state.lock().unwrap_or_else(|e| e.into_inner()).phase;
        if !matches!(
            phase,
            SyncPhase::Preview | SyncPhase::Failed | SyncPhase::Cancelled
        ) {
            return;
        }
        self.control
            .cancelled
            .store(false, std::sync::atomic::Ordering::Release);
        self.control
            .done
            .store(0, std::sync::atomic::Ordering::Release);
        let state = Arc::clone(&self.state);
        let control = self.control.clone();
        {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            s.phase = SyncPhase::Running;
            s.error = None;
        }
        self.task = Some(rt.spawn(async move {
            match sync::sync_source_with_control(&storage, source, &control).await {
                Ok(report) => {
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    s.report = Some(report);
                    s.phase = SyncPhase::Done;
                }
                Err(error) => {
                    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
                    s.error = Some(error.clone());
                    s.phase = if error == "同步已取消" {
                        SyncPhase::Cancelled
                    } else {
                        SyncPhase::Failed
                    };
                }
            }
        }));
    }
    pub fn retry(&mut self, storage: Arc<Storage>, rt: &tokio::runtime::Runtime) {
        self.start_for(self.source, storage, rt);
    }
    pub fn cancel(&mut self) {
        self.control.cancel();
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(s.phase, SyncPhase::Preparing | SyncPhase::Running) {
            s.phase = SyncPhase::Cancelled;
        }
    }
    pub fn render(&self, area: Rect, buf: &mut Buffer, ctx: &crate::context::AppContext) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .title(format!("同步预览 Diff · {}", self.source.display_name()));
        let inner = block.inner(area);
        block.render(area, buf);
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut lines = Vec::new();
        match s.phase {
            SyncPhase::Preparing => {
                lines.push(Line::from("正在读取网易云与本地歌单并计算 Diff..."))
            }
            SyncPhase::Preview => {
                if let Some(p) = s.preview {
                    lines.push(Line::from(Span::styled(
                        "本次不会修改任何数据。确认后才执行。",
                        Style::new().add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(
                        "↑ 上传到{}  {} 首",
                        self.source.display_name(),
                        p.upload
                    )));
                    lines.push(Line::from(format!("↓ 下载到本地      {} 首", p.download)));
                    lines.push(Line::from(format!("= 已匹配          {} 首", p.matched)));
                    lines.push(Line::from(format!("? 未匹配          {} 首", p.unmatched)));
                    lines.push(Line::from(format!(
                        "收藏：↑ {} / ↓ {}",
                        p.favorites_upload, p.favorites_download
                    )));
                    lines.push(Line::from(format!(
                        "歌单：名称映射 {} 个，本地独有 {}，网易云独有 {}",
                        p.playlists.len().saturating_sub(p.local_only),
                        p.local_only,
                        p.remote_only
                    )));
                    lines.push(Line::from(""));
                    for d in p
                        .playlists
                        .iter()
                        .take(inner.height.saturating_sub(10) as usize)
                    {
                        lines.push(Line::from(format!(
                            "  {}  ↑{} ↓{} ={} ?{} [{}]",
                            d.name, d.upload, d.download, d.matched, d.unmatched, d.mapping
                        )));
                    }
                    if !p.unmatched_songs.is_empty() {
                        lines.push(Line::from(""));
                        lines.push(Line::from("未匹配歌曲："));
                        for (name, singer) in p.unmatched_songs.iter().take(5) {
                            lines.push(Line::from(format!("  ? {} — {}", name, singer)));
                        }
                        if p.unmatched_songs.len() > 5 {
                            lines.push(Line::from(format!(
                                "  ... 还有 {} 首",
                                p.unmatched_songs.len() - 5
                            )));
                        }
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from("[Enter/S] 确认执行    [Esc] 取消"));
                }
            }
            SyncPhase::Running => {
                let (done, total) = self.control.progress();
                lines.push(Line::from("正在同步，不会自动删除歌曲。"));
                lines.push(Line::from(format!("进度  {done}/{total}")));
                let width = inner.width.saturating_sub(4) as usize;
                let filled = if total == 0 {
                    0
                } else {
                    width.saturating_mul(done.min(total)) / total
                };
                lines.push(Line::from(format!(
                    "[{}{}]",
                    "#".repeat(filled),
                    "-".repeat(width.saturating_sub(filled))
                )));
                lines.push(Line::from("[Esc] 取消"));
            }
            SyncPhase::Done => {
                if let Some(r) = s.report {
                    lines.push(Line::from("网易云同步完成"));
                    lines.push(Line::from(format!(
                        "歌单创建 {} · 拉取 {} · 推送 {}",
                        r.playlists_created, r.songs_pulled, r.songs_pushed
                    )));
                    lines.push(Line::from(format!(
                        "收藏拉取 {} · 收藏推送 {} · 未匹配 {}",
                        r.favorites_pulled, r.favorites_pushed, r.unmatched
                    )));
                    lines.push(Line::from("[Enter/Esc] 返回"));
                }
            }
            SyncPhase::Failed | SyncPhase::Cancelled => {
                lines.push(Line::from(if s.phase == SyncPhase::Cancelled {
                    "同步已取消"
                } else {
                    "同步失败"
                }));
                lines.push(Line::from(s.error.unwrap_or_else(|| "未知错误".into())));
                lines.push(Line::from(""));
                lines.push(Line::from("[R] 重试预览    [Esc] 返回"));
            }
        }
        Paragraph::new(lines)
            .style(Style::new().fg(crate::theme::text(ctx)))
            .render(inner, buf);
    }
}
