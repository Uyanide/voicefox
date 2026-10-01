//! 网易云远程歌单的**独立窗口**（只读浏览）。
//!
//! 以前这些集合挤在「设置 · 账号与扫码」面板底部的一小段里：名称被截断、
//! 数量对不齐、条数一多就只能看到前几行，既看不出全貌也没法找。现在改成
//! 一个居中窗口：列对齐、可滚动、支持排序与过滤，选中回车即跳到「歌单 ·
//! 我的歌单」里的对应歌单继续操作（那里才有播放 / 入队 / 收藏等管理动作）。
//!
//! 数据是 [`lx_core::sync::SyncCollection`] 的只读快照，窗口不修改任何缓存。

use lx_core::sync::{SyncCollection, SyncCollectionKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::context::AppContext;
use crate::pages::components::list_filter::ListFilter;
use crate::pages::sort::compare_text;
use crate::pages::components::scroll::ensure_visible;
use crate::pages::components::text::{pad_display, pad_display_left, truncate_width};

/// 远程缓存里的网易云账号歌单（只读快照）。
///
/// 放在这里而不是各调用点各写一遍：设置页的窗口、歌单页的「我的歌单」入口
/// 必须看到同一份口径（只取 Playlist，丢掉红心集合——红心在收藏页）。
pub fn account_collections() -> Vec<SyncCollection> {
    let mut collections = crate::remote_cache::all_netease();
    collections.retain(|collection| collection.kind != SyncCollectionKind::Favorites);
    collections
}

/// 窗口的排序口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionSort {
    /// 按名称（默认）
    Name,
    /// 按歌曲数，从多到少
    Songs,
}

impl CollectionSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "名称",
            Self::Songs => "歌曲数",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Name => Self::Songs,
            Self::Songs => Self::Name,
        }
    }
}

/// 窗口按键处理的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCollectionsOutcome {
    /// 继续开着窗口
    None,
    /// 关闭窗口
    Closed,
    /// 选中一条集合：调用方跳转到「我的歌单」并定位它（`id` 是远端歌单 id）
    Open(String),
}

#[derive(Debug, Clone)]
pub struct RemoteCollectionsWindow {
    collections: Vec<SyncCollection>,
    selected: usize,
    offset: usize,
    sort: CollectionSort,
    /// 过滤输入与条件：复用列表页统一的 `ListFilter`（INSERT/FILTER 语义、
    /// 按键吞噬规则都一致），不在这里再写一套过滤输入态。
    filter: ListFilter,
}

impl RemoteCollectionsWindow {
    pub fn new(collections: Vec<SyncCollection>) -> Self {
        Self {
            collections,
            selected: 0,
            offset: 0,
            sort: CollectionSort::Name,
            filter: ListFilter::new(),
        }
    }

    /// 用最新缓存替换内容，尽量保住当前选中的那条。
    pub fn refresh(&mut self, collections: Vec<SyncCollection>) {
        let selected_id = self.selected_id();
        self.collections = collections;
        if let Some(id) = selected_id
            && let Some(index) = self.visible().iter().position(|item| item.id == id)
        {
            self.selected = index;
        }
        self.clamp_selection();
    }

