use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use ratatui::layout::Rect;
use reqwest::header::{ACCEPT, REFERER};

const VOICEFOX_KITTY_IMAGE_ID: u32 = 0x56_46_58;

/// 封面宽高比的合理范围
const MIN_IMAGE_ASPECT: f32 = 0.2;
const MAX_IMAGE_ASPECT: f32 = 5.0;
/// 回退方图
const DEFAULT_IMAGE_ASPECT: f32 = 1.0;

/// 终端单元格的高宽比回退值
const DEFAULT_CELL_ASPECT: f32 = 2.0;

/// 封面状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverState {
    Empty,
    Loading,
    Ready,
    Unavailable(String),
}

/// 已就绪的封面
#[derive(Debug, Clone)]
struct CoverImage {
    path: String,
    /// 像素 宽/高
    aspect: f32,
}

pub struct CoverService {
    client: reqwest::Client,
    image: RwLock<Option<CoverImage>>,
    state: RwLock<CoverState>,
    request_id: AtomicU64,
    /// 封面版本号，每次加载新封面时递增。display_kitty 只在新版本时传输图片。
    display_gen: AtomicU64,
    /// 已显示到终端的版本号
    displayed_gen: AtomicU64,
    /// 上次显示封面时使用的终端区域，区域变化（如窗口缩放）时需要重新传输
    displayed_area: RwLock<Option<Rect>>,
    /// Kitty 图层中当前是否有 voicefox 封面
    image_visible: AtomicBool,
    /// 最近一帧封面的目标区域（封面框的 inner），由 set_display_area 记录
    display_area: RwLock<Rect>,
}

