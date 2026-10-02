//! 封面主色提取：为「界面强调色跟随专辑封面」提供数据。
//!
//! 提取在封面解码线程里做（[`crate::cover::render`] 调 [`dominant_color`]），
//! 结果发布到全局槽 [`publish`]；主题层按当前封面路径查询 [`current`]，
//! 与配置开关一起决定是否把主色柔和混进 accent。按路径匹配是为了防串歌：
//! 提取完成前切了歌，旧结果不会套到新封面上。

use std::sync::Mutex;

use palette::{FromColor, Oklch, Srgb};

/// 缩到该边长再统计像素：64×64 = 4k 像素足以代表整张封面的色彩倾向，
/// 单次提取微秒级，不占解码线程。
const THUMB_EDGE: u32 = 64;
/// Oklch 色度低于该值的像素视为灰色（黑白封面/灰调封面提取不出有意义的 accent）。
const MIN_CHROMA: f32 = 0.04;
/// 明度上下限：过暗（接近黑）与过亮（接近白）的像素不参与主色投票。
const MIN_LIGHTNESS: f32 = 0.2;
const MAX_LIGHTNESS: f32 = 0.92;
/// 色相分桶数（每桶 15°）。
const HUE_BINS: usize = 24;

/// 发布前的明度/色度归一化区间。
///
/// 深色封面的获胜桶平均色明度常在 0.25-0.4：不做归一化时，它与深色主题的
/// accent 混合后仍然偏暗，视觉上几乎无感，还会把选中行文字拉到读不清。
/// clamp 到「鲜艳但不过曝」的区间后再发布，深色封面自动提亮。
const NORMALIZED_LIGHTNESS: (f32, f32) = (0.55, 0.75);
const NORMALIZED_CHROMA: (f32, f32) = (0.10, 0.18);

/// 全局主色槽：`(封面路径, 主色)`。单写单读，锁竞争可忽略。
static COVER_ACCENT: Mutex<Option<(String, Option<[u8; 3]>)>> = Mutex::new(None);

/// 解码线程发布某封面的主色；`accent = None` 表示该封面没有可用主色，
/// 用于覆盖掉同路径可能残留的旧值。
pub fn publish(path: &str, accent: Option<[u8; 3]>) {
    if let Ok(mut slot) = COVER_ACCENT.lock() {
        *slot = Some((path.to_string(), accent));
    }
}

/// 查询当前封面主色；路径不匹配（封面已切换）或无主色时返回 `None`。
pub fn current(path: Option<&str>) -> Option<[u8; 3]> {
    let path = path?;
    let slot = COVER_ACCENT.lock().ok()?;
    let (stored, accent) = slot.as_ref()?;
    if stored == path { *accent } else { None }
}

/// 提取封面主色：按 Oklch 色相分桶投票，权重偏向「饱和、中等明度」的色族，
/// 返回得票最高色族的平均 RGB。
///
/// 返回 `None` 的情况：整张图都是灰调（没有任何像素过饱和门槛）。
pub fn dominant_color(image: &image::DynamicImage) -> Option<[u8; 3]> {
    let thumb = image.thumbnail(THUMB_EDGE, THUMB_EDGE).to_rgba8();

    // 每桶累计：权重和（选主桶用）+ RGB 和（桶内平均色用）+ 像素数。
    let mut weights = [0f32; HUE_BINS];
    let mut sums = [[0f64; 3]; HUE_BINS];
    let mut counts = [0u32; HUE_BINS];

    for pixel in thumb.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 128 {
            continue;
        }
        let srgb = Srgb::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        );
        let oklch = Oklch::from_color(srgb);
        if oklch.chroma < MIN_CHROMA || oklch.l < MIN_LIGHTNESS || oklch.l > MAX_LIGHTNESS {
            continue;
        }
        let hue = oklch.hue.into_positive_degrees();
        let bin = ((hue / 360.0 * HUE_BINS as f32).round() as usize) % HUE_BINS;
        // 色度平方加权：高饱和色族比大面积灰调背景更有资格代表封面。
        weights[bin] += oklch.chroma * oklch.chroma;
        sums[bin][0] += f64::from(r);
        sums[bin][1] += f64::from(g);
        sums[bin][2] += f64::from(b);
        counts[bin] += 1;
    }

    let best = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .filter(|(_, weight)| **weight > 0.0)
        .map(|(index, _)| index)?;
    let count = f64::from(counts[best]);
    Some(normalize([
        (sums[best][0] / count).round().clamp(0.0, 255.0) as u8,
        (sums[best][1] / count).round().clamp(0.0, 255.0) as u8,
        (sums[best][2] / count).round().clamp(0.0, 255.0) as u8,
    ]))
}