    /// 当前排序口径（渲染汇总行与测试断言共用）。
    pub fn sort_label(&self) -> &'static str {
        self.sort.label()
    }

    pub fn is_filtering(&self) -> bool {
        self.filter.is_active()
    }

    /// 当前选中集合的远端 id（跳转时用来定位）。
    pub fn selected_id(&self) -> Option<String> {
        self.visible().get(self.selected).map(|item| item.id.clone())
    }

    /// 过滤 + 排序后的可见集合。
    pub fn visible(&self) -> Vec<&SyncCollection> {
        let keyword = (!self.filter.query().trim().is_empty())
            .then(|| self.filter.query().trim().to_lowercase());
        let mut items: Vec<&SyncCollection> = self
            .collections
            .iter()
            .filter(|collection| {
                keyword
                    .as_deref()
                    .is_none_or(|keyword| collection.name.to_lowercase().contains(keyword))
            })
            .collect();
        match self.sort {
            // 比较器里用免分配的 `compare_text`（以前每次比较都 to_lowercase 分配）
            CollectionSort::Name => items.sort_by(|a, b| {
                compare_text(&a.name, &b.name).then_with(|| a.id.cmp(&b.id))
            }),
            CollectionSort::Songs => items.sort_by(|a, b| {
                b.songs
                    .len()
                    .cmp(&a.songs.len())
                    .then_with(|| compare_text(&a.name, &b.name))
            }),
        }
        items
    }

    fn clamp_selection(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = self.selected.min(len - 1);
    }

    /// 处理一次按键；返回窗口的去留与是否选中了某条。
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> RemoteCollectionsOutcome {
        use crossterm::event::{KeyCode, KeyModifiers};

        // 过滤输入交给统一的 ListFilter：Esc/Enter 结束输入但保留条件，
        // 其余按键全部被它吃掉（不会漏给底下的排序 / 关窗快捷键）。
        if self.filter.is_active() {
            self.filter.handle_input(&key);
            self.selected = 0;
            self.clamp_selection();
            return RemoteCollectionsOutcome::None;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => RemoteCollectionsOutcome::Closed,
            (KeyModifiers::NONE, KeyCode::Enter) => self
                .selected_id()
                .map_or(RemoteCollectionsOutcome::None, RemoteCollectionsOutcome::Open),
            (KeyModifiers::NONE, KeyCode::Char('/')) => {
                self.filter.reset();
                self.filter.activate();
                self.selected = 0;
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::Char('s')) => {
                self.sort = self.sort.next();
                self.selected = 0;
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::Up | KeyCode::Char('k')) => {
                self.selected = self.selected.saturating_sub(1);
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::Down | KeyCode::Char('j')) => {
                if self.selected + 1 < self.visible().len() {
                    self.selected += 1;
                }
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::Home | KeyCode::Char('g')) => {
                self.selected = 0;
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::End | KeyCode::Char('G')) => {
                self.selected = self.visible().len().saturating_sub(1);
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.selected = self.selected.saturating_sub(10);
                RemoteCollectionsOutcome::None
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                self.selected = (self.selected + 10).min(self.visible().len().saturating_sub(1));
                RemoteCollectionsOutcome::None
            }
            _ => RemoteCollectionsOutcome::None,
        }
    }

    /// 窗口矩形：居中，尽量大但不占满整屏，且**永不越界**（小终端下按可用空间收缩）。
    pub fn window_rect(area: Rect) -> Rect {
        let width = area.width.saturating_sub(8).clamp(40, 92).min(area.width);
        let height = area.height.saturating_sub(6).clamp(7, 26).min(area.height);
        Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        )
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let popup = Self::window_rect(area);
        Clear.render(popup, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(crate::theme::accent(ctx)))
            .title(" 网易云远程歌单 ");
        let inner = block.inner(popup);
        block.render(popup, buf);
        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // 列宽：数量列定宽，名称列吃掉剩余宽度
        let name_width = (inner.width as usize)
            .saturating_sub(1 + 2 + 1 + 1 + COUNT_COLUMN_WIDTH)
            .max(4);

        // 先把"要画的文本"和滚动位置算出来，再开始往缓冲里写。
        // 渲染函数的 `self` 借用与写入顺序必须分开：先取数据，后写状态。
        let (rows, total, normal, favorites, list_height) = {
            let all: Vec<&SyncCollection> = self.visible();
            let list_height = inner
                .height
                .saturating_sub(3)
                .min(all.len() as u16)
                .max(1) as usize;
            let (normal, favorites) = all.iter().fold((0usize, 0usize), |counts, item| {
                if item.kind == SyncCollectionKind::Favorites {
                    (counts.0, counts.1 + 1)
                } else {
                    (counts.0 + 1, counts.1)
                }
            });
            let rows: Vec<[String; 5]> = all
                .iter()
                .map(|item| collection_row(item, name_width))
                .collect();
            (rows, all.len(), normal, favorites, list_height)
        };
        // `self` 的不可变借用已经结束，这时才写回滚动位置
        ensure_visible(self.selected, list_height, total, &mut self.offset);
        let list_top = inner.y.saturating_add(2);

        let muted = crate::theme::muted(ctx);
        let query = self.filter.query().trim();
        let summary = if query.is_empty() {
            format!(
                " 共 {total} 个 · 普通 {normal} / 红心 {favorites} · 排序: {} ",
                self.sort_label()
            )
        } else if self.filter.is_active() {
            format!(" 过滤(输入中): {query}▏")
        } else {
            format!(" 共 {total} 个 · 过滤: {query} · 排序: {} ", self.sort_label())
        };
        Paragraph::new(Line::from(Span::styled(summary, Style::new().fg(muted)))).render(
            Rect::new(inner.x, inner.y, inner.width, 1),
            buf,
        );

        let header = Line::from(vec![
            Span::raw(" "),
            Span::raw("类型"),
            Span::raw("  "),
            Span::raw(pad_display("名称", name_width)),
            Span::raw(pad_display_left("歌曲", COUNT_COLUMN_WIDTH)),
        ]);
        Paragraph::new(header)
            .style(Style::new().fg(crate::theme::overlay0(ctx)))
            .render(Rect::new(inner.x, inner.y + 1, inner.width, 1), buf);

        if rows.is_empty() {
            let hint = if self.collections.is_empty() {
                " 还没有缓存任何远程歌单：选中「刷新远程歌单」行按 Enter 拉取。"
            } else {
                " 没有匹配的歌单。"
            };
            Paragraph::new(hint)
                .style(Style::new().fg(crate::theme::yellow(ctx)))
                .render(Rect::new(inner.x, list_top, inner.width, 1), buf);
        }

        for (row, cells) in rows.iter().skip(self.offset).take(list_height).enumerate() {
            let selected = self.offset + row == self.selected;
            let base = if selected {
                Style::new()
                    .fg(crate::theme::selection_fg(ctx))
                    .bg(crate::theme::accent(ctx))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(crate::theme::text(ctx))
            };
            let line = Line::from(
                cells
                    .iter()
                    .map(|text| Span::styled(text.clone(), base))
                    .collect::<Vec<_>>(),
            );
            Paragraph::new(line).render(
                Rect::new(inner.x, list_top + row as u16, inner.width, 1),
                buf,
            );
        }

        let footer_row = inner.bottom().saturating_sub(1);
        // 页码右对齐、提示靠左，画在同一行：此前页码先画、提示无条件覆盖，
        // 列表超长时页码永远看不见。
        if total > list_height {
            let position = format!("{} / {} ", self.selected + 1, total);
            let position_width = UnicodeWidthStr::width(position.as_str()) as u16;
            if inner.width > position_width {
                Paragraph::new(Line::from(Span::styled(
                    position,
                    Style::new().fg(crate::theme::overlay0(ctx)),
                )))
                .alignment(Alignment::Right)
                .render(Rect::new(inner.x, footer_row, inner.width, 1), buf);
            }
        }

        let footer = if self.is_filtering() {
            " 输入过滤词 · Enter/Esc 结束过滤"
        } else {
            " Enter 进入「我的歌单」 · s 切换排序 · / 过滤 · Esc 关闭"
        };
        Paragraph::new(Line::from(Span::styled(
            footer,
            Style::new().fg(crate::theme::overlay0(ctx)),
        )))
        .render(Rect::new(inner.x, footer_row, inner.width, 1), buf);
    }
}

