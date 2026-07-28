//! 封面在终端里的实际绘制

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::errors::Errors;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::thread::{ResizeRequest, ResizeResponse, ThreadProtocol};
use ratatui_image::{FilterType, FontSize, Resize, ResizeEncodeRender};

/// 用 Scale 而不是 Fit：Fit 内部是 min(area, image)，只缩不放
const RESIZE: Resize = Resize::Scale(Some(FilterType::Triangle));

/// 解码后的最长边上限
const MAX_DECODED_EDGE: u32 = 1024;

/// 主线程 → 解码线程
struct DecodeJob {
    path: String,
    /// 建 protocol 用，字号变了这里跟着变
    picker: Picker,
    /// 请求序号
    id: u64,
}

/// 后台线程 → 主线程
enum Done {
    Loaded {
        id: u64,
        protocol: Option<Box<StatefulProtocol>>,
    },
    Resized(Box<Result<ResizeResponse, Errors>>),
}

pub struct CoverRenderer {
    picker: Picker,
    enabled: bool,
    /// 解码请求
    decode_tx: Sender<DecodeJob>,
    /// 后台线程的结果
    done_rx: Receiver<Done>,
    /// 图片状态。内部为空表示后台正在算
    protocol: ThreadProtocol,
    /// 当前是否有封面要显示
    has_image: bool,
    /// 已经开始解码的路径
    loaded: Option<String>,
    /// 上一次从 ioctl 读到的单元格尺寸
    probed: Option<FontSize>,
    /// 解码请求序号
    request_id: u64,
}

impl CoverRenderer {
    /// 探测终端图形能力并返回实例
    ///
    /// 过程中会向终端发查询序列并读回复，必须在进入 alt screen 之后、
    /// 开始读终端事件之前调用，否则会和事件循环抢 stdin
    pub fn detect(cover_protocol: &str) -> Self {
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|error| {
            tracing::debug!("query terminal graphics capabilities failed: {error}");
            Picker::halfblocks()
        });
        if let Some(protocol) = parse_protocol(cover_protocol) {
            picker.set_protocol_type(protocol);
        }

        let mut renderer = Self::spawn(picker);
        // 记下 ioctl 的基准读数
        renderer.refresh_font_size();
        tracing::info!(
            "cover protocol {:?}, font size {:?}",
            renderer.picker.protocol_type(),
            renderer.picker.font_size()
        );
        renderer
    }

    fn spawn(picker: Picker) -> Self {
        let (decode_tx, decode_rx) = channel::<DecodeJob>();
        let (encode_tx, encode_rx) = channel::<ResizeRequest>();
        let (done_tx, done_rx) = channel::<Done>();
        let enabled = spawn_workers(decode_rx, encode_rx, done_tx);

        Self {
            protocol: ThreadProtocol::new(encode_tx, None),
            picker,
            enabled,
            decode_tx,
            done_rx,
            has_image: false,
            loaded: None,
            probed: None,
            request_id: 0,
        }
    }

    /// 终端单元格的像素尺寸，供 [`super::CoverGeometry`] 排版
    pub fn font_size(&self) -> FontSize {
        self.picker.font_size()
    }

    /// 把渲染器同步到给定封面路径。只有路径变化时才派活给后台线程
    pub fn sync(&mut self, path: Option<&str>) {
        if !self.enabled {
            return;
        }
        match path {
            None => {
                if self.loaded.is_none() {
                    return;
                }
                self.loaded = None;
                self.request_id += 1;
                self.protocol.empty_protocol();
                self.has_image = false;
            }
            Some(path) => {
                if self.loaded.as_deref() == Some(path) {
                    return;
                }
                self.loaded = Some(path.to_string());
                self.dispatch_decode(path.to_string());
            }
        }
    }

    /// 派一次解码，旧图立刻撤下
    fn dispatch_decode(&mut self, path: String) {
        self.request_id += 1;
        self.protocol.empty_protocol();
        self.has_image = self
            .decode_tx
            .send(DecodeJob {
                path,
                picker: self.picker.clone(),
                id: self.request_id,
            })
            .is_ok();
    }

    /// 重新读终端的单元格像素尺寸，变了就按新尺寸重新解码当前封面
    pub fn refresh_font_size(&mut self) {
        if !self.enabled {
            return;
        }
        let Some(font_size) = probe_font_size() else {
            return;
        };
        match self.probed.replace(font_size) {
            // 读数没变
            Some(previous)
                if previous.width == font_size.width && previous.height == font_size.height =>
            {
                return;
            }
            // 第一次读。保留 Picker 查询的字号(若有)
            // 依赖 capabilities 非空 => 字号非 arbitrary，无文档
            None if !self.picker.capabilities().is_empty() => return,
            _ => {}
        }
        tracing::debug!("cell size is now {font_size:?}");

        // Picker 没有单独改字号的接口，重建一个，把探测到的协议带过去
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(font_size);
        picker.set_protocol_type(self.picker.protocol_type());
        self.picker = picker;

        if let Some(path) = self.loaded.clone() {
            self.dispatch_decode(path);
        }
    }

    /// 收取后台线程算完的结果，返回是否需要重画
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.done_rx.try_recv() {
                Ok(Done::Loaded { id, protocol }) => {
                    // 对不上说明封面换过了
                    if id != self.request_id {
                        continue;
                    }
                    match protocol {
                        Some(protocol) => self.protocol.replace_protocol(*protocol),
                        None => {
                            self.protocol.empty_protocol();
                            self.has_image = false;
                        }
                    }
                    changed = true;
                }
                Ok(Done::Resized(result)) => match *result {
                    // 过期的结果会被 ThreadProtocol 按 id 丢掉
                    Ok(response) => changed |= self.protocol.update_resized_protocol(response),
                    Err(error) => {
                        tracing::debug!("cover encode failed: {error}");
                        self.protocol.empty_protocol();
                        self.has_image = false;
                        changed = true;
                    }
                },
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    /// 画封面，返回 false 表示没画
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) -> bool {
        if !self.has_image || area.width == 0 || area.height == 0 {
            return false;
        }
        if let Some(size) = self.protocol.needs_resize(&RESIZE, area.into()) {
            self.protocol.resize_encode(&RESIZE, size);
        }
        self.protocol.render(area, buf);
        true
    }
}

