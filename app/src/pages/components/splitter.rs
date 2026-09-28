//! 可拖拽分割条（Splitter）的通用件：命中判定 + 拖拽状态机 + 比例夹取。
//!
//! 以前 Queue / Leaderboard / Playlists 各写了一套同构实现，并因此分叉出两类问题：
//!
//! 1. **方向靠几何猜**：Queue 用 `layout.wide` 明确决定"这次只看竖线还是只看横线"，
//!    另外两个页面把横/竖两个 `if` 串行无条件求值，于是窄屏下"每行最右一列"、
//!    宽屏下"面板最后一行"都会被当成分割条抓走，正常点击被吞、拖拽方向还是错的。
//!    这里用 [`DividerHit`] 把"方向"变成入参，逼调用方先决定方向再命中。
//! 2. **预览/提交/取消各写一遍**：这里用 [`Splitter`] 统一。
//!
//! 分割条的绘制由各页面负责（颜色/字符与主题相关），但**绘制坐标必须是**
//! 这里命中用的同一个 `divider` 值，否则又会出现"看得到却抓不住"。

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
}
