//! Shared terminal hit-testing helpers. Rendering owns the Rect; mouse handling reuses it.
//!
//! 这里的两个件解决同一类问题：**渲染与鼠标命中必须是同一份几何**。
//!
//! - [`row_at`]：给"面板内固定 N 行头部 + 单行列表"的简单场景用（表头行数写死）。
//! - [`PanelRows`]：给"面板内还有过滤行 / 工具行（音源条等）"的场景用，
//!   把这几行**算一次**，渲染与命中都从这里取。
//!
//! 历史上 Favorites 正是因为渲染画了"过滤行 + 音源条 + 表头"三行、
//! 而命中只按两行记账，导致点击恒定选中下一行；History 则是渲染与命中
//! 各用一套 `filter_visible` 判定。凡是"面板内多出来的行"，都应该走
//! [`PanelRows`]，不要再在页面里手写 `inner.y + n`。

use ratatui::layout::{Position, Rect};
use ratatui::widgets::{Block, Borders};

/// Return the absolute list index for a one-line-per-row list inside `area`.
/// `header_rows` is the number of non-list rows inside the block.
pub fn row_at(
    area: Rect,
    position: Position,
    scroll: usize,
    len: usize,
    header_rows: u16,
) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let list = Rect::new(
        inner.x,
        inner.y.saturating_add(header_rows),
        inner.width,
        inner.height.saturating_sub(header_rows),
    );
    index_in(list, position, scroll, len)
}

/// 列表矩形内的绝对坐标 → 列表下标（带滚动偏移与越界判断）。
pub(crate) fn index_in(list: Rect, position: Position, scroll: usize, len: usize) -> Option<usize> {
    if !list.contains(position) {
        return None;
    }
    let index = scroll + position.y.saturating_sub(list.y) as usize;
    (index < len).then_some(index)
}

/// 面板内部的"行账本"：过滤行 / 工具行 / 表头 / 列表区，自上而下依次占位。
///
/// 一次算清后，渲染与鼠标命中取同一份结果，从结构上消除"点击差一行"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelRows {
    /// 去掉边框后的可用区。
    pub inner: Rect,
    filter: Option<Rect>,
    toolbar: Option<Rect>,
    /// 列头行（若该页面有表头）。
    pub header: Option<Rect>,
    /// 数据行区域。
    pub list: Rect,
}

impl PanelRows {
    /// 按 `过滤行 → 工具行 → 表头 → 列表` 的顺序自上而下排列。
    ///
    /// 可见但空间不够的行会被截断为 `None`，列表区高度相应为 0。
    pub fn new(
        area: Rect,
        filter_visible: bool,
        toolbar_visible: bool,
        header_visible: bool,
    ) -> Self {
        let inner = Block::default().borders(Borders::ALL).inner(area);
        let bottom = inner.bottom();
        let mut y = inner.y;

        let take = |y: &mut u16, visible: bool| -> Option<Rect> {
            if !visible || *y >= bottom {
                return None;
            }
            let row = Rect::new(inner.x, *y, inner.width, 1);
            *y = y.saturating_add(1);
            Some(row)
        };

        let filter = take(&mut y, filter_visible);
        let toolbar = take(&mut y, toolbar_visible);
        let header = take(&mut y, header_visible);
        let list = Rect::new(inner.x, y, inner.width, bottom.saturating_sub(y));
        Self {
            inner,
            filter,
            toolbar,
            header,
            list,
        }
    }

    /// 过滤行矩形（不可见或空间不足时为 `None`）。
    pub fn filter_row(&self) -> Option<Rect> {
        self.filter
    }

    /// 工具行矩形（音源条、搜索条等；不可见或空间不足时为 `None`）。
    pub fn toolbar_row(&self) -> Option<Rect> {
        self.toolbar
    }

    /// 列表区内的绝对坐标 → 列表下标。
    pub fn index_at(&self, position: Position, scroll: usize, len: usize) -> Option<usize> {
        index_in(self.list, position, scroll, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_hit_test_uses_the_same_rect_boundaries_as_rendering() {
        let area = Rect::new(10, 5, 40, 12);
        assert_eq!(row_at(area, Position::new(11, 7), 3, 20, 1), Some(3));
        assert_eq!(row_at(area, Position::new(10, 7), 3, 20, 1), None);
        assert_eq!(row_at(area, Position::new(20, 17), 3, 20, 1), None);
    }

    #[test]
    fn panel_rows_stack_filter_toolbar_and_header_in_order() {
        // inner = (11,6,38,10) → 行从 y=6 开始
        let rows = PanelRows::new(Rect::new(10, 5, 40, 12), true, true, true);

        assert_eq!(rows.filter_row().map(|r| r.y), Some(6));
        assert_eq!(rows.toolbar_row().map(|r| r.y), Some(7));
        assert_eq!(rows.header.map(|r| r.y), Some(8));
        assert_eq!(rows.list.y, 9, "列表首行必须排在过滤行/工具行/表头之后");
        assert_eq!(rows.list.height, 7);
    }

    /// Favorites 那个"点击恒定选中下一行"的回归测试：
    /// 工具行（音源条）**必须**计入列表偏移。
    #[test]
    fn list_offset_counts_the_toolbar_row() {
        let area = Rect::new(10, 5, 40, 12);
        let with_toolbar = PanelRows::new(area, false, true, true);
        let without_toolbar = PanelRows::new(area, false, false, true);

        assert_eq!(with_toolbar.list.y, without_toolbar.list.y + 1);

        // 列表首行必须解析出下标 0
        assert_eq!(
            with_toolbar.index_at(
                Position::new(with_toolbar.list.x + 1, with_toolbar.list.y),
                0,
                5
            ),
            Some(0)
        );
        // 表头行不属于列表区（旧的少算一行的算术会把它判成下标 0）
        assert_eq!(
            with_toolbar.index_at(
                Position::new(with_toolbar.list.x + 1, with_toolbar.header.unwrap().y),
                0,
                5
            ),
            None,
            "表头行不能被判成列表首行"
        );
    }

    #[test]
    fn panel_rows_clip_rows_when_there_is_no_room() {
        // inner 高度只有 1：过滤行占掉后其余都不可见
        let rows = PanelRows::new(Rect::new(0, 0, 10, 3), true, true, true);
        assert!(rows.filter_row().is_some());
        assert!(rows.toolbar_row().is_none());
        assert!(rows.header.is_none());
        assert_eq!(rows.list.height, 0);

        // 高度为 0 的边框面板不应 panic
        let empty = PanelRows::new(Rect::new(0, 0, 10, 0), true, true, true);
        assert_eq!(empty.list.height, 0);
        assert!(empty.filter_row().is_none());
    }
}
