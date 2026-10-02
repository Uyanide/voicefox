//! 频谱的 ratatui 渲染（视觉设计对齐 cava）：
//!
//! - **连续纵向渐变**：主题色锚点（blue → sapphire → sky → teal → green）经
//!   `colorgrad` 插值，每个"半行"按全高比例取色，柱子内部也有色彩过渡；
//! - **cava 式柱槽**：2 列柱宽 + 1 列间隙（窄终端自动降为 1 列柱 / 0 隙），
//!   柱子按宽度重采样频带，整行铺满、右侧不留空地；
//! - **峰顶标记**：每根柱顶一个随重力下落的小帽（cava 的 peak caps），
//!   用最亮的 `text` 色，是整个画面里唯一的高光点。
//!
//! 颜色与几何分离：[`Palette`] 是纯数据（可离线测试），`render` 只负责
//! 从主题解析出调色板再走同一条渲染路径。

use colorgrad::{Gradient as _, GradientBuilder, LinearGradient};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use super::VisualizerData;
use crate::context::AppContext;
use crate::theme;

/// 渐变锚点（底部 → 顶部）与峰顶标记色。
#[derive(Debug)]
pub struct Palette {
    pub background: Color,
    /// 峰顶小帽：全画面最亮的颜色。
    pub cap: Color,
    /// 纵向线性渐变（自下而上），交给 colorgrad 插值。
    pub gradient: LinearGradient,
}

impl Palette {
    fn from_theme(ctx: &AppContext) -> Self {
        let stops = [
            theme::blue(ctx),
            theme::sapphire(ctx),
            theme::sky(ctx),
            theme::teal(ctx),
            theme::green(ctx),
        ];
        let colors: Vec<colorgrad::Color> = stops
            .iter()
            .map(|color| to_colorgrad_color(*color))
            .collect();
        let fallback = || {
            let only = colors.first().copied().unwrap_or_default();
            GradientBuilder::new()
                .colors(&[only, only])
                .build::<LinearGradient>()
                .expect("two identical colors always build")
        };
        let gradient = GradientBuilder::new()
            .colors(&colors)
            .build::<LinearGradient>()
            .unwrap_or_else(|_| fallback());
        Self {
            background: theme::base(ctx),
            cap: theme::text(ctx),
            gradient,
        }
    }
}

/// 把主题色转换成 colorgrad 颜色；非 Rgb 主题色退化为中性灰蓝。
fn to_colorgrad_color(color: Color) -> colorgrad::Color {
    match color {
        Color::Rgb(r, g, b) => colorgrad::Color::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ),
        _ => colorgrad::Color::new(0.6, 0.65, 0.7, 1.0),
    }
}

