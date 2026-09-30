//! 可拖拽分割条（Splitter）的通用件：命中判定 + 拖拽状态机 + 比例夹取。
//!
//! 以前 Queue / Leaderboard / Playlists 各写了一套同构实现，并因此分叉出三类问题：
//!
//! 1. **方向靠几何猜**：Queue 用 `layout.wide` 明确决定"这次只看竖线还是只看横线"，
//!    另外两个页面把横/竖两个 `if` 串行无条件求值，于是窄屏下"每行最右一列"、
//!    宽屏下"面板最后一行"都会被当成分割条抓走，正常点击被吞、拖拽方向还是错的。
//!    这里用 [`DividerHit`] 把"方向"变成入参，逼调用方先决定方向再命中。
//! 2. **预览/提交/取消各写一遍**：这里用 [`Splitter`] 统一。
//! 3. **分割线与面板抢格子**：分割线压在"上/左面板的最后一行/列"上，那一格同时
//!    还想当面板内容（或边框），于是面板的最后一行内容既看不见也点不到。
//!    现在分割线占**它自己**的 1 行/1 列（[`GUTTER`]）：分栏一律走
//!    [`split_with_gutter`]，分割线位置一律走 [`divider_line`]。
//!
//! 分割条的绘制由各页面负责（颜色/字符与主题相关），但**绘制坐标必须是**
//! 这里命中用的同一个 `divider` 值，否则又会出现"看得到却抓不住"。

use ratatui::layout::Rect;

/// 分割条方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
    /// 竖直分割线：左右两个 Pane 之间，拖动改变宽度（命中比较 x）。
    Vertical,
    /// 水平分割线：上下两个 Pane 之间，拖动改变高度（命中比较 y）。
    Horizontal,
}

/// 抓取容差（列/行）。三个页面以前各定义一份，值都是 1，这里统一。
pub const GRAB_RADIUS: u16 = 1;

/// 一条分割线的命中定义。
///
/// `divider` 是分割线所在的列（[`SplitAxis::Vertical`]）或行（[`SplitAxis::Horizontal`]）；
/// `span` 是分割线自身的跨度区间（竖线给 y 区间，横线给 x 区间，半开区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DividerHit {
    pub axis: SplitAxis,
    pub divider: u16,
    pub span: (u16, u16),
}

impl DividerHit {
    pub fn new(axis: SplitAxis, divider: u16, span: (u16, u16)) -> Self {
        Self {
            axis,
            divider,
            span,
        }
    }

    /// 指针是否落在这条分割线上（±[`GRAB_RADIUS`]，且在该线的跨度内）。
    pub fn matches(&self, column: u16, row: u16) -> bool {
        let (start, end) = self.span;
        if start >= end {
            return false;
        }
        match self.axis {
            SplitAxis::Vertical => {
                column.abs_diff(self.divider) <= GRAB_RADIUS && row >= start && row < end
            }
            SplitAxis::Horizontal => {
                row.abs_diff(self.divider) <= GRAB_RADIUS && column >= start && column < end
            }
        }
    }
}

/// 分割条拖拽状态机。
///
/// 泛型 `T` 是页面自己的"哪条分割线"标识（例如 `ResizeTarget`），
/// 这样布局代码仍然能用强类型 match，而拖拽语义只实现一次。
#[derive(Debug, Clone)]
pub struct Splitter<T> {
    dragging: Option<T>,
    preview: Option<f32>,
}

impl<T> Default for Splitter<T> {
    fn default() -> Self {
        Self {
            dragging: None,
            preview: None,
        }
    }
}

impl<T: Clone + PartialEq> Splitter<T> {
    /// 是否正在拖拽（拖拽期间页面应吞掉其它鼠标事件，避免误触列表）。
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// 正在拖拽的目标。
    pub fn dragging(&self) -> Option<&T> {
        self.dragging.as_ref()
    }

    /// 按下分割线，开始一次拖拽会话。
    pub fn begin(&mut self, target: T, committed: f32) {
        self.dragging = Some(target);
        self.preview = Some(committed);
    }