impl CoverService {
    pub fn new(proxy_url: &str, timeout_secs: u64) -> Self {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs.clamp(1, 300)))
            .user_agent("voicefox/0.1");
        if !proxy_url.trim().is_empty()
            && let Ok(proxy) = reqwest::Proxy::all(proxy_url.trim())
        {
            builder = builder.proxy(proxy);
        }
        let client = builder.build().unwrap_or_default();
        Self {
            client,
            image: RwLock::new(None),
            state: RwLock::new(CoverState::Empty),
            request_id: AtomicU64::new(0),
            display_gen: AtomicU64::new(0),
            displayed_gen: AtomicU64::new(0),
            displayed_area: RwLock::new(None),
            image_visible: AtomicBool::new(false),
            display_area: RwLock::new(Rect::ZERO),
        }
    }

    pub fn clear(&self) {
        self.request_id.fetch_add(1, Ordering::SeqCst);
        *self.image.write().unwrap() = None;
        *self.state.write().unwrap() = CoverState::Empty;
        self.clear_display();
    }

    fn delete_kitty_image(&self) {
        let mut stdout = std::io::stdout();
        if is_tmux_session() {
            let command = format!("a=d,d=I,i={VOICEFOX_KITTY_IMAGE_ID},q=2");
            let _ = write_kitty_apc(&mut stdout, command.as_bytes(), true);
        } else {
            // viuer 不暴露 Kitty 图片 ID，非 tmux 环境只能清除当前终端图片图层。
            let _ = write_kitty_apc(&mut stdout, b"a=d", false);
        }
        let _ = stdout.flush();
    }

    pub fn has_image(&self) -> bool {
        self.image.read().unwrap().is_some()
    }

    /// 当前封面的像素宽高比
    pub fn image_aspect(&self) -> f32 {
        self.image
            .read()
            .unwrap()
            .as_ref()
            .map_or(DEFAULT_IMAGE_ASPECT, |image| image.aspect)
    }

    pub fn state(&self) -> CoverState {
        self.state.read().unwrap().clone()
    }

    pub async fn load(&self, url: Option<String>) -> Result<(), String> {
        let request_id = self.request_id.fetch_add(1, Ordering::SeqCst) + 1;
        *self.image.write().unwrap() = None;
        self.clear_display();

        let Some(url) = url
            .map(|url| normalize_url(&url))
            .filter(|url| !url.trim().is_empty())
        else {
            *self.state.write().unwrap() =
                CoverState::Unavailable("当前音源没有返回封面".to_string());
            return Ok(());
        };
        *self.state.write().unwrap() = CoverState::Loading;

        let mut last_error = "封面请求失败".to_string();
        let mut result: Option<CoverImage> = None;

        for attempt in 0..3 {
            if self.request_id.load(Ordering::SeqCst) != request_id {
                return Ok(());
            }
            match self.download_and_cache(&url).await {
                Ok(image) => {
                    result = Some(image);
                    break;
                }
                Err(error) => {
                    last_error = error;
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(150 * (attempt + 1))).await;
                    }
                }
            }
        }

        if self.request_id.load(Ordering::SeqCst) == request_id {
            if result.is_some() {
                *self.image.write().unwrap() = result.clone();
                *self.state.write().unwrap() = CoverState::Ready;
                self.display_gen.fetch_add(1, Ordering::SeqCst);
            } else {
                *self.state.write().unwrap() = CoverState::Unavailable(last_error.clone());
            }
        }

        match result {
            Some(_) => Ok(()),
            None => Err(last_error),
        }
    }

    /// 下载封面到本地缓存，返回缓存路径与像素宽高比
    async fn download_and_cache(&self, url: &str) -> Result<CoverImage, String> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("voicefox")
            .join("covers");

        if !cache_dir.exists() {
            let _ = std::fs::create_dir_all(&cache_dir);
        }

        // 本地文件直接返回路径
        if url.starts_with('/') || url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap_or(url);
            if !std::path::Path::new(path).exists() {
                return Err("封面文件不存在".to_string());
            }
            return Ok(CoverImage {
                path: path.to_string(),
                aspect: probe_aspect(path).await.unwrap_or(DEFAULT_IMAGE_ASPECT),
            });
        }

        // 远程文件：下载到缓存
        let hash = simple_hash(url.as_bytes());
        let cache_path = cache_dir.join(format!("{}.jpg", hash));

        if cache_path.exists() {
            return Ok(CoverImage {
                path: cache_path.to_string_lossy().to_string(),
                aspect: probe_aspect(&cache_path)
                    .await
                    .unwrap_or(DEFAULT_IMAGE_ASPECT),
            });
        }

        // HTTP 下载
        let mut request = self
            .client
            .get(url)
            .header(ACCEPT, "image/avif,image/webp,image/apng,image/*,*/*;q=0.8");
        if let Some(referer) = cover_referer(url) {
            request = request.header(REFERER, referer);
        }
        let bytes = request
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .bytes()
            .await
            .map_err(|error| error.to_string())?;

        let cache_path_clone = cache_path.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::write(&cache_path_clone, &bytes).ok();
        })
        .await
        .ok();

        Ok(CoverImage {
            path: cache_path.to_string_lossy().to_string(),
            aspect: probe_aspect(&cache_path)
                .await
                .unwrap_or(DEFAULT_IMAGE_ASPECT),
        })
    }

    /// 当前终端是否支持用 Kitty 图形协议显示封面
    pub fn kitty_available(&self) -> bool {
        // tmux 里 TERM 是 screen/tmux，环境探测必然失败，改由 kitten icat 透传承担
        is_tmux_session()
            || supports_kitty_graphics() && viuer::get_kitty_support() != viuer::KittySupport::None
    }

    /// 记录本帧封面图片应占据的区域（TUI 中该区域留空，Kitty 图片浮动在上方）。
    /// 必须传入封面框的 inner，否则图片会盖住边框。
    pub fn set_display_area(&self, area: Rect) {
        *self.display_area.write().unwrap() = area;
    }

    /// 在终端中使用 Kitty 协议显示封面（必须在 terminal.draw() 之后调用）
    pub fn display_kitty(&self) {
        let area = *self.display_area.read().unwrap();
        if area.width == 0 || area.height == 0 {
            // 本帧没有封面位置（窗口过窄、或没画封面框），移除已经显示的图片
            self.clear_display();
            return;
        }
        let current_gen = self.display_gen.load(Ordering::SeqCst);
        if current_gen == 0 || !self.kitty_available() {
            return;
        }
        // 同一张图且终端区域未变化时不重复传输。
        if current_gen == self.displayed_gen.load(Ordering::SeqCst)
            && self.displayed_area.read().unwrap().as_ref() == Some(&area)
        {
            return;
        }

        let path = match self.image.read().unwrap().as_ref() {
            Some(image) => image.path.clone(),
            None => return,
        };

        if !std::path::Path::new(&path).exists() {
            return;
        }

        // 先清除旧图片，避免新旧图层重叠
        self.clear_display();

        let displayed = if is_tmux_session() {
            display_with_kitten(&path, area)
        } else {
            // 普通终端继续使用 viuer 的 Kitty 协议支持。
            let config = viuer::Config {
                x: area.x,
                y: area.y as i16,
                width: Some(area.width as u32),
                height: Some(area.height as u32),
                restore_cursor: true,
                use_iterm: false,
                ..Default::default()
            };
            viuer::print_from_file(&path, &config).is_ok()
        };

        if displayed {
            let _ = std::io::stdout().flush();
            self.image_visible.store(true, Ordering::SeqCst);
            self.displayed_gen.store(current_gen, Ordering::SeqCst);
            *self.displayed_area.write().unwrap() = Some(area);
        }
    }

    /// 清除终端中的封面图片
    pub fn clear_display(&self) {
        self.displayed_gen.store(0, Ordering::SeqCst);
        *self.displayed_area.write().unwrap() = None;
        if self.image_visible.swap(false, Ordering::SeqCst) {
            self.delete_kitty_image();
        }
    }
}