/// 一行的纯文本切分：`[" ", 类型, "  ", 名称(定宽), 数量(右对齐)]`。
///
/// 渲染与测试共用这一份，因此"名称被截断、数量列永远对齐"是可断言的。
fn collection_row(item: &SyncCollection, name_width: usize) -> [String; 5] {
    let kind = if item.kind == SyncCollectionKind::Favorites {
        "红心"
    } else {
        "歌单"
    };
    [
        " ".to_string(),
        kind.to_string(),
        "  ".to_string(),
        pad_display(
            truncate_width(&item.name, name_width).as_ref(),
            name_width,
        ),
        pad_display_left(&item.songs.len().to_string(), COUNT_COLUMN_WIDTH),
    ]
}

/// 数量列宽（`歌曲数` 右对齐到这个宽度）。
const COUNT_COLUMN_WIDTH: usize = 7;

#[cfg(test)]
mod tests {
    use super::{RemoteCollectionsOutcome, RemoteCollectionsWindow};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use lx_core::model::song::SongInfo;
    use lx_core::model::source::SourceId;
    use lx_core::sync::{SyncCollection, SyncCollectionKind};
    use ratatui::layout::Rect;
    use unicode_width::UnicodeWidthStr;

    fn collection(id: &str, name: &str, songs: usize, kind: SyncCollectionKind) -> SyncCollection {
        SyncCollection {
            kind,
            id: id.to_string(),
            name: name.to_string(),
            source: SourceId::Wy,
            songs: (0..songs)
                .map(|index| {
                    SongInfo::new(
                        format!("{id}-{index}"),
                        SourceId::Wy,
                        format!("song {index}"),
                        "artist".to_string(),
                    )
                })
                .collect(),
        }
    }

    fn window() -> RemoteCollectionsWindow {
        RemoteCollectionsWindow::new(vec![
            collection("1", "beta", 3, SyncCollectionKind::Playlist),
            collection("2", "Alpha", 10, SyncCollectionKind::Playlist),
            collection("3", "我喜欢的音乐", 20, SyncCollectionKind::Favorites),
        ])
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn sorts_by_name_then_by_song_count() {
        let mut window = window();
        let names: Vec<&str> = window.visible().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "beta", "我喜欢的音乐"]);