/// 把提取出的主色归一化到 [`NORMALIZED_LIGHTNESS`] / [`NORMALIZED_CHROMA`]
/// 区间，保证它混进 accent 后肉眼可见。
fn normalize(rgb: [u8; 3]) -> [u8; 3] {
    let srgb = Srgb::new(
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
    );
    let mut oklch = Oklch::from_color(srgb);
    oklch.l = oklch
        .l
        .clamp(NORMALIZED_LIGHTNESS.0, NORMALIZED_LIGHTNESS.1);
    oklch.chroma = oklch.chroma.clamp(NORMALIZED_CHROMA.0, NORMALIZED_CHROMA.1);
    let vivid = Srgb::from_color(oklch);
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    [
        channel(vivid.red),
        channel(vivid.green),
        channel(vivid.blue),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> image::DynamicImage {
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(width, height, |_, _| {
            image::Rgba([rgb[0], rgb[1], rgb[2], 255])
        }))
    }

    #[test]
    fn solid_red_cover_yields_a_red_dominant_color() {
        let color = dominant_color(&solid(80, 80, [200, 30, 30])).expect("纯红封面应有主色");
        // RGB 通道关系锁定色相（红 >> 绿/蓝），不锁具体值（缩放/色彩空间转换有舍入）。
        assert!(color[0] > color[1] + 60, "R 应显著大于 G: {color:?}");
        assert!(color[0] > color[2] + 60, "R 应显著大于 B: {color:?}");
    }

    #[test]
    fn dark_covers_are_normalized_into_the_vivid_range() {
        // 深红封面：提取结果必须被提亮到目标明度区间，否则与深色主题的
        // accent 混合后肉眼无感（旧版的问题）。
        let color = dominant_color(&solid(80, 80, [80, 12, 12])).expect("深红封面应有主色");
        let srgb = Srgb::new(
            f32::from(color[0]) / 255.0,
            f32::from(color[1]) / 255.0,
            f32::from(color[2]) / 255.0,
        );
        let oklch = Oklch::from_color(srgb);
        assert!(
            (0.55..=0.75).contains(&oklch.l),
            "明度应被归一化到鲜艳区间，实际 l={:.3} color={color:?}",
            oklch.l
        );
        assert!(
            oklch.chroma >= 0.1 - 1e-3,
            "色度不应低于下限，实际 c={:.3}",
            oklch.chroma
        );
        // 仍然保持红色相。
        assert!(color[0] > color[1] + 40, "R 应显著大于 G: {color:?}");
    }

    #[test]
    fn grayscale_cover_has_no_dominant_color() {
        assert!(dominant_color(&solid(80, 80, [128, 128, 128])).is_none());
        assert!(dominant_color(&solid(80, 80, [20, 20, 20])).is_none());
        assert!(dominant_color(&solid(80, 80, [245, 245, 245])).is_none());
    }

    #[test]
    fn publish_and_current_match_by_path() {
        publish("/tmp/a.jpg", Some([10, 20, 30]));
        assert_eq!(current(Some("/tmp/a.jpg")), Some([10, 20, 30]));
        assert_eq!(current(Some("/tmp/b.jpg")), None);
        assert_eq!(current(None), None);
        // 同路径再次发布覆盖旧值；None 也会覆盖（表示该封面无主色）。
        publish("/tmp/a.jpg", None);
        assert_eq!(current(Some("/tmp/a.jpg")), None);
        // 清场，避免影响其它测试。
        publish("/tmp/none.jpg", None);
    }
}
