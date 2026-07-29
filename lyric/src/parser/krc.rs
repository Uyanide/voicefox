//! 酷狗 KRC 明文逐字歌词解析。

pub fn decode_and_parse(data: &[u8]) -> Vec<lx_core::model::lyric::YrcLine> {
    std::str::from_utf8(data)
        .map(super::yrc::parse)
        .unwrap_or_default()
}