        assert_eq!(
            window.handle_key(key(KeyCode::Char('s'))),
            RemoteCollectionsOutcome::None
        );
        assert_eq!(window.sort_label(), "歌曲数");
        let names: Vec<&str> = window.visible().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["我喜欢的音乐", "Alpha", "beta"]);
    }

    #[test]
    fn filter_narrows_the_list_and_escape_leaves_filtering_first() {
        let mut window = window();
        window.handle_key(key(KeyCode::Char('/')));
        assert!(window.is_filtering());
        for character in "alp".chars() {
            window.handle_key(key(KeyCode::Char(character)));
        }
        let names: Vec<&str> = window.visible().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Alpha"]);

        // Esc 先结束过滤输入，**保留过滤条件**（与列表页的 ListFilter 语义一致：
        // Esc 退出 INSERT 不回退 FILTER），也不关窗口
        assert_eq!(
            window.handle_key(key(KeyCode::Esc)),
            RemoteCollectionsOutcome::None
        );
        assert!(!window.is_filtering(), "Esc 应退出输入态");
        assert_eq!(window.visible().len(), 1, "条件仍生效");
        // 关窗口要再按一次 Esc
        assert_eq!(
            window.handle_key(key(KeyCode::Esc)),
            RemoteCollectionsOutcome::Closed
        );
    }

    #[test]
    fn enter_reports_the_selected_remote_id() {
        let mut window = window();
        // 默认按名称排序：Alpha(1) → beta(2) → 我喜欢的音乐(3)
        // 名称序：Alpha(id=2) → beta(id=1) → 我喜欢的音乐(id=3)
        window.handle_key(key(KeyCode::Down));
        assert_eq!(window.selected_id().as_deref(), Some("1"));
        assert_eq!(
            window.handle_key(key(KeyCode::Enter)),
            RemoteCollectionsOutcome::Open("1".to_string())
        );
        assert_eq!(
            window.handle_key(key(KeyCode::Esc)),
            RemoteCollectionsOutcome::Closed
        );
    }

    #[test]
    fn selection_stays_inside_the_visible_list() {
        let mut window = window();
        for _ in 0..10 {
            window.handle_key(key(KeyCode::Down));
        }
        assert_eq!(window.selected_id().as_deref(), Some("3"), "应停在最后一条");
        // 过滤后选中不越界
        window.handle_key(key(KeyCode::Char('/')));
        window.handle_key(key(KeyCode::Char('z')));
        assert!(window.visible().is_empty());
        assert_eq!(window.selected_id(), None);
    }

    #[test]
    fn number_column_is_right_aligned() {
        use crate::pages::components::text::{pad_display, pad_display_left};

        assert_eq!(pad_display_left("7", 7), "      7");
        assert_eq!(UnicodeWidthStr::width(pad_display("红心", 6).as_str()), 6);
        assert_eq!(UnicodeWidthStr::width(pad_display("beta", 6).as_str()), 6);
    }

    /// 渲染用的一行文本（直接取渲染用的切分函数，因此断言的是真实渲染口径）。
    fn row_text(item: &SyncCollection, name_width: usize) -> String {
        super::collection_row(item, name_width).concat()
    }

    #[test]
    fn row_layout_keeps_long_names_inside_their_column() {
        let long = collection("9", "那些绝不会忘记的国漫主题曲", 173, SyncCollectionKind::Playlist);
        let row = row_text(&long, 12);
        // 名称列被截断且宽度固定，数字列因此永远对齐
        assert!(row.contains('…'), "超长名称应被截断: {row:?}");
        assert!(row.ends_with("    173"), "数量应右对齐: {row:?}");
        assert_eq!(UnicodeWidthStr::width(row.as_str()), 1 + 4 + 2 + 12 + 7);

        let short = collection("8", "7", 1, SyncCollectionKind::Playlist);
        let row = row_text(&short, 12);
        assert_eq!(UnicodeWidthStr::width(row.as_str()), 1 + 4 + 2 + 12 + 7);
        assert!(row.ends_with("      1"));
    }

    #[test]
    fn window_is_centered_and_never_larger_than_the_screen() {
        let area = Rect::new(0, 0, 100, 30);
        let popup = RemoteCollectionsWindow::window_rect(area);
        assert!(area.union(popup) == area, "窗口不能越界");
        assert_eq!(popup.x, (100 - popup.width) / 2);
        assert_eq!(popup.y, (30 - popup.height) / 2);

        // 很小的终端也要给出可渲染的矩形
        let tiny = RemoteCollectionsWindow::window_rect(Rect::new(0, 0, 20, 6));
        assert!(tiny.width > 0 && tiny.height > 0);
        assert!(tiny.right() <= 20 && tiny.bottom() <= 6);
    }
}