/// 读图片文件头取像素宽高比，错误返回 None
fn probe_aspect_blocking(path: &std::path::Path) -> Option<f32> {
    let (width, height) = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(width as f32 / height as f32)
}

async fn probe_aspect(path: impl AsRef<std::path::Path>) -> Option<f32> {
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || probe_aspect_blocking(&path))
        .await
        .ok()
        .flatten()
}

/// 终端单元格的高宽比。失败返回回退值
fn cell_aspect() -> f32 {
    let Some((columns, rows, width, height)) = terminal_window_size() else {
        return DEFAULT_CELL_ASPECT;
    };
    if columns == 0 || rows == 0 {
        return DEFAULT_CELL_ASPECT;
    }
    let cell_width = f32::from(width) / f32::from(columns);
    let cell_height = f32::from(height) / f32::from(rows);
    cell_height / cell_width
}

/// 封面排版参数
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverGeometry {
    /// 终端单元格的 高/宽
    cell_aspect: f32,
    /// 封面像素的 宽/高
    image_aspect: f32,
}

impl CoverGeometry {
    /// 读当前终端和封面服务的实际参数
    pub fn detect(cover: &CoverService) -> Self {
        Self::new(cell_aspect(), cover.image_aspect())
    }

    /// 拒绝 NaN、无穷、非正数等极端值
    fn sanitize(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
        if value.is_finite() && value > 0.0 {
            value.clamp(min, max)
        } else {
            fallback
        }
    }

    pub fn new(cell_aspect: f32, image_aspect: f32) -> Self {
        Self {
            cell_aspect: cell_aspect,
            image_aspect: CoverGeometry::sanitize(
                image_aspect,
                MIN_IMAGE_ASPECT,
                MAX_IMAGE_ASPECT,
                DEFAULT_IMAGE_ASPECT,
            ),
        }
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

    /// 封面框应有的高度（含边框），返回 0 表示放不下
    pub fn box_height(self, box_width: u16, max_height: u16) -> u16 {
        let inner_width = box_width.saturating_sub(2);
        if inner_width == 0 || max_height < 3 {
            return 0;
        }
        self.rows_for(inner_width).saturating_add(2).min(max_height)
    }

    /// 封面图片在框内 inner 区域中的实际位置。
    /// - 高度充足时按宽度铺满、垂直居中
    /// - 高度不够时压缩高度、水平居中、两侧留边
    /// - 返回的矩形永远落在 inner 之内
    pub fn image_rect(self, inner: Rect) -> Rect {
        if inner.width == 0 || inner.height == 0 {
            return Rect::ZERO;
        }
        let fit_height = self.rows_for(inner.width);
        if fit_height <= inner.height {
            let y = inner.y + (inner.height - fit_height) / 2;
            return Rect::new(inner.x, y, inner.width, fit_height);
        }
        let fit_width = self.columns_for(inner.height).min(inner.width);
        let x = inner.x + (inner.width - fit_width) / 2;
        Rect::new(x, inner.y, fit_width, inner.height)
    }
}

fn display_with_kitten(path: &str, area: Rect) -> bool {
    let args = kitten_icat_args(path, area, terminal_window_size());
    for (program, prefix) in [("kitten", &[][..]), ("kitty", &["+kitten"][..])] {
        let result = Command::new(program)
            .args(prefix)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null())
            .status();
        match result {
            Ok(status) => return status.success(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return false,
        }
    }
    false
}

fn kitten_icat_args(
    path: &str,
    area: Rect,
    window_size: Option<(u16, u16, u16, u16)>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "icat".into(),
        "--stdin=no".into(),
        "--transfer-mode=stream".into(),
        "--passthrough=tmux".into(),
        "--unicode-placeholder".into(),
        "--align=center".into(),
        "--scale-up=yes".into(),
        format!("--image-id={VOICEFOX_KITTY_IMAGE_ID}").into(),
        format!(
            "--place={}x{}@{}x{}",
            area.width, area.height, area.x, area.y
        )
        .into(),
        "--no-trailing-newline".into(),
    ];
    if let Some((columns, rows, width, height)) = window_size {
        args.push(format!("--use-window-size={columns},{rows},{width},{height}").into());
    }
    args.extend([OsString::from("--"), OsString::from(path)]);
    args
}

