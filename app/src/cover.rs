use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use ratatui::layout::Rect;
use reqwest::header::{ACCEPT, REFERER};

const VOICEFOX_KITTY_IMAGE_ID: u32 = 0x56_46_58;

/// 封面状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverState {
    Empty,
    Loading,
    Ready,
    Unavailable(String),
}

pub struct CoverService {
    client: reqwest::Client,
    image_path: RwLock<Option<String>>,
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
            image_path: RwLock::new(None),
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
        *self.image_path.write().unwrap() = None;
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
        self.image_path.read().unwrap().is_some()
    }

    pub fn state(&self) -> CoverState {
        self.state.read().unwrap().clone()
    }

    pub async fn load(&self, url: Option<String>) -> Result<(), String> {
        let request_id = self.request_id.fetch_add(1, Ordering::SeqCst) + 1;
        *self.image_path.write().unwrap() = None;
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
        let mut result_path: Option<String> = None;

        for attempt in 0..3 {
            if self.request_id.load(Ordering::SeqCst) != request_id {
                return Ok(());
            }
            match self.download_and_cache(&url).await {
                Ok(path) => {
                    result_path = Some(path);
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
            if let Some(ref path) = result_path {
                *self.image_path.write().unwrap() = Some(path.clone());
                *self.state.write().unwrap() = CoverState::Ready;
                self.display_gen.fetch_add(1, Ordering::SeqCst);
            } else {
                *self.state.write().unwrap() = CoverState::Unavailable(last_error.clone());
            }
        }

        match result_path {
            Some(_) => Ok(()),
            None => Err(last_error),
        }
    }

    /// 下载封面到本地缓存，返回缓存路径
    async fn download_and_cache(&self, url: &str) -> Result<String, String> {
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
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
            return Err("封面文件不存在".to_string());
        }

        // 远程文件：下载到缓存
        let hash = simple_hash(url.as_bytes());
        let cache_path = cache_dir.join(format!("{}.jpg", hash));

        if cache_path.exists() {
            return Ok(cache_path.to_string_lossy().to_string());
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

        Ok(cache_path.to_string_lossy().to_string())
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

        let path = match self.image_path.read().unwrap().clone() {
            Some(p) => p,
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

/// 终端单元格的高宽比。失败返回 2.0
pub fn cell_aspect() -> f32 {
    let Some((columns, rows, width, height)) = terminal_window_size() else {
        return 2.0;
    };
    if columns == 0 || rows == 0 || width == 0 || height == 0 {
        return 2.0;
    }
    let cell_width = f32::from(width) / f32::from(columns);
    let cell_height = f32::from(height) / f32::from(rows);
    if cell_width <= 0.0 {
        return 2.0;
    }
    (cell_height / cell_width).clamp(1.0, 4.0)
}

/// 封面框应有的高度，返回 0 表示放不下。
pub fn cover_box_height(box_width: u16, max_height: u16, cell_aspect: f32) -> u16 {
    let inner_width = box_width.saturating_sub(2);
    if inner_width == 0 || max_height < 3 {
        return 0;
    }
    let inner_height = (f32::from(inner_width) / cell_aspect).round().max(1.0) as u16;
    inner_height.saturating_add(2).min(max_height)
}

/// 封面图片在框内 inner 区域中的实际位置。
/// - 高度不够时压缩高度、水平居中、两侧留边
/// - 返回的矩形永远落在 inner 之内
/// - 封面尺寸假设 1:1，以避免额外的解码路径开销
pub fn cover_image_rect(inner: Rect, cell_aspect: f32) -> Rect {
    if inner.width == 0 || inner.height == 0 {
        return Rect::ZERO;
    }
    let fit_height = (f32::from(inner.width) / cell_aspect).round().max(1.0) as u16;
    if fit_height <= inner.height {
        let y = inner.y + (inner.height - fit_height) / 2;
        return Rect::new(inner.x, y, inner.width, fit_height);
    }
    let fit_width =
        ((f32::from(inner.height) * cell_aspect).round().max(1.0) as u16).clamp(1, inner.width);
    let x = inner.x + (inner.width - fit_width) / 2;
    Rect::new(x, inner.y, fit_width, inner.height)
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

    use super::{cover_box_height, cover_image_rect, kitten_icat_args, write_kitty_apc};

    #[test]
    fn cover_box_height_follows_the_cell_aspect() {
        // inner 宽 41 → 41/2 ≈ 21 行内容，加上下边框 = 23
        assert_eq!(cover_box_height(43, 24, 2.0), 23);
        // 高度不够时被 max_height 压住
        assert_eq!(cover_box_height(43, 8, 2.0), 8);
        // 连一行内容都放不下就整个不画
        assert_eq!(cover_box_height(43, 2, 2.0), 0);
        assert_eq!(cover_box_height(2, 24, 2.0), 0);
    }

    #[test]
    fn cover_image_is_centered_and_never_leaves_the_box() {
        // 高度充足：正好占满 inner
        let inner = Rect::new(5, 8, 40, 20);
        assert_eq!(cover_image_rect(inner, 2.0), inner);

        // 高度被压缩：按高度反推宽度，水平居中留边
        let squashed = Rect::new(5, 8, 40, 6);
        let rect = cover_image_rect(squashed, 2.0);
        assert_eq!(rect, Rect::new(19, 8, 12, 6));
        assert_eq!(squashed.union(rect), squashed);

        // 极端窄框也不会溢出
        let tiny = Rect::new(0, 0, 3, 9);
        assert_eq!(tiny.union(cover_image_rect(tiny, 2.0)), tiny);
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