    /// 拖拽中更新预览比例（只影响布局，不落盘）。
    pub fn drag(&mut self, ratio: f32) {
        if self.dragging.is_some() {
            self.preview = Some(ratio);
        }
    }

    /// 布局函数用：该分割线当前生效的比例（拖拽预览优先于已提交值）。
    pub fn effective(&self, target: &T, committed: f32) -> f32 {
        match (&self.dragging, self.preview) {
            (Some(active), Some(preview)) if active == target => preview,
            _ => committed,
        }
    }

    /// 鼠标抬起：结束会话并返回 `(目标, 待提交比例)`。
    pub fn commit(&mut self) -> Option<(T, f32)> {
        let target = self.dragging.take()?;
        let ratio = self.preview.take()?;
        Some((target, ratio))
    }

    /// 取消本次拖拽，已提交比例不受影响。
    pub fn cancel(&mut self) {
        self.dragging = None;
        self.preview = None;
    }
}

/// 把比例夹到 `[min, max]`，并挡住 NaN（NaN 会让 `clamp` 之后仍是 NaN，
/// 进而把布局算成 0 尺寸）。
pub fn clamp_ratio(ratio: f32, min: f32, max: f32) -> f32 {
    if !ratio.is_finite() {
        return min;
    }
    ratio.clamp(min, max)
}

/// 指针在某个 Pane 内的相对位置换算成比例（竖向 Pane 用宽度，横向用高度）。
pub fn ratio_within(start: u16, extent: u16, pointer: u16) -> f32 {
    if extent == 0 {
        return 0.0;
    }
    (pointer.saturating_sub(start) as f32) / extent as f32
}

/// 分割线**自己**占的 1 格 gutter：夹在两个面板之间，既不属于上/左面板的
/// 内容区、也不属于它的边框，更不覆盖下/右面板的边框。
pub const GUTTER: u16 = 1;

/// 把 `desired` 夹进 `[min, max]`；`max < min`（可用空间不够）时取 `min`。
///
/// 与 `u16::clamp` 的区别只有一个：**不 panic**。窗口被压得很小时
/// （例如榜单条数多、`min_boards_rows` 顶到 11 而内容区只有 7 行）原先的
/// `clamp(min, max)` 会因为 `min > max` 直接 panic；这里退化成"给最小尺寸"，
/// 剩下的由 [`split_with_gutter`] 保证：宁可面板变小，也绝不让分割线压内容。
pub fn clamp_extent(desired: u16, min: u16, max: u16) -> u16 {
    desired.min(max).max(min)
}

/// 沿 `axis` 把 `area` 切成「第一块 · 1 格 gutter · 第二块」。
///
/// `axis` 是**分割线**的方向：[`SplitAxis::Horizontal`] 是上下分栏
/// （gutter 占 1 **行**），[`SplitAxis::Vertical`] 是左右分栏（gutter 占 1 **列**）。
///
/// `first_extent` 是第一块在分栏方向上的长度，会被夹到 `可用长度 - GUTTER`
/// 以内，因此 **gutter 一定存在**：即使 `area` 只剩 1 格，也宁可把第一块压成
/// 0 尺寸（那一块本来也画不出内容），而不是回头去借用它的最后一格。
///
/// 第二块吃掉剩下的全部长度。空间不够时两块都可能退化成 0 尺寸的空矩形
/// （此时调用方按"两块都不 > 0"跳过绘制与命中，见各页 `dividers`/`divider`）。
pub fn split_with_gutter(area: Rect, axis: SplitAxis, first_extent: u16) -> (Rect, Rect) {
    match axis {
        SplitAxis::Horizontal => {
            let first_height = first_extent.min(area.height.saturating_sub(GUTTER));
            let first = Rect::new(area.x, area.y, area.width, first_height);
            let second = Rect::new(
                area.x,
                area.y.saturating_add(first_height).saturating_add(GUTTER),
                area.width,
                area.height
                    .saturating_sub(first_height)
                    .saturating_sub(GUTTER),
            );
            (first, second)
        }
        SplitAxis::Vertical => {
            let first_width = first_extent.min(area.width.saturating_sub(GUTTER));
            let first = Rect::new(area.x, area.y, first_width, area.height);
            let second = Rect::new(
                area.x.saturating_add(first_width).saturating_add(GUTTER),
                area.y,
                area.width
                    .saturating_sub(first_width)
                    .saturating_sub(GUTTER),
                area.height,
            );
            (first, second)
        }
    }
}

