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
            SyncPhase::Preparing => lines.push(Line::from("正在读取网易云远程歌单并与缓存比对...")),
            SyncPhase::Preview => {
                if let Some(p) = s.preview {
                    lines.push(Line::from(Span::styled(
                        "本次只把网易云歌单读进远程缓存，不会修改任何远端或本地数据。",
                        Style::new().add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(
                        "远程歌单   {} 个 · 红心 {} 个",
                        p.playlists, p.favorites
                    )));
                    lines.push(Line::from(format!("歌曲合计   {} 首", p.songs)));
                    lines.push(Line::from(format!(
                        "缓存变化   新增 {} · 更新 {} · 移除 {}",
                        p.added, p.updated, p.removed
                    )));
                    if !p.failed.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("未取到 {} 个歌单，将保留缓存里的旧数据", p.failed.len()),
                            Style::new().fg(crate::theme::yellow(ctx)),
                        )));
                    }
                    lines.push(Line::from(""));
                    let room = inner
                        .height
                        .saturating_sub(8 + p.failed.len().min(5) as u16)
                        as usize;
                    for d in p.rows.iter().take(room) {
                        lines.push(Line::from(format!(
                            "  {} {}  {}→{}  [{}]",
                            d.kind, d.name, d.cached, d.remote, d.status
                        )));
                    }
                    if !p.failed.is_empty() {
                        lines.push(Line::from(""));
                        lines.push(Line::from("未取到的歌单："));
                        for (name, reason) in p.failed.iter().take(5) {
                            lines.push(Line::from(format!("  ? {name} — {reason}")));
                        }
                        if p.failed.len() > 5 {
                            lines.push(Line::from(format!("  ... 还有 {} 个", p.failed.len() - 5)));
                        }
                    }
                    lines.push(Line::from(""));
                    lines.push(Line::from("[Enter/S] 确认刷新    [Esc] 取消"));
                }
            }
            SyncPhase::Running => {
                let (done, total) = self.control.progress();
                lines.push(Line::from("正在从网易云读取远程歌单，不会写入本地歌单。"));
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
                    lines.push(Line::from("网易云远程歌单已刷新"));
                    lines.push(Line::from(format!(
                        "歌单 {} 个 · 红心 {} 个 · 歌曲 {} 首",
                        r.playlists, r.favorites, r.songs
                    )));
                    lines.push(Line::from(format!(
                        "新增 {} · 更新 {} · 移除 {}",
                        r.added, r.updated, r.removed
                    )));
                    if !r.failed.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("{} 个歌单未取到，已保留缓存里的旧数据", r.failed.len()),
                            Style::new().fg(crate::theme::yellow(ctx)),
                        )));
                        for (name, reason) in r.failed.iter().take(5) {
                            lines.push(Line::from(format!("  ? {name} — {reason}")));
                        }
                    }
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
