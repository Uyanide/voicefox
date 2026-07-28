//! 封面在终端里的实际绘制

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FilterType, FontSize, Resize, ResizeEncodeRender};

/// 用 Scale 而不是 Fit：Fit 内部是 min(area, image)，只缩不放。
const RESIZE: Resize = Resize::Scale(Some(FilterType::Triangle));

pub struct CoverRenderer {
    picker: Picker,
    /// cover_protocol = "off" 时关掉，全部退回文字占位
    enabled: bool,
    protocol: Option<StatefulProtocol>,
    /// 已经建立 protocol 的封面路径，路径不变就不重新解码
    loaded: Option<String>,
}

impl CoverRenderer {
    /// 探测终端图形能力并返回实例。
    ///
    /// 过程中会向终端发查询序列并读回复，必须在进入 alt screen 之后、
    /// 开始读终端事件之前调用，否则会和事件循环抢 stdin。
    pub fn detect(cover_protocol: &str) -> Self {
        let forced = parse_protocol(cover_protocol);
        let enabled = forced.is_some();

        let picker = if enabled {
            let mut picker = Picker::from_query_stdio().unwrap_or_else(|error| {
                tracing::debug!("query terminal graphics capabilities failed: {error}");
                Picker::halfblocks()
            });
            if let Some(Some(protocol)) = forced {
                picker.set_protocol_type(protocol);
            }
            picker
        } else {
            Picker::halfblocks()
        };

        tracing::info!(
            "cover protocol {:?}, font size {:?}, enabled {}",
            picker.protocol_type(),
            picker.font_size(),
            enabled
        );
        Self {
            picker,
            enabled,
            protocol: None,
            loaded: None,
        }
    }

    /// 终端单元格的像素尺寸，供 [`super::CoverGeometry`] 排版
    pub fn font_size(&self) -> FontSize {
        self.picker.font_size()
    }

    /// 把渲染器同步到给定封面路径。只有路径变化时才解码，失败也记下来，避免每帧重试。
    pub fn sync(&mut self, path: Option<&str>) {
        if !self.enabled {
            return;
        }
        match path {
            None => {
                self.protocol = None;
                self.loaded = None;
            }
            Some(path) => {
                if self.loaded.as_deref() == Some(path) {
                    return;
                }
                self.loaded = Some(path.to_string());
                self.protocol = decode(path).map(|image| self.picker.new_resize_protocol(image));
            }
        }
    }

    /// 画封面，返回 false 表示没画（调用方应该退回文字占位）
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> bool {
        if area.width == 0 || area.height == 0 {
            return false;
        }
        let Some(protocol) = self.protocol.as_mut() else {
            return false;
        };

        if let Some(size) = protocol.needs_resize(&RESIZE, area.into()) {
            protocol.resize_encode(&RESIZE, size);
        }
        if let Some(Err(error)) = protocol.last_encoding_result() {
            tracing::debug!("cover encode failed: {error}");
            // 丢掉这张图，下一帧起走文字占位，不再反复尝试
            self.protocol = None;
            return false;
        }

        protocol.render(area, buf);
        true
    }
}

fn decode(path: &str) -> Option<image::DynamicImage> {
    match image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
    {
        Ok(image) => Some(image),
        Err(error) => {
            tracing::debug!("decode cover {path} failed: {error}");
            None
        }
    }
}

/// 解析配置里的 ui.cover_protocol。
///
/// Some(None) 表示 auto（交给探测），Some(Some(_)) 表示强制指定，None 表示关闭。
fn parse_protocol(value: &str) -> Option<Option<ProtocolType>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => Some(None),
        "kitty" => Some(Some(ProtocolType::Kitty)),
        "sixel" | "sixels" => Some(Some(ProtocolType::Sixel)),
        "iterm2" | "iterm" => Some(Some(ProtocolType::Iterm2)),
        "halfblocks" | "blocks" => Some(Some(ProtocolType::Halfblocks)),
        "off" | "none" | "text" => None,
        other => {
            tracing::warn!("unknown ui.cover_protocol {other:?}, falling back to auto");
            Some(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_protocol;
    use ratatui_image::picker::ProtocolType;

    #[test]
    fn protocol_config_is_parsed_leniently() {
        assert_eq!(parse_protocol("auto"), Some(None));
        assert_eq!(parse_protocol(""), Some(None));
        assert_eq!(parse_protocol("  Kitty "), Some(Some(ProtocolType::Kitty)));
        assert_eq!(parse_protocol("SIXEL"), Some(Some(ProtocolType::Sixel)));
        assert_eq!(parse_protocol("iterm"), Some(Some(ProtocolType::Iterm2)));
        assert_eq!(
            parse_protocol("halfblocks"),
            Some(Some(ProtocolType::Halfblocks))
        );
        assert_eq!(parse_protocol("off"), None);
        // 非致命错误
        assert_eq!(parse_protocol("kity"), Some(None));
    }
}