/// 第一块与第二块之间那条分割线所在的行（[`SplitAxis::Horizontal`]）
/// 或列（[`SplitAxis::Vertical`]）。
///
/// gutter 紧跟在第一块之后，所以它就是第一块的 `bottom()` / `right()`。
/// **渲染与命中都必须经过这个函数**，不要再各自写 `bottom() - 1` ——
/// 那正是"分割线压住面板最后一格"的来源。
pub fn divider_line(first: Rect, axis: SplitAxis) -> u16 {
    match axis {
        SplitAxis::Horizontal => first.bottom(),
        SplitAxis::Vertical => first.right(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Target {
        WideBoards,
        NarrowBoards,
    }

    /// 回归：窄屏（横竖 Pane 同宽同高）时，方向必须先定死，
    /// 否则"每行最右一列"会被误判成竖分割条，正常点击被吞。
    #[test]
    fn direction_decides_the_hit_axis() {
        // 窄布局：榜单在上、歌曲在下，只应命中水平分割线
        let horizontal = DividerHit::new(SplitAxis::Horizontal, 20, (10, 90));
        assert!(horizontal.matches(50, 20));
        assert!(horizontal.matches(50, 21), "容差 ±1");
        assert!(!horizontal.matches(50, 22));

        // 同一位置若按竖线判定就会误命中，这正是旧实现的 bug
        let vertical = DividerHit::new(SplitAxis::Vertical, 89, (10, 30));
        assert!(vertical.matches(89, 15));
        assert!(!vertical.matches(50, 20), "不在线上");
    }

    #[test]
    fn hit_requires_the_pointer_to_be_within_the_divider_span() {
        let hit = DividerHit::new(SplitAxis::Vertical, 40, (10, 20));
        assert!(hit.matches(40, 10));
        assert!(hit.matches(40, 19));
        assert!(!hit.matches(40, 20), "span 是半开区间");
        assert!(!hit.matches(40, 9));

        // 退化的跨度不应命中
        assert!(!DividerHit::new(SplitAxis::Vertical, 40, (10, 10)).matches(40, 10));
    }

    #[test]
    fn splitter_previews_then_commits() {
        let mut splitter: Splitter<Target> = Splitter::default();
        assert!(!splitter.is_dragging());

        splitter.begin(Target::NarrowBoards, 0.3);
        assert!(splitter.is_dragging());
        assert_eq!(splitter.effective(&Target::NarrowBoards, 0.3), 0.3);

        splitter.drag(0.5);
        assert_eq!(
            splitter.effective(&Target::NarrowBoards, 0.3),
            0.5,
            "拖拽预览优先"
        );
        assert_eq!(
            splitter.effective(&Target::WideBoards, 0.2),
            0.2,
            "别的分割线不受影响"
        );

        assert_eq!(splitter.commit(), Some((Target::NarrowBoards, 0.5)));
        assert!(!splitter.is_dragging());
        assert_eq!(
            splitter.effective(&Target::NarrowBoards, 0.4),
            0.4,
            "提交后以页面保存的值为准"
        );
    }

    #[test]
    fn cancel_keeps_the_committed_ratio() {
        let mut splitter: Splitter<Target> = Splitter::default();
        splitter.begin(Target::WideBoards, 0.36);
        splitter.drag(0.9);
        splitter.cancel();
        assert!(!splitter.is_dragging());
        assert_eq!(splitter.effective(&Target::WideBoards, 0.36), 0.36);
        assert_eq!(splitter.commit(), None, "取消后没有可提交的值");
    }

    #[test]
    fn drag_without_begin_is_ignored() {
        let mut splitter: Splitter<Target> = Splitter::default();
        splitter.drag(0.7);
        assert_eq!(splitter.commit(), None);
    }

    #[test]
    fn clamp_ratio_rejects_non_finite_values() {
        assert_eq!(clamp_ratio(0.5, 0.2, 0.8), 0.5);
        assert_eq!(clamp_ratio(0.0, 0.2, 0.8), 0.2);
        assert_eq!(clamp_ratio(1.0, 0.2, 0.8), 0.8);
        assert_eq!(clamp_ratio(f32::NAN, 0.2, 0.8), 0.2);
        assert_eq!(clamp_ratio(f32::INFINITY, 0.2, 0.8), 0.2);
    }

    #[test]
    fn ratio_within_maps_pointer_to_pane_progress() {
        assert_eq!(ratio_within(10, 100, 10), 0.0);
        assert_eq!(ratio_within(10, 100, 60), 0.5);
        assert_eq!(ratio_within(10, 0, 60), 0.0);
    }

    /// 分隔线占**自己**的那一格：两块面板在分栏方向上被 gutter 隔开，
    /// 面板的最后一格（内容或边框）都不再落在这条线上。
    #[test]
    fn divider_owns_a_gutter_cell_between_the_two_panes() {
        use crate::pages::components::hit_test::panel_inner;

        let area = Rect::new(4, 6, 40, 20);

        let (top, bottom) = split_with_gutter(area, SplitAxis::Horizontal, 7);
        assert_eq!(top, Rect::new(4, 6, 40, 7));
        // 横线占 top.bottom() 那一行，下方面板从它的下一行开始
        assert_eq!(divider_line(top, SplitAxis::Horizontal), 13);
        assert_eq!(bottom.y, 14);
        assert_eq!(bottom.height, 20 - 7 - GUTTER);
        // 面板内容区（去掉四周全框）与 gutter 严格不重叠
        assert!(panel_inner(top).bottom() <= divider_line(top, SplitAxis::Horizontal));
        assert!(panel_inner(bottom).y > divider_line(top, SplitAxis::Horizontal));
        // 下方面板的边框也不与 gutter 重叠（gutter 是独立的一行）
        assert!(bottom.y > divider_line(top, SplitAxis::Horizontal));

        let (left, right) = split_with_gutter(area, SplitAxis::Vertical, 12);
        assert_eq!(left, Rect::new(4, 6, 12, 20));
        assert_eq!(divider_line(left, SplitAxis::Vertical), 16);
        assert_eq!(right.x, 17);
        assert_eq!(right.width, 40 - 12 - GUTTER);
        assert_eq!(left.y, right.y, "左右分栏时两块都占满整个高度");
        assert!(panel_inner(left).right() <= divider_line(left, SplitAxis::Vertical));
        assert!(panel_inner(right).x > divider_line(left, SplitAxis::Vertical));
        assert!(right.x > divider_line(left, SplitAxis::Vertical));

        // 两块长度之和 + 1 格 gutter 恰好用满分栏方向
        assert_eq!(top.height + bottom.height + GUTTER, area.height);
        assert_eq!(left.width + right.width + GUTTER, area.width);
    }

    /// 空间不足时 `split_with_gutter` 也不能把 gutter 还给面板：
    /// 宁可第一块退化成 0 尺寸，也不与第二块紧贴。
    #[test]
    fn gutter_survives_a_squeezed_area() {
        let (first, second) = split_with_gutter(Rect::new(0, 0, 10, 1), SplitAxis::Horizontal, 9);
        assert_eq!(first.height, 0);
        assert_eq!(second.height, 0);
        assert_eq!(divider_line(first, SplitAxis::Horizontal), 0);

        // 一点空间都没有时不 panic，也不产生负数尺寸
        let (first, second) = split_with_gutter(Rect::new(0, 0, 0, 0), SplitAxis::Vertical, 5);
        assert_eq!(first.width, 0);
        assert_eq!(second.width, 0);
    }

    #[test]
    fn clamp_extent_does_not_panic_when_the_min_does_not_fit() {
        assert_eq!(clamp_extent(5, 2, 8), 5);
        assert_eq!(clamp_extent(1, 2, 8), 2);
        assert_eq!(clamp_extent(9, 2, 8), 8);
        // min > max（可用空间不足）：取 min，而不是 `clamp` 那样 panic
        assert_eq!(clamp_extent(9, 11, 7), 11);
        assert_eq!(clamp_extent(0, 11, 7), 11);
    }
}