/// 渐变在 `fraction`（0..1，自底部向上）处的颜色。
fn level_color(palette: &Palette, fraction: f32) -> Color {
    let rgba = palette.gradient.at(fraction.clamp(0.0, 1.0));
    Color::Rgb(
        (rgba.r * 255.0).round() as u8,
        (rgba.g * 255.0).round() as u8,
        (rgba.b * 255.0).round() as u8,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerStyle {
    Classic,
    Modern,
}

impl VisualizerStyle {
    pub fn from_ctx(ctx: &AppContext) -> Self {
        let value = ctx
            .config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .ui
            .visualizer_style
            .trim()
            .to_ascii_lowercase();
        match value.as_str() {
            "modern" => Self::Modern,
            _ => Self::Classic,
        }
    }
}

/// 柱槽布局：参考 cava 的连续填满风格，避免中间明显的空隙。
/// 窄终端保留 1 列柱；中宽/宽终端直接用 2 列柱并取消间隙，视觉更像
/// 传统 cava 的紧密输出。
fn slot_layout(width: u16) -> (u16, u16) {
    match width {
        0..=15 => (1, 0),
        _ => (2, 0),
    }
}

fn slot_layout_for_style(width: u16, style: VisualizerStyle) -> (u16, u16) {
    match style {
        VisualizerStyle::Classic => slot_layout(width),
        VisualizerStyle::Modern => match width {
            0..=19 => (1, 0),
            _ => (2, 0),
        },
    }
}

pub fn render_data(
    area: Rect,
    buf: &mut Buffer,
    ctx: &AppContext,
    data: &VisualizerData,
    peaks: &[f32],
) {
    let palette = Palette::from_theme(ctx);
    if area.width == 0 || area.height == 0 {
        return;
    }
    render_with_style(
        area,
        buf,
        &data.spectrum,
        peaks,
        &palette,
        VisualizerStyle::from_ctx(ctx),
    );
}

/// 主题无关的渲染主体（[`Palette::from_theme`] 解耦出的可测试部分）。
#[cfg(test)]
pub fn render_with(area: Rect, buf: &mut Buffer, bars: &[f32], peaks: &[f32], palette: &Palette) {
    render_with_style(area, buf, bars, peaks, palette, VisualizerStyle::Classic);
}

fn render_with_style(
    area: Rect,
    buf: &mut Buffer,
    bars: &[f32],
    peaks: &[f32],
    palette: &Palette,
    style: VisualizerStyle,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    paint_background(area, buf, palette.background);
    if bars.is_empty() {
        return;
    }

    let (bar_width, gap_width) = slot_layout_for_style(area.width, style);
    let slot = (bar_width + gap_width).max(1) as usize;
    let visible_bars = ((area.width as usize) / slot).max(1).min(bars.len());
    let half_rows = area.height as usize * 2;

    for bar_index in 0..visible_bars {
        let x0 = area.x + (bar_index * slot) as u16;
        let x1 = (x0 + bar_width).min(area.right());
        // 96 个频带均匀映射到可见柱：每柱取对应区间的最大值（保峰）。
        let band_start = bar_index * bars.len() / visible_bars;
        let band_end = ((bar_index + 1) * bars.len() / visible_bars).max(band_start + 1);
        let level = bars[band_start..band_end.min(bars.len())]
            .iter()
            .fold(0.0f32, |acc, value| acc.max(*value));
        // 柱高换算成"半行"数并向上取整：低电平也至少显示一格。
        let filled = (level.clamp(0.0, 1.0) * half_rows as f32).ceil() as usize;

        for y in 0..area.height {
            let row_from_bottom = (area.height - 1 - y) as usize;
            // 一个终端行由上下两个"半行"组成：下 = 2*row，上 = 2*row+1。
            let bottom_filled = filled > row_from_bottom * 2;
            let top_filled = filled > row_from_bottom * 2 + 1;
            if !bottom_filled && !top_filled {
                continue;
            }
            let bottom_color =
                level_color(palette, (row_from_bottom * 2 + 1) as f32 / half_rows as f32);
            let top_color =
                level_color(palette, (row_from_bottom * 2 + 2) as f32 / half_rows as f32);

            let (glyph, fg) = match (top_filled, bottom_filled) {
                // 整格都是柱：fg 覆盖全格，取上半格颜色即可（同一根柱同色系）。
                (true, true) => ('█', top_color),
                // 只有上半格是柱：下半格露背景。
                (true, false) => ('▀', top_color),
                // 只有下半格是柱：上半格露背景。
                (false, true) => ('▄', bottom_color),
                (false, false) => continue,
            };
            for x in x0..x1 {
                if let Some(cell) = buf.cell_mut((x, area.y + y)) {
                    cell.set_char(glyph);
                    cell.set_fg(fg);
                    cell.set_bg(palette.background);
                    // 频谱是叠加层：底层内容（暗色列、加粗选中行…）的
                    // DIM/BOLD 必须清掉，否则柱子会带着底下的修饰色块，
                    // 既难看又凭空多出一堆 SGR 切换。
                    cell.set_style(Style::new().remove_modifier(Modifier::all()));
                }
            }
        }

        // 峰顶小帽：柱顶上方一格内的亮色半块（cava peak cap）。
        if let Some(cap) = peaks.get(bar_index) {
            let cap_half = (cap.clamp(0.0, 1.0) * half_rows as f32).ceil() as usize;
            // 帽只画在柱体之外，否则会盖住渐变顶端。
            if cap_half > filled && cap_half >= 1 {
                let half_index = cap_half - 1;
                let y = area.y + (half_rows - 1 - half_index) as u16 / 2;
                let upper = half_index % 2 == 1;
                let glyph = if upper { '▀' } else { '▄' };
                for x in x0..x1 {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(glyph);
                        cell.set_fg(palette.cap);
                        cell.set_style(Style::new().remove_modifier(Modifier::all()));
                    }
                }
            }
        }
    }
}

fn paint_background(area: Rect, buf: &mut Buffer, background: Color) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ');
                // `Cell::set_style` 只增删 modifier，不会清空；必须显式移除全部
                // 修饰位，否则会继承底层歌词/表格的 DIM、BOLD。
                cell.set_style(Style::new().bg(background).remove_modifier(Modifier::all()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        let gradient = GradientBuilder::new()
            .colors(&[
                colorgrad::Color::new(0.0, 0.0, 1.0, 1.0),
                colorgrad::Color::new(0.0, 1.0, 0.0, 1.0),
            ])
            .build::<LinearGradient>()
            .unwrap();
        Palette {
            background: Color::Black,
            cap: Color::White,
            gradient,
        }
    }

    fn buffer(width: u16, height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, height))
    }

    #[test]
    fn full_height_bar_fills_whole_column() {
        let mut buf = buffer(6, 2);
        render_with(
            Rect::new(0, 0, 6, 2),
            &mut buf,
            &[1.0, 0.0],
            &[],
            &palette(),
        );
        // 6 列属于 1+1 槽位：第一根柱占第 0 列，第 1 列是间隙。
        assert_eq!(buf[(0u16, 0u16)].symbol(), "█");
        assert_eq!(buf[(0u16, 1u16)].symbol(), "█");
        assert_eq!(buf[(1u16, 1u16)].symbol(), " ");
        // 第二根柱电平为 0，不显示。
        assert_eq!(buf[(2u16, 0u16)].symbol(), " ");
    }

    #[test]
    fn bars_fill_columns_without_visible_gaps() {
        let mut buf = buffer(20, 1);
        render_with(
            Rect::new(0, 0, 20, 1),
            &mut buf,
            &[1.0, 1.0, 1.0, 1.0],
            &[],
            &palette(),
        );
        for x in 0..8u16 {
            assert_eq!(buf[(x, 0u16)].symbol(), "█", "col {x} should be filled in classic cava layout");
        }
        for x in 8..20u16 {
            assert_eq!(buf[(x, 0u16)].symbol(), " ", "trailing cols beyond the compact fill should stay blank");
        }
    }

    #[test]
    fn wide_terminal_uses_compact_two_column_bars() {
        let mut buf = buffer(20, 1);
        assert_eq!(slot_layout(96), (2, 0));
        render_with(
            Rect::new(0, 0, 20, 1),
            &mut buf,
            &[1.0, 1.0, 1.0, 1.0],
            &[],
            &Palette {
                background: Color::Black,
                cap: Color::White,
                gradient: palette().gradient,
            },
        );
        for x in 0..8u16 {
            assert_eq!(buf[(x, 0u16)].symbol(), "█");
        }
        for x in 8..20u16 {
            assert_eq!(buf[(x, 0u16)].symbol(), " ");
        }
    }

    #[test]
    fn gradient_is_continuous_not_hard_banded() {
        let palette = palette();
        // 中点两侧的颜色应介于两个锚点之间（插值生效，而不是硬切色块）。
        let low = level_color(&palette, 0.1);
        let mid = level_color(&palette, 0.5);
        let high = level_color(&palette, 0.9);
        let Color::Rgb(lr, lg, _) = low else {
            panic!("expected rgb")
        };
        let Color::Rgb(_mr, mg, _) = mid else {
            panic!("expected rgb")
        };
        let Color::Rgb(_hr, hg, _) = high else {
            panic!("expected rgb")
        };
        // 蓝→绿渐变：绿分量单调上升、蓝分量单调下降（红恒为 0）。
        assert_eq!(lr, 0);
        assert!(lg < mg && mg < hg, "green channel {lg} {mg} {hg}");
        let Color::Rgb(_, _, lb) = low else {
            panic!("expected rgb")
        };
        let Color::Rgb(_, _, mb) = mid else {
            panic!("expected rgb")
        };
        let Color::Rgb(_, _, hb) = high else {
            panic!("expected rgb")
        };
        assert!(lb > mb && mb > hb, "blue channel {lb} {mb} {hb}");
    }

    #[test]
    fn peak_cap_drawn_above_bar_top() {
        let mut buf = buffer(3, 4);
        // 柱 0.5（8 半行中的 4 格），峰 0.75（第 6 半行）→ 帽悬在柱顶上方。
        render_with(
            Rect::new(0, 0, 3, 4),
            &mut buf,
            &[0.5, 0.0],
            &[0.75, 0.0],
            &palette(),
        );
        // 半行索引 5（0 起）是奇数 → 行 1 的上半格 → '▀'，颜色是帽色（白）。
        assert_eq!(buf[(0u16, 1u16)].symbol(), "▀");
        assert_eq!(buf[(0u16, 1u16)].fg, Color::White);
        // 柱顶本身（半行 3 → 行 2 上半格）保持柱色而不是帽色。
        assert_ne!(buf[(0u16, 2u16)].fg, Color::White);
    }

    #[test]
    fn empty_bars_paint_only_background() {
        let mut buf = buffer(3, 2);
        render_with(Rect::new(0, 0, 3, 2), &mut buf, &[], &[], &palette());
        for y in 0..2u16 {
            for x in 0..3u16 {
                assert_eq!(buf[(x, y)].symbol(), " ");
                assert_eq!(buf[(x, y)].bg, Color::Black);
            }
        }
    }

    /// 频谱是叠加层：底层内容的 DIM/BOLD 不能被柱子继承（既难看又多出 SGR）。
    #[test]
    fn spectrum_strips_modifiers_inherited_from_the_underlying_content() {
        let mut buf = buffer(6, 2);
        for y in 0..2u16 {
            for x in 0..6u16 {
                buf.cell_mut((x, y))
                    .expect("cell in range")
                    .set_style(Style::new().add_modifier(Modifier::DIM | Modifier::BOLD));
            }
        }

        render_with(
            Rect::new(0, 0, 6, 2),
            &mut buf,
            &[1.0, 1.0],
            &[1.0, 1.0],
            &palette(),
        );

        for y in 0..2u16 {
            for x in 0..6u16 {
                assert!(
                    buf[(x, y)].modifier.is_empty(),
                    "cell ({x},{y}) kept {:?}",
                    buf[(x, y)].modifier
                );
            }
        }
    }

    #[test]
    fn render_data_keeps_a_classic_cava_style_without_waveform_or_meter() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 8));
        let palette = palette();

        render_with(
            Rect::new(0, 0, 20, 8),
            &mut buf,
            &vec![0.0; 12],
            &vec![0.0; 12],
            &palette,
        );

        for y in 0..8u16 {
            for x in 0..20u16 {
                assert_ne!(buf[(x, y)].symbol(), "•");
                assert_ne!(buf[(x, y)].symbol(), "R");
                assert_ne!(buf[(x, y)].symbol(), "M");
                assert_ne!(buf[(x, y)].symbol(), "T");
            }
        }
    }

    #[test]
    fn modern_visualizer_style_reduces_gap_between_bars() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        render_with_style(
            Rect::new(0, 0, 12, 1),
            &mut buf,
            &[1.0, 1.0, 1.0],
            &[],
            &palette(),
            VisualizerStyle::Modern,
        );
        assert_eq!(buf[(0u16, 0u16)].symbol(), "█");
        assert_eq!(buf[(1u16, 0u16)].symbol(), "█");
        assert_eq!(buf[(2u16, 0u16)].symbol(), "█");
        assert_eq!(buf[(3u16, 0u16)].symbol(), " ");
    }

    #[test]
    fn narrow_terminal_drops_gaps() {
        assert_eq!(slot_layout(10), (1, 0));
        assert_eq!(slot_layout(32), (2, 0));
        assert_eq!(slot_layout(120), (2, 0));
    }
}
