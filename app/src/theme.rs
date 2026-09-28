#![allow(dead_code)]

use lx_core::model::config::ThemeConfig;
use ratatui::style::Color;

use crate::context::AppContext;

const ROSEWATER: Color = Color::Rgb(245, 224, 220);
const FLAMINGO: Color = Color::Rgb(242, 205, 205);
const PINK: Color = Color::Rgb(245, 194, 231);
const MAUVE: Color = Color::Rgb(203, 166, 247);
const RED: Color = Color::Rgb(243, 139, 168);
const MAROON: Color = Color::Rgb(235, 160, 172);
const PEACH: Color = Color::Rgb(250, 179, 135);
const YELLOW: Color = Color::Rgb(249, 226, 175);
const GREEN: Color = Color::Rgb(166, 227, 161);
const TEAL: Color = Color::Rgb(148, 226, 213);
const SKY: Color = Color::Rgb(137, 220, 235);
const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
const BLUE: Color = Color::Rgb(137, 180, 250);
const LAVENDER: Color = Color::Rgb(180, 190, 254);
const TEXT: Color = Color::Rgb(205, 214, 244);
const SUBTEXT_1: Color = Color::Rgb(186, 194, 222);
const SUBTEXT_0: Color = Color::Rgb(166, 173, 200);
const OVERLAY_2: Color = Color::Rgb(147, 153, 178);
const OVERLAY_1: Color = Color::Rgb(127, 132, 156);
const OVERLAY_0: Color = Color::Rgb(108, 112, 134);
const SURFACE_2: Color = Color::Rgb(88, 91, 112);
const SURFACE_1: Color = Color::Rgb(69, 71, 90);
const SURFACE_0: Color = Color::Rgb(49, 50, 68);
const BASE: Color = Color::Rgb(30, 30, 46);
const MANTLE: Color = Color::Rgb(24, 24, 37);
const CRUST: Color = Color::Rgb(17, 17, 27);

pub fn accent(ctx: &AppContext) -> Color {
    configured(ctx, |theme| &theme.accent, MAUVE)
}

pub fn border(ctx: &AppContext) -> Color {
    configured(ctx, |theme| &theme.border, SURFACE_2)
}

pub fn text(ctx: &AppContext) -> Color {
    configured(ctx, |theme| &theme.text, TEXT)
}

pub fn muted(ctx: &AppContext) -> Color {
    configured(ctx, |theme| &theme.muted, SUBTEXT_0)
}

macro_rules! palette_color {
    ($name:ident, $field:ident, $fallback:ident) => {
        pub fn $name(ctx: &AppContext) -> Color {
            configured(ctx, |theme| &theme.$field, $fallback)
        }
    };
}

palette_color!(rosewater, rosewater, ROSEWATER);
palette_color!(flamingo, flamingo, FLAMINGO);
palette_color!(pink, pink, PINK);
palette_color!(mauve, mauve, MAUVE);
palette_color!(red, red, RED);
palette_color!(maroon, maroon, MAROON);
palette_color!(peach, peach, PEACH);
palette_color!(yellow, yellow, YELLOW);
palette_color!(green, green, GREEN);
palette_color!(teal, teal, TEAL);
palette_color!(sky, sky, SKY);
palette_color!(sapphire, sapphire, SAPPHIRE);
palette_color!(blue, blue, BLUE);
palette_color!(lavender, lavender, LAVENDER);
palette_color!(subtext1, subtext_1, SUBTEXT_1);
palette_color!(subtext0, subtext_0, SUBTEXT_0);
palette_color!(overlay2, overlay_2, OVERLAY_2);
palette_color!(overlay1, overlay_1, OVERLAY_1);
palette_color!(overlay0, overlay_0, OVERLAY_0);
palette_color!(surface2, surface_2, SURFACE_2);
palette_color!(surface1, surface_1, SURFACE_1);
palette_color!(surface0, surface_0, SURFACE_0);
palette_color!(base, base, BASE);
palette_color!(mantle, mantle, MANTLE);
palette_color!(crust, crust, CRUST);

pub fn selection_fg(ctx: &AppContext) -> Color {
    crust(ctx)
}

fn configured(ctx: &AppContext, value: fn(&ThemeConfig) -> &String, fallback: Color) -> Color {
    let config = ctx.config.read().unwrap_or_else(|e| e.into_inner());
    parse(value(&config.theme), fallback)
}

