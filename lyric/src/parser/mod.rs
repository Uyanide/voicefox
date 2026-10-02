//! 歌词格式解析器

use std::sync::OnceLock;

use lx_core::model::lyric::YrcLine;
use regex::Regex;

pub mod lrc; // 标准 LRC
pub mod qrc; // QQ音乐 QRC
pub mod yrc; // 网易云 YRC

/// 自动识别 YRC、QRC 和 lx-music 的统一逐字格式。
pub fn parse_karaoke(content: &str) -> Vec<YrcLine> {
    let content = normalize_karaoke_text(content);
    let yrc = yrc::parse(&content);
    let qrc = qrc::parse(&content);
    let yrc_words = yrc.iter().map(|line| line.words.len()).sum::<usize>();
    let qrc_words = qrc.iter().map(|line| line.words.len()).sum::<usize>();
    if qrc_words > yrc_words { qrc } else { yrc }
}

/// 把逐字歌词统一成「一行一句」的裸文本，供各解析器共享。
///
/// 音源可能返回三种变体，解析前都要抹平：
/// - QRC 的 XML 包装 `LyricContent="..."`（含实体转义）
/// - 把换行写成字面量 `\n` / `&#10;` 的转义文本
/// - `|` 分隔的片段（KRC 风格）
pub fn normalize_karaoke_text(content: &str) -> String {
    let mut text = content.to_string();
    if let Some((_, rest)) = text.split_once("LyricContent=\"") {
        let body = rest.split_once("\"/>").map_or(rest, |(body, _)| body);
        text = body.to_string();
    }
    text.replace("&#10;", "\n")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("\\r\\n", "\n")
        .replace("\\n", "\n")
        .replace("\\r", "\n")
        .replace('|', "\n")
        .lines()
        // XML 包装的收尾残留（`"/>` 或游离引号）不属于歌词正文
        .map(|line| {
            line.trim()
                .trim_end_matches(['"', '\'', '/', '>', '\\'])
                .trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 文本里是否带逐字（QRC/YRC/增强 LRC）时间标记。
///
/// 用于兜底判断：带这类标记的内容即使解析不出逐字歌词，也绝不能按
/// 纯文本歌词原样显示，否则用户会看到 `[0,4810]<0,210,0>You ...`
/// 这样的时间轴源码。
pub fn looks_like_karaoke(content: &str) -> bool {
    static ANGLE: OnceLock<Regex> = OnceLock::new();
    static BRACKET: OnceLock<Regex> = OnceLock::new();
    // `[12,340]` 这类标记不会与 LRC 的 `[mm:ss.xx]` 混淆
    static NUMERIC: OnceLock<Regex> = OnceLock::new();
    // 增强 LRC 的 `<mm:ss.xx>` 需要与 `<0,210,0>` 一起覆盖
    static CLOCK: OnceLock<Regex> = OnceLock::new();

    if content.contains("LyricContent=") {
        return true;
    }
    ANGLE
        .get_or_init(|| Regex::new(r"<\s*\d+\s*,\s*\d+").expect("valid karaoke angle regex"))
        .is_match(content)
        || BRACKET
            .get_or_init(|| Regex::new(r"\(\s*\d+\s*,\s*\d+").expect("valid karaoke paren regex"))
            .is_match(content)
        || NUMERIC
            .get_or_init(|| {
                Regex::new(r"\[\s*\d+\s*,\s*\d+\s*\]").expect("valid karaoke line regex")
            })
            .is_match(content)
        || CLOCK
            .get_or_init(|| {
                Regex::new(r"<\d{1,3}:\d{1,2}(?:[.:,]\d{1,3})?>").expect("valid clock tag regex")
            })
            .is_match(content)
}

#[cfg(test)]
mod tests {
    use super::{looks_like_karaoke, parse_karaoke};

    #[test]
    fn detects_karaoke_markup_in_every_known_flavour() {
        assert!(looks_like_karaoke("[0,4810]<0,210,0>You <220,350,0>ready"));
        assert!(looks_like_karaoke(
            "[3380,3388](3380,847,0)词(4227,847,0)许"
        ));
        assert!(looks_like_karaoke("LyricContent=\"[0,4810]<0,210,0>You\""));
        assert!(looks_like_karaoke("[00:01.00]<00:01.00>你<00:01.50>好"));
        assert!(looks_like_karaoke("[21456,2914]<0,146,0>牵<146,135,0>着"));
    }

    #[test]
    fn plain_lyrics_are_not_mistaken_for_karaoke() {
        assert!(!looks_like_karaoke("第一行歌词\n第二行歌词"));
        assert!(!looks_like_karaoke("[00:12.34]第一行\n[00:45.67]第二行"));
        assert!(!looks_like_karaoke("[offset:+500]\n[00:01.00]第一行"));
        assert!(!looks_like_karaoke(""));
    }

    #[test]
    fn parses_karaoke_sent_in_the_plain_lyric_field() {
        // QRC 常常连 XML 包装和 `\n` 转义一起塞进 `lyric` 字段
        let raw = "LyricContent=\"[0,4810]<0,210,0>You <220,350,0>ready\\n\
[4980,3190]<0,470,0>C'mon <2880,310,0>Yo\"";
        let parsed = parse_karaoke(raw);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].words[0].text, "You ");
        assert_eq!(parsed[0].words[0].start, 0);
        assert_eq!(
            parsed[1]
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<String>(),
            "C'mon Yo"
        );
    }
}
