//! 封面框的排版计算

use ratatui::layout::Rect;
use ratatui_image::FontSize;

// 封面宽高比的合理范围
const MIN_IMAGE_ASPECT: f32 = 0.2;
const MAX_IMAGE_ASPECT: f32 = 5.0;
/// 封面宽高比的回退值，1.0 为方图
pub const DEFAULT_IMAGE_ASPECT: f32 = 1.0;

/// 终端单元格的高宽比回退值
const DEFAULT_CELL_ASPECT: f32 = 2.0;

/// 封面排版参数
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverGeometry {
    /// 终端单元格的 高/宽
    cell_aspect: f32,
    /// 封面像素的 宽/高
    image_aspect: f32,
}

impl CoverGeometry {
    /// 把数值夹进 [min, max]，非有限值与非正数取 fallback
    fn sanitize(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
        if value.is_finite() && value > 0.0 {
            value.clamp(min, max)
        } else {
            fallback
        }
    }

    pub fn new(cell_aspect: f32, image_aspect: f32) -> Self {
        Self {
            cell_aspect,
            image_aspect: CoverGeometry::sanitize(
                image_aspect,
                MIN_IMAGE_ASPECT,
                MAX_IMAGE_ASPECT,
                DEFAULT_IMAGE_ASPECT,
            ),
        }
    }

    /// 用终端字体的像素尺寸构造，宽或高为 0 时单元格按 2:1 计算
    pub fn from_font_size(font_size: FontSize, image_aspect: f32) -> Self {
        let FontSize { width, height } = font_size;
        let cell_aspect = if width == 0 || height == 0 {
            DEFAULT_CELL_ASPECT
        } else {
            f32::from(height) / f32::from(width)
        };
        Self::new(cell_aspect, image_aspect)
    }

    /// 封面铺满 columns 列时要占的行数，至少 1 行
    fn rows_for(self, columns: u16) -> u16 {
        (f32::from(columns) / (self.image_aspect * self.cell_aspect))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    /// 封面铺满 rows 行时要占的列数，至少 1 列
    fn columns_for(self, rows: u16) -> u16 {
        (f32::from(rows) * self.image_aspect * self.cell_aspect)
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    /// 封面框应有的高度（含边框），返回 0 表示可用空间不足
    pub fn box_height(self, box_width: u16, max_height: u16) -> u16 {
        let inner_width = box_width.saturating_sub(2);
        if inner_width == 0 || max_height < 3 {
            return 0;
        }
        self.rows_for(inner_width).saturating_add(2).min(max_height)
    }

    /// 封面图片在框内 inner 区域中的实际位置，返回的矩形不会超出 inner
    pub fn image_rect(self, inner: Rect) -> Rect {
        if inner.width == 0 || inner.height == 0 {
            return Rect::ZERO;
        }
        // 高度充足时按宽度铺满，垂直居中
        let fit_height = self.rows_for(inner.width);
        if fit_height <= inner.height {
            let y = inner.y + (inner.height - fit_height) / 2;
            return Rect::new(inner.x, y, inner.width, fit_height);
        }
        // 高度不足时按高度反推宽度，水平居中
        let fit_width = self.columns_for(inner.height).min(inner.width);
        let x = inner.x + (inner.width - fit_width) / 2;
        Rect::new(x, inner.y, fit_width, inner.height)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui_image::FontSize;

    use super::CoverGeometry;

    /// 方形封面 + 2:1 单元格
    fn square() -> CoverGeometry {
        CoverGeometry::new(2.0, 1.0)
    }

    #[test]
    fn box_height_follows_the_cell_aspect() {
        // inner 宽 41 → 41/2 ≈ 21 行内容，加上下边框 = 23
        assert_eq!(square().box_height(43, 24), 23);
        // 超出 max_height 时取 max_height
        assert_eq!(square().box_height(43, 8), 8);
        // 内容区不足一行时返回 0
        assert_eq!(square().box_height(43, 2), 0);
        assert_eq!(square().box_height(2, 24), 0);
    }

    #[test]
    fn box_height_follows_the_image_aspect() {
        assert_eq!(CoverGeometry::new(2.0, 0.745).box_height(43, 40), 30);
        assert_eq!(square().box_height(43, 40), 23);
        assert_eq!(CoverGeometry::new(2.0, 1.78).box_height(43, 40), 14);
    }

    #[test]
    fn cover_image_is_centered_and_never_leaves_the_box() {
        // 高度充足：正好占满 inner
        let inner = Rect::new(5, 8, 40, 20);
        assert_eq!(square().image_rect(inner), inner);

        // 高度被压缩：按高度反推宽度，水平居中留边
        let squashed = Rect::new(5, 8, 40, 6);
        assert_eq!(square().image_rect(squashed), Rect::new(19, 8, 12, 6));

        // 任何比例、任何尺寸的框，图片都不能越界
        for aspect in [0.2, 0.444, 0.745, 1.0, 1.78, 5.0] {
            let geometry = CoverGeometry::new(2.0, aspect);
            for width in [1, 3, 12, 41, 200] {
                for height in [1, 2, 7, 23, 90] {
                    let inner = Rect::new(5, 8, width, height);
                    let rect = geometry.image_rect(inner);
                    assert_eq!(inner.union(rect), inner, "{aspect} {width}x{height}");
                    assert!(rect.width > 0 && rect.height > 0);
                }
            }
        }
    }

    #[test]
    fn geometry_falls_back_on_degenerate_ratios() {
        // NaN 回退到默认宽高比
        assert_eq!(
            CoverGeometry::new(0.0, f32::NAN),
            CoverGeometry::new(0.0, super::DEFAULT_IMAGE_ASPECT)
        );
    }

    #[test]
    fn geometry_clamps_on_extreme_ratios() {
        // 极端但有限的比例被夹进合理区间
        assert_eq!(
            CoverGeometry::new(2.0, 1000.0),
            CoverGeometry::new(2.0, super::MAX_IMAGE_ASPECT)
        );
        assert_eq!(
            CoverGeometry::new(2.0, 0.001),
            CoverGeometry::new(2.0, super::MIN_IMAGE_ASPECT)
        );
    }

    #[test]
    fn font_size_maps_to_cell_aspect() {
        // 10x20 的单元格 = 2:1
        assert_eq!(
            CoverGeometry::from_font_size(FontSize::new(10, 20), 1.0),
            CoverGeometry::new(2.0, 1.0)
        );
        // 字号为 0 时回退到默认单元格比例
        assert_eq!(
            CoverGeometry::from_font_size(FontSize::new(0, 0), 1.0),
            CoverGeometry::new(super::DEFAULT_CELL_ASPECT, 1.0)
        );
    }
}