fn parse(value: &str, fallback: Color) -> Color {
    parse_value(value).unwrap_or(fallback)
}

/// 解析一个主题色值；无法识别时返回 `None`。
///
/// 支持四种写法：
///
/// - **跟随终端**：`default` / `reset` / `terminal` / `none` / `transparent` / `-`
///   都映射到 `Color::Reset`，也就是终端自己的默认前景/背景色。把 `base` 设成它
///   就能透出终端主题（配合透明终端 / 壁纸）；把 `text` 设成它就用终端的前景色。
/// - 16 个 ANSI 名字：`black`…`white`，另接受 `light_*` / `bright_*` 与
///   `dark_gray` 的几种写法。
/// - `#rgb` 与 `#rrggbb`。
/// - ANSI 256 色号：`236` 或 `color236`（引用终端调色板，做"半透明感"常用）。
pub fn parse_value(value: &str) -> Option<Color> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        // 空值表示"没配"，交给调用方的默认色。
        return None;
    }
    match value.as_str() {
        "default" | "reset" | "terminal" | "none" | "transparent" | "-" => {
            return Some(Color::Reset);
        }
        "black" => return Some(Color::Black),
        "red" => return Some(Color::Red),
        "green" => return Some(Color::Green),
        "yellow" => return Some(Color::Yellow),
        "blue" => return Some(Color::Blue),
        "magenta" => return Some(Color::Magenta),
        "cyan" => return Some(Color::Cyan),
        "gray" | "grey" => return Some(Color::Gray),
        "dark_gray" | "dark-grey" | "darkgray" | "dark_grey" => return Some(Color::DarkGray),
        "white" => return Some(Color::White),
        "light_red" | "light-red" | "lightred" | "bright_red" | "bright-red" => {
            return Some(Color::LightRed);
        }
        "light_green" | "light-green" | "lightgreen" | "bright_green" | "bright-green" => {
            return Some(Color::LightGreen);
        }
        "light_yellow" | "light-yellow" | "lightyellow" | "bright_yellow" | "bright-yellow" => {
            return Some(Color::LightYellow);
        }
        "light_blue" | "light-blue" | "lightblue" | "bright_blue" | "bright-blue" => {
            return Some(Color::LightBlue);
        }
        "light_magenta" | "light-magenta" | "lightmagenta" | "bright_magenta" => {
            return Some(Color::LightMagenta);
        }
        "light_cyan" | "light-cyan" | "lightcyan" | "bright_cyan" | "bright-cyan" => {
            return Some(Color::LightCyan);
        }
        "light_white" | "light-white" | "lightwhite" | "bright_white" => {
            return Some(Color::White);
        }
        _ => {}
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(index) = value
        .strip_prefix("color")
        .and_then(|rest| rest.parse::<u8>().ok())
    {
        return Some(Color::Indexed(index));
    }
    value.parse::<u8>().ok().map(Color::Indexed)
}

/// 解析 `#` 之后的十六进制部分；`3` 位缩写与 `6` 位写法都支持。
///
/// 这里**必须先确认整串都是 ASCII 十六进制字符再切**：以前直接判 `value.len() == 7`
/// （字节长度）就切片，`"#abc晴"` 这种值字节长正好是 7，`&value[3..5]` 会切在汉字
/// 中间，在非字符边界上 panic —— 而主题色每帧都会解析，等于一启动就崩。
fn parse_hex(hex: &str) -> Option<Color> {
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => {
            let channel = |index: usize| {
                let digit = hex.as_bytes()[index] as char;
                u8::from_str_radix(&format!("{digit}{digit}"), 16).ok()
            };
            Some(Color::Rgb(channel(0)?, channel(1)?, channel(2)?))
        }
        6 => {
            let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(red, green, blue))
        }
        _ => None,
    }
}

