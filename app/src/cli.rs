use clap::Parser;

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
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