fn terminal_window_size() -> Option<(u16, u16, u16, u16)> {
    if let Ok(size) = crossterm::terminal::window_size() {
        let width = if size.width == 0 {
            size.columns.saturating_mul(10)
        } else {
            size.width
        };
        let height = if size.height == 0 {
            size.rows.saturating_mul(20)
        } else {
            size.height
        };
        return Some((size.columns, size.rows, width, height));
    }

    crossterm::terminal::size().ok().map(|(columns, rows)| {
        (
            columns,
            rows,
            columns.saturating_mul(10),
            rows.saturating_mul(20),
        )
    })
}

fn write_kitty_apc(output: &mut impl Write, command: &[u8], tmux: bool) -> std::io::Result<()> {
    let mut apc = Vec::with_capacity(command.len() + 5);
    apc.extend_from_slice(b"\x1b_G");
    apc.extend_from_slice(command);
    apc.extend_from_slice(b"\x1b\\");

    if !tmux {
        return output.write_all(&apc);
    }

    output.write_all(b"\x1bPtmux;")?;
    for byte in apc {
        if byte == b'\x1b' {
            output.write_all(b"\x1b")?;
        }
        output.write_all(&[byte])?;
    }
    output.write_all(b"\x1b\\")
}

fn is_tmux_session() -> bool {
    std::env::var_os("TMUX").is_some()
}

fn supports_kitty_graphics() -> bool {
    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();

    term_program.contains("wezterm")
        || term_program.contains("ghostty")
        || term.contains("kitty")
        || std::env::var_os("WEZTERM_PANE").is_some()
        || std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
        || std::env::var_os("KONSOLE_VERSION").is_some()
}

fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("//") {
        format!("https:{url}")
    } else {
        url.to_string()
    }
}

fn cover_referer(url: &str) -> Option<&'static str> {
    if url.contains("kuwo.cn") {
        Some("https://www.kuwo.cn/")
    } else if url.contains("kugou.com") {
        Some("https://www.kugou.com/")
    } else if url.contains("qq.com") {
        Some("https://y.qq.com/")
    } else if url.contains("music.163.com") || url.contains("126.net") {
        Some("https://music.163.com/")
    } else {
        None
    }
}

fn simple_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use ratatui::layout::Rect;

    use super::{CoverGeometry, kitten_icat_args, probe_aspect_blocking, write_kitty_apc};

    /// 方形封面 + 2:1 单元格
    fn square() -> CoverGeometry {
        CoverGeometry::new(2.0, 1.0)
    }

    #[test]
    fn box_height_follows_the_cell_aspect() {
        // inner 宽 41 → 41/2 ≈ 21 行内容，加上下边框 = 23
        assert_eq!(square().box_height(43, 24), 23);
        // 高度不够时被 max_height 压住
        assert_eq!(square().box_height(43, 8), 8);
        // 连一行内容都放不下就整个不画
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
        // 错误回退
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
    fn probe_reads_dimensions_even_when_the_extension_lies() {
        // 缓存文件名一律是 .jpg，实际内容却可能是任何格式
        let path = std::env::temp_dir().join("voicefox-cover-probe.jpg");
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(20, 10))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(probe_aspect_blocking(&path), Some(2.0));
    }

    #[test]
    fn tmux_kitty_apc_escapes_inner_escape_bytes() {
        let mut output = Vec::new();
        write_kitty_apc(&mut output, b"a=d", true).unwrap();

        assert_eq!(output, b"\x1bPtmux;\x1b\x1b_Ga=d\x1b\x1b\\\x1b\\".to_vec());
    }

    #[test]
    fn kitten_uses_tmux_passthrough_and_exact_placement() {
        let args = kitten_icat_args(
            "/tmp/cover.jpg",
            Rect::new(5, 8, 28, 10),
            Some((120, 40, 1200, 800)),
        );
        let args: Vec<&OsStr> = args.iter().map(|arg| arg.as_os_str()).collect();

        assert!(args.contains(&OsStr::new("--passthrough=tmux")));
        assert!(args.contains(&OsStr::new("--unicode-placeholder")));
        assert!(args.contains(&OsStr::new("--place=28x10@5x8")));
        assert!(args.contains(&OsStr::new("--use-window-size=120,40,1200,800")));
        assert_eq!(args.last(), Some(&OsStr::new("/tmp/cover.jpg")));
    }
}