/// 起解码线程和编码线程
fn spawn_workers(
    decode_rx: Receiver<DecodeJob>,
    encode_rx: Receiver<ResizeRequest>,
    done_tx: Sender<Done>,
) -> bool {
    let decode_tx = done_tx.clone();
    let decode = thread::Builder::new()
        .name("voicefox-cover-decode".to_string())
        .spawn(move || {
            for job in decode_rx {
                let protocol =
                    decode(&job.path).map(|image| Box::new(job.picker.new_resize_protocol(image)));
                if decode_tx
                    .send(Done::Loaded {
                        id: job.id,
                        protocol,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

    let encode = thread::Builder::new()
        .name("voicefox-cover-encode".to_string())
        .spawn(move || {
            for request in encode_rx {
                if done_tx
                    .send(Done::Resized(Box::new(request.resize_encode())))
                    .is_err()
                {
                    break;
                }
            }
        });

    match (decode, encode) {
        (Ok(_), Ok(_)) => true,
        _ => {
            tracing::warn!("spawn cover worker threads failed, cover rendering disabled");
            false
        }
    }
}

/// 从终端窗口的像素尺寸和行列数反推单元格大小
fn probe_font_size() -> Option<FontSize> {
    let size = crossterm::terminal::window_size().ok()?;
    if size.width == 0 || size.height == 0 || size.columns == 0 || size.rows == 0 {
        return None;
    }
    Some(FontSize::new(
        size.width / size.columns,
        size.height / size.rows,
    ))
}

fn decode(path: &str) -> Option<image::DynamicImage> {
    match image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|error| error.to_string())
        .and_then(|reader| reader.decode().map_err(|error| error.to_string()))
    {
        Ok(image) => Some(shrink(image)),
        Err(error) => {
            tracing::debug!("decode cover {path} failed: {error}");
            None
        }
    }
}

/// 把最长边超过 [`MAX_DECODED_EDGE`] 的图按比例缩到上限
fn shrink(image: image::DynamicImage) -> image::DynamicImage {
    if image.width() <= MAX_DECODED_EDGE && image.height() <= MAX_DECODED_EDGE {
        return image;
    }
    image.resize(MAX_DECODED_EDGE, MAX_DECODED_EDGE, FilterType::Triangle)
}

/// 解析配置里的 ui.cover_protocol，None 表示交给探测
fn parse_protocol(value: &str) -> Option<ProtocolType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        "kitty" => Some(ProtocolType::Kitty),
        "sixel" | "sixels" => Some(ProtocolType::Sixel),
        "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
        "halfblocks" | "blocks" => Some(ProtocolType::Halfblocks),
        other => {
            tracing::warn!("unknown ui.cover_protocol {other:?}, falling back to auto");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui_image::picker::{Picker, ProtocolType};

    use super::{CoverRenderer, parse_protocol};

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 8,
        height: 4,
    };

    fn renderer() -> CoverRenderer {
        CoverRenderer::spawn(Picker::halfblocks())
    }

    /// 存一张 16x16 的纯色 PNG，返回路径
    fn write_image(name: &str, color: [u8; 3]) -> String {
        write_sized_image(name, 16, 16, color)
    }

    /// 存一张指定尺寸的纯色 PNG，返回路径
    fn write_sized_image(name: &str, width: u32, height: u32, color: [u8; 3]) -> String {
        let path = std::env::temp_dir().join(format!("voicefox-cover-{name}.png"));
        let pixel = image::Rgb(color);
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, pixel))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        path.to_string_lossy().to_string()
    }

    /// 一直转到后台把解码和编码都干完，返回画出来的 buffer
    fn settle(renderer: &mut CoverRenderer) -> Buffer {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            renderer.poll();
            let mut buf = Buffer::empty(AREA);
            assert!(renderer.render(AREA, &mut buf), "应有封面可以显示");
            if buf != Buffer::empty(AREA) {
                return buf;
            }
            assert!(Instant::now() < deadline, "后台线程应已经完成");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// 画面左上角的颜色
    fn corner(buf: &Buffer) -> (Color, Color) {
        let cell = buf.cell((AREA.x, AREA.y)).unwrap();
        (cell.fg, cell.bg)
    }

    #[test]
    fn cover_is_decoded_off_thread_and_then_rendered() {
        let red = write_image("red", [255, 0, 0]);
        let mut renderer = renderer();

        // 还没有封面
        let mut buf = Buffer::empty(AREA);
        assert!(!renderer.render(AREA, &mut buf));

        renderer.sync(Some(&red));
        // 后台还在解码：框仍然归渲染器管，但这一帧留白
        let mut buf = Buffer::empty(AREA);
        assert!(renderer.render(AREA, &mut buf));
        assert_eq!(buf, Buffer::empty(AREA));

        let buf = settle(&mut renderer);
        assert_eq!(corner(&buf), (Color::Rgb(255, 0, 0), Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn a_cover_swapped_mid_decode_beats_the_stale_one() {
        let red = write_image("stale", [255, 0, 0]);
        let green = write_image("fresh", [0, 255, 0]);
        let mut renderer = renderer();

        // 中间不 poll，红色那次的结果回来时序号已经过期，必须被丢掉
        renderer.sync(Some(&red));
        renderer.sync(Some(&green));

        let buf = settle(&mut renderer);
        assert_eq!(corner(&buf), (Color::Rgb(0, 255, 0), Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn an_undecodable_cover_stops_rendering() {
        let mut renderer = renderer();
        renderer.sync(Some("/voicefox/does/not/exist.png"));

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            renderer.poll();
            let mut buf = Buffer::empty(AREA);
            if !renderer.render(AREA, &mut buf) {
                return;
            }
            assert!(Instant::now() < deadline, "解码失败后应没有封面可以显示");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_oversized_cover_is_shrunk_when_decoded() {
        let huge = write_sized_image("huge", 3000, 1500, [0, 0, 255]);
        let image = super::decode(&huge).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (1024, 512),
            "最长边应缩到上限，比例应保持"
        );

        let small = write_sized_image("small", 300, 150, [0, 0, 255]);
        let image = super::decode(&small).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (300, 150),
            "上限以内的图应原样保留"
        );
    }

    #[test]
    fn protocol_config_is_parsed_leniently() {
        assert_eq!(parse_protocol("auto"), None);
        assert_eq!(parse_protocol(""), None);
        assert_eq!(parse_protocol("  Kitty "), Some(ProtocolType::Kitty));
        assert_eq!(parse_protocol("SIXEL"), Some(ProtocolType::Sixel));
        assert_eq!(parse_protocol("iterm"), Some(ProtocolType::Iterm2));
        assert_eq!(parse_protocol("halfblocks"), Some(ProtocolType::Halfblocks));
        // 非致命错误
        assert_eq!(parse_protocol("kity"), None);
    }
}
