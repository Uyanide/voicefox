use clap::Parser;
use std::path::PathBuf;

/// voicefox: Rust TUI 版 lx-music-desktop
#[derive(Parser, Debug)]
#[command(name = "voicefox", version, about)]
pub struct Cli {
    /// 配置文件路径
    #[arg(short, long, default_value = "")]
    pub config: String,

    /// 日志级别
    #[arg(short, long, default_value = "info")]
    pub log_level: String,

    /// 检查 libmpv 运行时是否可以初始化后退出
    #[arg(long)]
    pub check_libmpv: bool,

    /// 将收藏、历史和自建歌单导出为版本化 JSON 文件后退出
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["import_data", "import_playlist"]
    )]
    pub export_data: Option<PathBuf>,

    /// 从版本化 JSON 文件导入数据后退出（导入前会自动备份）
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["export_data", "import_playlist"]
    )]
    pub import_data: Option<PathBuf>,

    /// 导入 M3U、LX Music 或网易云歌单文件并创建自建歌单后退出
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["export_data", "import_data"]
    )]
    pub import_playlist: Option<PathBuf>,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