/// 在配置加载后调用一次：把识别不了的主题色值报出来。
///
/// `parse_value` 认不出来时会静默回退到默认色，用户会以为"改了配置却没生效"，
/// 所以在加载阶段集中提示一次（渲染路径每帧都会解析颜色，不能在那里打日志）。
pub fn warn_unrecognized(theme: &ThemeConfig) {
    let fields: [(&str, &String); 29] = [
        ("accent", &theme.accent),
        ("text", &theme.text),
        ("muted", &theme.muted),
        ("border", &theme.border),
        ("rosewater", &theme.rosewater),
        ("flamingo", &theme.flamingo),
        ("pink", &theme.pink),
        ("mauve", &theme.mauve),
        ("red", &theme.red),
        ("maroon", &theme.maroon),
        ("peach", &theme.peach),
        ("yellow", &theme.yellow),
        ("green", &theme.green),
        ("teal", &theme.teal),
        ("sky", &theme.sky),
        ("sapphire", &theme.sapphire),
        ("blue", &theme.blue),
        ("lavender", &theme.lavender),
        ("subtext_1", &theme.subtext_1),
        ("subtext_0", &theme.subtext_0),
        ("overlay_2", &theme.overlay_2),
        ("overlay_1", &theme.overlay_1),
        ("overlay_0", &theme.overlay_0),
        ("surface_2", &theme.surface_2),
        ("surface_1", &theme.surface_1),
        ("surface_0", &theme.surface_0),
        ("base", &theme.base),
        ("mantle", &theme.mantle),
        ("crust", &theme.crust),
    ];
    for (name, value) in fields {
        if parse_value(value).is_none() && !value.trim().is_empty() {
            tracing::warn!(
                "主题色 theme.{name} = {value:?} 无法识别，已回退到默认色；\
                 可用 #rgb / #rrggbb、ANSI 名字（red/light_blue/…）、256 色号（236 / color236），\
                 或 default（跟随终端）"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_value};
    use lx_core::model::config::ThemeConfig;
    use ratatui::style::Color;

    #[test]
    fn parses_hex_color() {
        assert_eq!(parse("#12aBcD", Color::Black), Color::Rgb(0x12, 0xab, 0xcd));
    }

    #[test]
    fn non_ascii_hex_falls_back_instead_of_panicking() {
        // 修复前："#abc晴" 字节长度正好是 7，会走进 &value[3..5] 这类汉字中间的
        // 切片并在非字符边界 panic；主题色每帧都解析，等于一启动就崩。
        assert_eq!(parse("#abc晴", Color::Red), Color::Red);
        assert_eq!(parse("#日日", Color::Red), Color::Red);
        // 位数不对的也一律回退。
        assert_eq!(parse("#12345", Color::Red), Color::Red);
        assert_eq!(parse("#1234567", Color::Red), Color::Red);
    }

    #[test]
    fn three_digit_hex_is_expanded() {
        assert_eq!(parse_value("#abc"), Some(Color::Rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_value("#F0A"), Some(Color::Rgb(0xff, 0x00, 0xaa)));
    }

    #[test]
    fn default_tokens_follow_the_terminal() {
        for token in [
            "default",
            "reset",
            "terminal",
            "none",
            "transparent",
            "-",
            "  DEFAULT  ",
        ] {
            assert_eq!(parse_value(token), Some(Color::Reset), "{token}");
        }
    }

    #[test]
    fn ansi_names_and_256_indexes_are_supported() {
        assert_eq!(parse_value("light_blue"), Some(Color::LightBlue));
        assert_eq!(parse_value("bright-blue"), Some(Color::LightBlue));
        assert_eq!(parse_value("dark_gray"), Some(Color::DarkGray));
        assert_eq!(parse_value("236"), Some(Color::Indexed(236)));
        assert_eq!(parse_value("color236"), Some(Color::Indexed(236)));
    }

    #[test]
    fn empty_and_unknown_values_are_reported_as_none() {
        assert_eq!(parse_value(""), None);
        assert_eq!(parse_value("   "), None);
        assert_eq!(parse_value("greyish"), None);
        assert_eq!(parse_value("color999"), None);
        assert_eq!(parse_value("300"), None);
    }

    #[test]
    fn base_can_follow_the_terminal() {
        // 方案 A：默认配色不变，但 base 显式设成 default 时必须真的透明。
        let theme = ThemeConfig {
            base: "default".to_string(),
            ..ThemeConfig::default()
        };
        assert_eq!(parse(&theme.base, super::BASE), Color::Reset);
        // 没配的字段仍然用 Mocha 默认值。
        assert_eq!(theme.accent, "#cba6f7");
    }
}
