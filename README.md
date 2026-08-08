# voicefox

> 终端里的音乐播放器 — Rust TUI 版 lx-music-desktop

[![CI](https://github.com/emoeem/voicefox/actions/workflows/ci.yml/badge.svg)](https://github.com/emoeem/voicefox/actions/workflows/ci.yml)

voicefox 是一个运行在终端中的音乐播放器，使用 Rust 编写，基于 ratatui 构建界面，通过 libmpv 播放音频。支持多音源搜索、在线播放、歌词显示、收藏管理等功能。

无需离开终端，也能享受完整的音乐体验。

## 截图

```
┌──────────────────────────────────────────────────────────┐
│ 1 队列 │ 2 搜索 │ 3 排行榜 │ 4 歌单 │ 5 收藏 │ 6 历史 │ 7 本地 │ 8 设置 │
├──────────────────────────────────────────────────────────┤
│ ┌────────────────────────────────────────────────────┐  │
│ │ ▶ 歌曲名称                                        │  │
│ │ 歌手 - 专辑                                       │  │
│ │ 音源: kw | 音质: 320k                             │  │
│ └────────────────────────────────────────────────────┘  │
│ ┌──────────────┐  ┌──────────────────────────────────┐  │
│ │ 最近播放      │  │ 歌词                             │  │
│ │ 1. 歌曲 A     │  │ [00:12.34] 歌词第一行            │  │
│ │ 2. 歌曲 B     │  │ [00:16.78] 歌词第二行            │  │
│ │ 3. 歌曲 C     │  │ ...                              │  │
│ └──────────────┘  └──────────────────────────────────┘  │
├──────────────────────────────────────────────────────────┤
│ ████████████████░░░░░░░░░░░░░░░░ 03:12 / 04:30          │
│ ▶ 歌曲名称 · 音量: 80 · 循环列表                       │
└──────────────────────────────────────────────────────────┘
```

## 特性

### ✅ 已实现
- **多音源搜索**：网易云音乐、酷狗音乐、酷我音乐、QQ 音乐、咪咕音乐、哔哩哔哩
- **在线播放**：通过进程内 libmpv 播放高品质音乐
- **本地音乐**：扫描本地音乐目录，支持 MP3/FLAC/M4A/OGG/WAV，自动读取封面、同名 LRC 和音频内嵌歌词，并可确认后删除本地文件
- **封面显示**：按终端能力自动选择 Kitty / Sixel / iTerm2 图片协议，都不支持时用 Unicode 半格块渲染
- **tmux 封面**：在 tmux 中通过 passthrough 传递图形协议序列，封面照常显示；detach 后再 attach 自动重传，关掉终端、SSH 断线、换机器 attach 都能恢复，前提是这些终端支持同一种图形协议
- **歌词支持**：支持 LRC、KRC、QRC、YRC 多种歌词格式，支持翻译歌词
- **收藏管理**：添加/取消收藏歌曲和热门歌单
- **播放历史**：自动记录播放记录，支持删除单条、清空和配置保留上限
- **内容排序**：收藏、历史和本地音乐支持按最近时间、名称、歌手、专辑和时长排序；收藏与历史可按来源排序，本地音乐可按路径排序，当前模式显示在底部状态栏和右键菜单
- **单曲入队**：可将任意列表中的选中歌曲追加到队尾或设为下一首
- **队列管理**：支持键盘调序、鼠标拖拽调序、移除单曲和清空队列
- **播放模式**：支持列表循环、单曲循环、随机、顺序播放和播完停止
- **排行榜**：按音源切换榜单目录，查看各音源实时热门歌曲
- **热门歌单**：按音源切换并分页浏览酷我、酷狗、QQ、网易云、咪咕的实时歌单
- **换源与失败跳过**：获取地址或实际播放失败时依次尝试其他解析器和音源，全部失败后自动播放下一首
- **JS 自定义音源**：同时加载多个社区音源脚本，按配置顺序解析播放地址、歌词和封面；libmpv 拒绝失效链接时可从下一个脚本继续
- **主题配置**：可自定义颜色主题
- **鼠标支持**：支持点击、滚轮、队列拖拽和歌曲右键操作菜单
- **TUI 通知**：支持信息、成功、警告、错误四级浮动通知，可配置开关和停留时间
- **桌面通知**：Linux 上通过 D-Bus 发送系统通知，切歌时支持专辑封面
- **Waybar / MPRIS**：显示歌曲、歌手、专辑、进度和播放状态，并支持播放控制
- **键盘快捷键**：完整的键盘操作

## 最近更新

### 2026-08-08：设置页键位与鼠标交互修复（v0.3.1）

- 数字键 `1` 到 `8` 始终用于切换标签页；新增播放和数据操作改用 `F1` 到 `F10` 及 `Shift+F1` 到 `Shift+F8`。
- 旧配置中的冲突键位会在启动时迁移，`Ctrl+数字` 等不冲突的自定义组合键保持不变。
- 设置项支持鼠标点击；底部 JS 音源、本地目录和状态栏面板支持点击选择、滚轮移动、右键操作与状态栏拖动排序。
- 补全音频设备和外部歌单路径输入弹窗，鼠标触发与键盘快捷键使用相同处理流程。

### 2026-08-08：完整播放控制、增量本地库与数据迁移（v0.3.0）

- 播放器增加播放速度、音频输出设备、ReplayGain、均衡器、声道模式、左右平衡、A-B 循环、无缝播放和淡入淡出控制。
- 设置页新增这些控制项和数据导入导出快捷键；快捷键使用现有 `[keybindings.pages.settings]` 配置，可单独重绑，鼠标点击设置项与快捷键共用同一动作。
- 歌曲右键菜单增加“播放控制”子菜单，可直接切换速度、ReplayGain、均衡器、声道和平衡，并执行淡入淡出与 A-B 循环。
- 本地音乐库支持 `notify` 文件监听、增量扫描、重复/损坏/缺失诊断、OPUS/CUE/AIFF 识别和 `lofty` 标签封面读写。
- 数据导出采用版本化 JSON、原子写入和自动备份；支持旧格式迁移，以及 M3U、LX Music 和网易云风格歌单导入。

### 2026-08-04：播放器迁移到 libmpv2

- 播放器改为通过 `libmpv2 6.0.0` 在 voicefox 进程内控制 libmpv，不再启动 `mpv` 命令行程序或使用 JSON IPC。
- 播放进度、`audio-pts`、时长、暂停和缓冲状态改为由 libmpv 属性事件直接推送；歌词继续优先跟随实际可听音频进度。
- Windows CI 和发布制品改用 MinGW 构建，并将 `libmpv-2.dll` 与 `voicefox.exe` 一起打包。

### 2026-08-01：播放模式、进度条与音源识别修复

- 播放结束事件现在携带播放器代次，只处理当前歌曲的 mpv 事件；播放模式会按当前队列继续切歌，旧播放器事件不会干扰下一首。
- 修复鼠标点击进度条时按整行宽度计算导致的偏移，点击位置现在与实际绘制的进度条一致。
- 设置页显示 JS 音源脚本声明的 `@name`，底部状态栏显示当前实际解析成功的内置音源或 JS 音源名称。
- 新增可配置状态栏内容，可在设置页显示、隐藏并调整字段顺序，也可编辑 `[ui].status_bar_items`。

### 2026-07-31：播放失败恢复与多 JS 音源续试

- 获取播放地址失败、请求超时或 mpv 无法播放时，在所有可用解析方式耗尽后自动跳到队列下一首。
- 单曲循环遇到失败歌曲时也会前进；列表循环最多尝试完整一轮，避免整队列不可用时无限重试。
- 多个 JS 音源按配置顺序解析。前一个脚本返回错误时继续下一个；如果脚本返回的 URL 被 mpv 拒绝，也会从后续脚本继续。
- 开启自动换源后，回退顺序为：后续 JS 脚本、歌曲内置音源、其他已启用平台的同曲匹配，最后跳过当前歌曲。

### 2026-07-30：设置、历史、分页与播放器状态

- 设置页可直接修改音质、默认音源、音源启用状态、自动换源、歌词选项、代理、超时、封面协议、FPS、滚动步长、MPRIS、本地扫描深度和历史上限。
- 播放音量通过快捷键或 MPRIS 修改后会写回配置；停止状态恢复时只还原队列，不会启动 mpv。
- 历史页面支持 `d` 删除单条、`Shift+D` 清空全部，右键菜单提供相同操作。
- 热门歌单支持滚动到末尾自动加载下一页，并按音源缓存已经加载的页。
- 多个 JS 音源可以同时加载、依次回退，并共同参与聚合搜索；禁用音源不会继续显示在搜索范围中。
- mpv 现在根据真实加载、缓冲和 `playback-restart` 事件更新播放状态。

### 2026-07-28：B 站链接直达与内容排序

- 搜索框支持输入 B 站 BV/av 号、视频长链接、`b23.tv` 短链和分 P 链接，解析后可直接选择播放。
- 收藏、历史和本地音乐页面新增多种排序方式，可通过 `s` 或歌曲右键菜单切换。
- 底部状态栏显示当前页面的排序方式，本地音乐支持按文件修改时间排序。

### 2026-07-27：右键菜单、双通道通知与桌面集成

- 搜索、队列、排行榜歌曲、歌单歌曲、收藏、历史和本地音乐支持歌曲右键菜单。
- 右键菜单提供播放、设为下一首、加入队尾、收藏切换，以及队列移除或本地文件删除等页面相关操作。
- TUI 通知升级为信息、成功、警告、错误四个级别，改为浮动 toast，并支持点击关闭。
- 新增 Linux 桌面通知，播放歌曲时显示歌曲信息并可选用专辑封面。
- 新增标准 MPRIS 服务，Waybar、playerctl 和桌面媒体键可控制 voicefox。
- 通知配置迁移到独立的 `[notification]`，旧版 `[ui]` 通知字段会自动迁移。
- 配置版本升级时会修复旧配置已有本地音乐目录但 `enabled = false` 导致不自动扫描的问题。

### 2026-07-24：键位配置与本地音乐交互

- 修复旧配置缺少 `local_music.enabled` 时启动自动扫描被意外禁用的问题。
- 本地音乐列表支持鼠标滚轮、单击选择和双击播放。
- 自定义键位现在按动作合并默认配置，只覆盖一个键位不会清空其他全局或页面键位。
- 兼容 Kitty 等终端为大写字符附带 `Shift` 修饰符的按键事件。
- 完善 Colemak 预设，所有列表页统一使用 `e` / `n` 上下移动，并保留其余默认键位。

### 2026-07-22：队列、本地文件与 Windows 稳定性

- 队列页面支持使用 `Shift+K` / `Shift+J` 或 `Shift+↑` / `Shift+↓` 调整歌曲顺序。
- 队列页面支持使用 `d` / `Delete` 移除选中歌曲，使用 `Shift+D` 清空队列。
- 全局按 `m` 可依次切换列表循环、单曲循环、随机、顺序播放和播完停止，切换结果会保存到配置文件。
- 本地音乐页面支持使用 `d` / `Delete` 删除歌曲文件；删除前显示歌曲名称和文件路径，并要求按 `y` 确认。
- 删除确认框为模态操作，打开时不会触发切歌、退出、切换页面等其他快捷键。
- 修复 Windows Terminal 和 WezTerm 中直接关闭终端后 mpv 仍在后台播放、任务管理器残留进程的问题。
- 修复 Windows 下启动 mpv 和 Node.js 子进程时可能弹出额外终端窗口的问题。
- 修复播放后列表页被自动切回队列、本地列表无法循环导航，以及部分歌曲列表不能使用 `l` 播放的问题。
- 改进音源导入、删除与退出时的后台任务清理，避免 TUI 卡住或终端闪退。

### 多音源内容浏览

- 排行榜和热门歌单都支持在页面顶部切换音源。
- 每个音源使用自己的榜单目录、榜单歌曲和热门歌单接口，不再固定使用酷我音源。
- 热门歌单支持进入详情、播放歌曲，以及收藏和取消收藏歌单。
- 列表支持手动刷新，切换音源时会使用对应音源的数据缓存，避免不同音源内容混用。

### 播放与队列

- 在搜索、排行榜歌曲、热门歌单歌曲、收藏、历史和本地音乐页面中：
  - `a` 将选中歌曲追加到队尾。
  - `Shift+A` 将选中歌曲插入当前播放歌曲之后。
- 加入队列不会替换当前播放列表，也不会打断当前播放。
- 获取播放地址失败或实际播放失败时，开启换源后会依次尝试剩余 JS 脚本、内置音源和其他平台的同曲匹配。
- 当前歌曲的所有解析方式都失败后会自动播放下一首；连续失败达到队列长度时停止，避免列表循环无限重试。

### 本地音乐与歌词

- 本地音乐优先读取同名 `.lrc` 文件。
- 没有同名 `.lrc` 时，会读取音频文件内嵌的 ID3/Vorbis/MP4 歌词。
- 没有时间戳的纯文本内嵌歌词也会显示，并按歌曲时长分配歌词行。
- 封面图片只显示在队列主页；本地音乐、历史等列表页不会残留上一首歌曲的封面图层。

### 稳定性

- 修复 Windows GNU 目标编译错误。
- 修复音源导入和后台加载任务的请求代次竞态。
- 修复阻塞读取导致的超时检查失效问题。
- 修复本地音乐扫描阻塞 TUI，以及配置默认值和配置读取错误处理问题。

## 开发中 / 未来计划
- [x] **哔哩哔哩音频**：支持搜索、BV/av 号、视频长链接、`b23.tv` 短链、分 P 直达、热门推荐、收藏夹、视频音频流和扫码登录；推荐接口异常时自动降级到全网热门
- [ ] **听书模式**：支持有声书、播客内容
- [x] **自动补全歌词**：播放时自动从多个源匹配歌词
- [x] **歌单管理**：创建、重命名和编辑自定义歌单
- [x] **非原生图片终端兼容**：Kitty、Sixel、iTerm2 三种图片协议自动探测，都不支持时退回 Unicode 半格块渲染
- [ ] **跨平台包管理**：支持更多 Linux 发行版、macOS
- [ ] **更多音源插件**：兼容更多 lx-music 社区音源
- [ ] **TUI 超窄布局**：主要页面已支持宽窄布局，继续完善极小终端下的导航和提示

## 安装

### 前置依赖

- **libmpv**（必需）：音频播放引擎，无需安装或调用 `mpv` 命令行程序
  - Linux：`sudo pacman -S mpv`（Arch） / `sudo apt install libmpv-dev`（Debian/Ubuntu）
  - macOS：`brew install mpv`
  - Windows：GitHub Actions 制品已包含 `libmpv-2.dll`

### tmux 中显示封面

在 `~/.tmux.conf` 中启用终端控制序列透传：

```tmux
set -g allow-passthrough on
```

同时确保 `$TERM` 以 `tmux` 为前缀：

```tmux
set -g default-terminal "tmux-256color"
```

重新加载配置并重启 voicefox：

```bash
tmux source-file ~/.tmux.conf
```

detach 再 attach 之后封面会自动恢复。极少数情况下可能无法自动恢复，这种情况下可以按 `Ctrl+R` 手动重传。

图形协议只在启动时探测一次，之后不再重探。因此同一个 voicefox 进程的生命周期内，所有 attach 上来的终端模拟器必须支持同一种图形协议：

- 不要从图形协议不兼容的终端同时 attach 到一个 session（例如仅支持 kitty 协议的 Ghostty 和仅支持 sixel 协议的 Foot）
- 也不要 detach 之后换用不兼容的终端 attach 回来

协议不匹配时封面会显示异常或完全不显示，重启 voicefox 即可按新终端重新探测。都使用同一协议的情况不受影响。混用无法避免时，可以在配置里把 `cover_protocol` 固定成各终端都支持的协议（例如 `halfblocks`）。

### 封面渲染协议

默认自动探测，按终端能力选择 Kitty / Sixel / iTerm2，都不支持时用 Unicode 半格块。探测不准时可以在配置文件里强制指定：

```toml
[ui]
# auto（默认）| kitty | sixel | iterm2 | halfblocks
cover_protocol = "auto"
```

### Waybar 控制模块

voicefox 在 Linux 上默认注册标准 MPRIS 服务。Waybar 使用内置 `mpris` 模块即可显示和控制，无需额外轮询脚本：

```jsonc
"mpris": {
  "format": "{player_icon}  {title}",
  "format-paused": "{status_icon}  {title}",
  "format-stopped": "",
  "player-icons": {
    "voicefox": "",
    "default": ""
  },
  "status-icons": {
    "playing": "",
    "paused": ""
  },
  "tooltip-format": "{player} - {status}\n{title}\n{artist} - {album}",
  "on-click": "play-pause",
  "on-click-middle": "previous",
  "on-click-right": "next"
}
```

将 `"mpris"` 放入 Waybar 的模块列表并重启 Waybar。也可以使用 `playerctl -l` 检查是否出现 `voicefox`。

### 从源码编译

```bash
# 克隆仓库
git clone https://github.com/emoeem/voicefox.git
cd voicefox

# 编译运行
cargo run --release

# 编译后的二进制位于
# ./target/release/voicefox
```

### Linux

```bash
# 编译
git clone https://github.com/emoeem/voicefox.git
cd voicefox
cargo build --release

# 安装到系统
sudo cp target/release/voicefox /usr/local/bin/

# 安装桌面入口和通知图标（当前用户）
install -Dm644 icons/512.png \
  ~/.local/share/icons/hicolor/512x512/apps/voicefox.png
install -Dm644 assets/voicefox.desktop \
  ~/.local/share/applications/voicefox.desktop

# Arch Linux 安装 libmpv
sudo pacman -S mpv

# Debian/Ubuntu 安装 libmpv 开发包
sudo apt install libmpv-dev

# Fedora 安装 libmpv 开发包
sudo dnf install mpv-libs-devel
```

### macOS

```bash
# 安装依赖
brew install mpv

# 编译
git clone https://github.com/emoeem/voicefox.git
cd voicefox
cargo build --release

# 安装
cp target/release/voicefox /usr/local/bin/
```

### Windows

#### 方法一：GitHub Actions 下载（推荐，无需安装 Rust）

1. 前往 [Actions](https://github.com/emoeem/voicefox/actions) 页面
2. 选择最新的 CI 构建
3. 下载 `voicefox-windows-x86_64` 制品
4. 解压 `voicefox-windows-x86_64.zip`
5. 保持 `voicefox.exe` 和 `libmpv-2.dll` 在同一目录并运行

#### 方法二：从 Linux 交叉编译

```bash
# 在 Linux 上交叉编译 Windows 版本
sudo apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu

# 下载 Windows libmpv 开发包，将 libmpv.dll.a 和 libmpv-2.dll
# 解压到 .deps/mpv/64 后编译
MPV_SOURCE="$PWD/.deps/mpv" cargo build --release \
  --target x86_64-pc-windows-gnu \
  --features lx-player/build_libmpv

# 输出文件
# ./target/x86_64-pc-windows-gnu/release/voicefox.exe
# 将 .deps/mpv/64/libmpv-2.dll 复制到 exe 同一目录
```

#### 方法三：在 Windows 上本地编译

```powershell
# 安装 Rust 和 MinGW-w64
# 下载 Windows libmpv 开发包，将其解压到 .deps/mpv/64

git clone https://github.com/emoeem/voicefox.git
cd voicefox
rustup target add x86_64-pc-windows-gnu
$env:MPV_SOURCE = "$PWD\.deps\mpv"
cargo build --release --target x86_64-pc-windows-gnu --features lx-player/build_libmpv
# 输出在 target/x86_64-pc-windows-gnu/release/voicefox.exe
# 将 .deps/mpv/64/libmpv-2.dll 复制到 exe 同一目录
```

## 快速开始

### 启动

```bash
voicefox
```

首次启动会自动创建默认配置文件 `~/.config/voicefox/config.toml`。

完整的快捷键说明见 [KEYBINDINGS.md](KEYBINDINGS.md)。

### 键盘快捷键

#### 全局快捷键（任意页面）

| 按键 | 功能 |
|------|------|
| `1` | 队列页面 |
| `2` | 搜索页面 |
| `3` | 排行榜页面 |
| `4` | 热门歌单页面 |
| `5` | 收藏歌曲页面 |
| `6` | 历史页面 |
| `7` | 本地音乐页面 |
| `8` | 设置页面 |
| `/` | 切换到搜索页面 |
| `Tab` / `Shift+Tab` | 下一个 / 上一个标签页 |
| `q` | 退出 |
| `Space` | 播放 / 暂停 |
| `n` / `>` | 下一首 |
| `b` / `<` | 上一首 |
| `m` | 依次切换播放模式：列表循环 / 单曲循环 / 随机 / 顺序播放 / 播完停止 |
| `]` | 快进 5 秒 |
| `[` | 后退 5 秒 |
| `.` | 音量增加 |
| `,` | 音量减少 |
| `Ctrl+L` | 收藏 / 取消收藏当前歌曲 |
| `Esc` | 返回上一级 / 取消 |

通过快捷键或 MPRIS 修改后的音量会自动写回配置，并在下次启动时恢复。

#### 队列页面

| 按键 | 功能 |
|------|------|
| `k` / `j` | 选择上一首 / 下一首队列歌曲 |
| `Enter` | 播放选中的队列歌曲 |
| `Shift+↑` / `Shift+↓` | 将选中歌曲向前 / 向后移动一位 |
| `Shift+K` / `Shift+J` | 将选中歌曲向前 / 向后移动一位 |
| `d` / `Delete` | 从队列移除选中的一首歌曲 |
| `Shift+D` | 清空整个播放队列 |
| `←` `→` | 后退 / 快进 5 秒 |
| `↑` `↓` | 音量加 / 音量减 |

#### 搜索页面

| 按键 | 功能 |
|------|------|
| 输入文字 | 自动搜索（300ms 防抖） |
| `i` / `/` | 进入搜索输入模式 |
| `Enter` / `l` | 播放选中歌曲 |
| `a` / `A` | 加入队尾 / 插到当前歌曲之后 |
| `↑` `k` | 选择上一首 |
| `↓` `j` | 选择下一首 |
| `PgUp` / `Ctrl+U` | 向上翻页 |
| `PgDn` / `Ctrl+D` | 向下翻页 |
| `Home` / `g` | 跳到列表顶部 |
| `End` / `G` / `Shift+G` | 跳到列表底部 |
| `v` | 打开当前歌曲的其他音源版本 |
| `←` `→` | 切换搜索范围或音源 |
| `Esc` | 退出输入模式 / 返回队列页面 |

#### 排行榜页面

| 按键 | 功能 |
|------|------|
| `↑` `k` | 选择上一项 |
| `↓` `j` | 选择下一项 |
| `Enter` / `l` | 播放选中歌曲 |
| `a` / `A` | 加入队尾 / 插到当前歌曲之后 |
| `PgUp` / `Ctrl+U` | 向上翻页 |
| `PgDn` / `Ctrl+D` | 向下翻页 |
| `Home` / `g` | 跳到列表顶部 |
| `End` / `G` / `Shift+G` | 跳到列表底部 |
| `←` | 返回榜单列表 |
| `→` | 进入选中榜单 |
| `Esc` | 返回上一级 |

#### 热门歌单页面

| 按键 | 功能 |
|------|------|
| `←` `→` / `[` `]` | 切换已收藏或不同音源 |
| `↑` `k` / `↓` `j` | 选择歌单或歌曲 |
| `Enter` / `l` | 进入歌单 / 播放选中歌曲 |
| `a` / `A` | 在歌曲列表中加入队尾 / 插到当前歌曲之后 |
| `f` / `Ctrl+L` | 收藏 / 取消收藏当前歌单 |
| `r` | 刷新当前音源或歌单 |
| `Esc` | 返回歌单列表 |

浏览在线热门歌单时，选中并滚动到当前列表末尾会自动加载下一页。

#### 收藏页面

| 按键 | 功能 |
|------|------|
| `↑` `k` | 选择上一首 |
| `↓` `j` | 选择下一首 |
| `Enter` / `l` | 播放选中歌曲 |
| `a` / `A` | 加入队尾 / 插到当前歌曲之后 |
| `s` | 切换排序方式 |
| `/` | 筛选收藏歌曲 |
| `d` / `Delete` / `Ctrl+L` | 取消收藏选中歌曲 |
| `Esc` | 退出筛选模式 |

#### 历史页面

| 按键 | 功能 |
|------|------|
| `↑` `k` | 选择上一首 |
| `↓` `j` | 选择下一首 |
| `Enter` / `l` | 播放选中歌曲 |
| `a` / `A` | 加入队尾 / 插到当前歌曲之后 |
| `PgUp` / `Ctrl+U` | 向上翻页 |
| `PgDn` / `Ctrl+D` | 向下翻页 |
| `Home` / `g` | 跳到列表顶部 |
| `End` / `G` / `Shift+G` | 跳到列表底部 |
| `s` | 切换排序方式 |
| `/` | 筛选播放历史 |
| `d` | 删除选中的一条历史 |
| `Shift+D` | 清空播放历史 |

#### 本地音乐页面

| 按键 | 功能 |
|------|------|
| `↑` `k` / `↓` `j` | 选择上一首 / 下一首 |
| `Enter` / `l` | 播放选中歌曲 |
| `a` / `A` | 加入队尾 / 插到当前歌曲之后 |
| `s` | 切换排序方式 |
| `/` | 筛选本地音乐 |
| `d` / `Delete` | 打开确认框，永久删除选中的本地音乐文件 |
| `y` | 在删除确认框中确认删除 |
| `n` / `Esc` | 取消删除 |
| `r` | 重新扫描本地音乐目录 |

鼠标滚轮可移动选择，单击歌曲可选中，双击歌曲可直接播放。

### 配置

配置文件位于 `~/.config/voicefox/config.toml`：

```toml
[player]
engine = "mpv"
quality = "320k"      # 音质: 128k / 320k / flac / flac24bit
volume = 80
play_mode = "list-loop"  # list-loop / single-loop / random / list / none
remember_playback_state = true
history_limit = 100

[source]
enabled = ["kw", "kg", "tx", "wy", "mg", "bili"]
default = "kw"
auto_toggle = true

# JS 自定义音源；数组顺序即播放地址解析优先级
# js_sources = [
#   "https://example.com/source-a.js",
#   "https://example.com/source-b.js",
# ]

[lyric]
show_translation = true
show_yrc = true
offset = 0

[network]
proxy_url = ""
timeout = 15

[theme]
use_dark = true
accent = "#cba6f7"
text = "#cdd6f4"
muted = "#a6adc8"
border = "#585b70"
rosewater = "#f5e0dc"
flamingo = "#f2cdcd"
pink = "#f5c2e7"
mauve = "#cba6f7"
red = "#f38ba8"
maroon = "#eba0ac"
peach = "#fab387"
yellow = "#f9e2af"
green = "#a6e3a1"
teal = "#94e2d5"
sky = "#89dceb"
sapphire = "#74c7ec"
blue = "#89b4fa"
lavender = "#b4befe"
subtext_1 = "#bac2de"
subtext_0 = "#a6adc8"
overlay_2 = "#9399b2"
overlay_1 = "#7f849c"
overlay_0 = "#6c7086"
surface_2 = "#585b70"
surface_1 = "#45475a"
surface_0 = "#313244"
base = "#1e1e2e"
mantle = "#181825"
crust = "#11111b"

[ui]
enable_mouse = true
wrap_navigation = true
scroll_amount = 3
aggregate_search = true
show_cover = true
cover_protocol = "auto"   # auto | kitty | sixel | iterm2 | halfblocks
max_fps = 20
# 数组顺序即状态栏从左到右的显示顺序；删除字段可隐藏对应内容
status_bar_items = [
  "state", "source", "sort", "song", "time", "volume",
  "play-mode", "quality", "queue", "js-source-state",
]

[notification]
enable = true         # 桌面系统通知
inApp = true          # TUI 内 toast
inAppTimeout = 4      # TUI toast 停留秒数，运行时限制为 1-60
albumCover = true     # 桌面通知显示专辑封面
trackChange = true    # 切歌时发送桌面通知

[integration]
mpris = true          # Linux MPRIS / Waybar 集成，修改后重启生效

[local_music]
enabled = true
paths = ["/home/user/Music"]
max_depth = 0

```

`status_bar_items` 支持以下字段：

| 字段 | 显示内容 |
|------|----------|
| `state` | 播放、暂停、缓冲、停止或空闲状态 |
| `source` | 当前歌曲实际使用的内置音源或 JS 脚本名称 |
| `sort` | 收藏、历史和本地音乐页面的当前排序方式 |
| `song` | 当前歌曲名与歌手 |
| `time` | 当前进度与歌曲时长 |
| `volume` | 播放音量 |
| `play-mode` | 列表循环、单曲循环、随机等播放模式 |
| `quality` | 当前请求音质 |
| `queue` | 当前歌曲在播放队列中的位置 |
| `js-source-state` | JS 自定义音源是否可用 |

字段按数组顺序从左到右显示。窄终端中，放不下的后续字段会自动省略，因此可以把最重要的字段放在前面。空数组会保留状态栏背景但不显示任何内容；未知字段会在加载时忽略，重复字段会自动去重。

## 键位配置

voicefox 支持通过配置文件自定义快捷键，无需修改代码。配置文件位置：`~/.config/voicefox/config.toml`。

未在配置中指定的键位保持默认行为，因此你只需要修改想要改变的键位即可。

### 键位格式

| 格式 | 示例 | 说明 |
|------|------|------|
| 单字符 | `"q"` `"n"` `"j"` | 字母、数字、标点 |
| 特殊键 | `"Space"` `"Tab"` `"Esc"` `"Enter"` `"Backspace"` | 功能键名 |
| 方向键 | `"Up"` `"Down"` `"Left"` `"Right"` | 方向键 |
| 翻页键 | `"PageUp"` `"PageDown"` `"Home"` `"End"` | 翻页和跳转 |
| 功能键 | `"F1"` ~ `"F12"` | F1 到 F12 |
| 组合键 | `"Ctrl+l"` `"Shift+Tab"` `"Alt+Enter"` | Ctrl / Shift / Alt 组合 |

键名不区分大小写，`"k"` 和 `"K"` 效果相同。

### 全局可配置动作

在 `[keybindings.global]` 下配置，以下动作在所有页面都生效：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `global_quit` | `"q"` | 退出应用 |
| `global_play_pause` | `"Space"` | 播放 / 暂停 |
| `global_next_track` | `"n"` | 下一首 |
| `global_prev_track` | `"b"` | 上一首 |
| `global_cycle_mode` | `"m"` | 切换播放模式 |
| `global_seek_forward` | `"]"` | 快进 5 秒 |
| `global_seek_backward` | `"["` | 后退 5 秒 |
| `global_volume_up` | `"."` | 音量增加 |
| `global_volume_down` | `","` | 音量减少 |
| `global_next_tab` | `"Tab"` | 下一个标签页 |
| `global_prev_tab` | `"Shift+Tab"` | 上一个标签页 |
| `global_go_to_main` | `"Esc"` | 返回主界面 |
| `global_toggle_favorite` | `"Ctrl+l"` | 收藏 / 取消收藏 |

### 页面级可配置动作

在 `[keybindings.pages.<页面名>]` 下配置。以下动作只在对应页面生效。

**通用列表动作**（搜索、队列、排行榜、歌单、收藏、历史、本地音乐、设置 都支持）：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `list_select_up` | `"k"` | 选择上一项 |
| `list_select_down` | `"j"` | 选择下一项 |
| `list_select_first` | `"g"` | 跳到第一项 |
| `list_select_last` | `"G"` | 跳到最后一项 |
| `list_page_up` | `"Ctrl+u"` | 向上翻页 |
| `list_page_down` | `"Ctrl+d"` | 向下翻页 |
| `list_activate` | `"Enter"` 或 `"l"` | 播放 / 进入 / 激活 |
| `list_go_back` | `"Esc"` | 返回 / 退出 |
| `list_add_to_queue` | `"a"` | 添加到队列尾部 |
| `list_add_to_queue_next` | `"A"` | 插到下一首播放 |

**搜索页面专用**：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `search_input_mode` | `"i"` | 进入搜索输入模式 |
| `search_start` | `"Enter"` | 开始搜索 / 播放结果 |
| `search_toggle_aggregate` | `"v"` | 切换聚合 / 单音源搜索 |
| `search_cycle_source_prev` | `"Left"` | 切换上一个音源 |
| `search_cycle_source_next` | `"Right"` | 切换下一个音源 |

**收藏页面专用**：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `favorites_filter` | `"/"` | 进入过滤模式 |
| `favorites_remove` | `"d"` | 取消收藏选中歌曲 |
| `list_cycle_sort` | `"s"` | 切换排序方式 |

**历史页面专用**：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `history_filter` | `"/"` | 进入过滤模式 |
| `list_cycle_sort` | `"s"` | 切换排序方式 |

**本地音乐页面专用**：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `local_rescan` | `"r"` | 重新扫描本地目录 |
| `local_delete` | `"d"` | 删除选中文件（弹出确认） |
| `local_filter` | `"/"` | 进入过滤模式 |
| `list_cycle_sort` | `"s"` | 切换排序方式 |

**设置页面的播放与数据动作**：

这些动作默认使用功能键和 `Shift+功能键`；数字键 `1` 到 `8` 保留给侧边栏标签页。可以在 `[keybindings.pages.settings]` 中逐项重绑。设置页会显示重绑后的按键，鼠标点击对应行与按键执行相同动作。

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `settings_cycle_playback_speed` | `"F1"` | 循环播放速度 |
| `settings_edit_audio_device` | `"F2"` | 编辑 libmpv 音频设备名 |
| `settings_cycle_replay_gain_mode` | `"F3"` | 切换 ReplayGain 模式 |
| `settings_cycle_channel_mode` | `"F4"` | 切换自动 / 立体声 / 单声道 / 左 / 右 |
| `settings_cycle_replay_gain_preamp` | `"F5"` | 调整 ReplayGain 预放大 |
| `settings_cycle_balance` | `"F6"` | 调整左右平衡 |
| `settings_toggle_replay_gain_clip` | `"F7"` | 切换削波保护 |
| `settings_cycle_fade_in_duration` | `"F8"` | 循环淡入时长 |
| `settings_cycle_fade_out_duration` | `"F9"` | 循环淡出时长 |
| `settings_cycle_equalizer_preset` | `"F10"` | 循环均衡器预设 |
| `settings_run_fade_in` / `settings_run_fade_out` | `"Shift+F1"` / `"Shift+F2"` | 立即淡入 / 淡出当前歌曲 |
| `settings_set_ab_loop_start` / `settings_set_ab_loop_end` | `"Shift+F3"` / `"Shift+F4"` | 设置 A / B 点 |
| `settings_clear_ab_loop` | `"Shift+F5"` | 清除 A-B 循环 |
| `settings_export_data` / `settings_import_data` | `"Shift+F6"` / `"Shift+F7"` | 导出 / 导入版本化数据备份 |
| `settings_import_playlist` | `"Shift+F8"` | 输入路径并导入 M3U、LX Music 或网易云歌单 |

### 配置示例

以下示例只修改了部分键位，其余未指定的键位保持默认：

```toml
[keybindings.global]
global_next_track = "k"

[keybindings.pages.search]
list_select_up = "e"
list_select_down = "n"

[keybindings.pages.settings]
settings_cycle_playback_speed = "Alt+s"
settings_cycle_equalizer_preset = "Alt+e"
```

### 页面名列表

`[keybindings.pages.<页面名>]` 中的 `<页面名>` 必须是以下之一：

- `main` — 队列页面（主页）
- `search` — 搜索页面
- `leaderboard` — 排行榜页面
- `playlists` — 热门歌单页面
- `favorites` — 收藏页面
- `history` — 历史页面
- `local` — 本地音乐页面
- `settings` — 设置页面

## 音源说明

voicefox 内置以下音源模块：

| 音源 | ID | 说明 |
|------|----|------|
| 酷我音乐 | kw | **默认音源**，稳定性较好 |
| 酷狗音乐 | kg | 曲库丰富 |
| QQ 音乐 | tx | 腾讯旗下，热门歌曲全 |
| 网易云音乐 | wy | 社区活跃，评论多 |
| 咪咕音乐 | mg | 移动旗下，版权较多 |
| 哔哩哔哩 | bili | 支持搜索、BV/av 号、视频链接、短链和多 P 音频解析 |

**JS 自定义音源**：支持同时加载多个社区维护的 lx-music 兼容音源脚本。脚本按 `js_sources` 数组顺序参与播放地址、歌词和封面解析，并共同参与聚合搜索；前一个脚本返回错误时会继续尝试后续脚本。若某个脚本成功返回 URL，但 libmpv 实际无法播放，开启自动换源后会记住该脚本并从下一个脚本继续，而不会重复请求同一个失效链接。

通过设置页添加脚本时，新添加的 URL 会放到列表最前面，因此最后添加的脚本优先级最高。需要固定顺序时可直接编辑 `config.toml` 中的 `js_sources`。

## 项目结构

```
voicefox/
├── app/          # 主程序（TUI 界面 + 业务逻辑）
│   └── src/
│       ├── pages/       # 各页面（搜索/队列/歌单/收藏/历史/设置/排行）
│       │   └── components/  # 可复用组件（歌词/进度条/状态栏/表格）
│       ├── config/      # 配置加载
│       ├── playlist/    # 播放队列管理
│       └── theme.rs     # 主题系统
├── core/         # 核心类型和接口定义
│   └── src/
│       ├── model/       # 数据模型（歌曲/歌词/配置/音源）
│       └── traits/      # 抽象接口（音源/播放器/歌词）
├── source/       # 音源实现（各平台 API 对接）
│   └── src/
│       ├── wy/ kw/ kg/ tx/ mg/ bili/ local/  # 各音源实现
│       └── js/                  # JS 自定义音源引擎
├── player/       # 播放器引擎（libmpv2）
└── lyric/        # 歌词解析库（LRC/KRC/QRC/YRC）
```

## 技术栈

- **语言**：Rust (edition 2024)
- **TUI 框架**：[ratatui](https://github.com/ratatui/ratatui) 0.30
- **终端事件**：[crossterm](https://github.com/crossterm-rs/crossterm) 0.29
- **异步运行时**：[tokio](https://github.com/tokio-rs/tokio)
- **音频播放**：[libmpv2](https://docs.rs/libmpv2/)（进程内控制 libmpv）
- **HTTP 客户端**：[reqwest](https://github.com/seanmonstar/reqwest)
- **歌词解析**：LRC/KRC/QRC/YRC 自实现解析器

## 更新日志

### 2026-08-08（v0.3.1）

- 设置页播放与数据操作改用功能键，数字键 `1` 到 `8` 保留给标签页并自动迁移旧配置。
- 设置项和底部三个管理面板补全鼠标点击、滚轮、右键及拖动操作。

### 2026-08-04

- 使用 `libmpv2 6.0.0` 替换外部 mpv 子进程和 JSON IPC。
- 使用 libmpv 属性事件更新媒体进度、实际音频进度、时长、暂停和缓冲状态。
- Windows 构建和发布制品包含 `libmpv-2.dll`。

### 2026-07-31

- 修复音源解析失败、超时或 mpv 播放失败后停留在当前歌曲的问题。
- 所有解析方式失败后自动播放下一首，并限制连续失败次数以避免无限循环。
- mpv 确认开始播放后重置失败计数。
- 多个 JS 音源支持在实际播放失败后从下一个脚本继续解析。
- 增加失败跳过、单曲循环前进、整队列失败停止和 JS 顺序续试测试。

### 2026-07-30

- 补全设置页，可管理播放、音源、歌词、网络、通知、封面、MPRIS 和本地扫描等选项。
- 修复停止状态恢复后错误加载上一首的问题，并持久化快捷键和 MPRIS 音量调整。
- 历史页面增加筛选、单条删除、全部清空和数量上限。
- 热门歌单增加自动分页加载。
- 多个 JS 音源可同时加载、依次回退并参与聚合搜索。
- 搜索页隐藏已禁用音源。
- mpv 使用真实加载、缓冲和恢复播放事件更新状态。

### 2026-07-28

- 搜索框支持解析 B 站 BV/av 号、视频链接、短链和多 P 链接。
- 收藏、历史和本地音乐增加排序状态栏、快捷键和右键菜单。

### 2026-07-27

- 增加歌曲右键菜单、TUI 浮动通知、桌面通知和 MPRIS 集成。
- 增加队列插入、鼠标拖拽调序及收藏、历史、本地音乐的列表交互。

### 2026-07-24

- 修复旧版本地音乐配置无法在启动时自动扫描的问题。
- 增加本地音乐列表的鼠标选择、滚动和双击播放。
- 修复键位默认映射为空、局部配置覆盖整组默认键位的问题。
- 改进 Kitty 大写键事件与 Colemak 键位预设兼容性。

### 2026-07-22

- 增加队列键盘调序、单曲移除和清空功能。
- 增加本地音乐文件删除确认框，并在删除后立即刷新本地音乐库。
- 增加全局播放模式切换快捷键 `m`。
- 改进 Windows Terminal、WezTerm 的按键与子进程兼容性。
- 使用 Windows Job Object 绑定 mpv 生命周期，直接关闭终端时自动结束 mpv。
- 修复音源导入、删除、程序退出和本地音乐扫描可能阻塞的问题。
- 补充 Windows、队列、本地音乐与播放模式相关测试。

### v0.1.0 (2026-07-13)

- ✨ 首次发布，原项目名 lx-tui 更名为 voicefox
- 🎵 多音源在线音乐搜索与播放（网易云、酷狗、酷我、QQ、咪咕）
- 📜 歌词显示（LRC/KRC/QRC/YRC 格式，支持翻译）
- 🔍 聚合搜索与单音源切换
- 🏆 各音源排行榜浏览
- 📚 各音源热门歌单浏览与收藏
- ❤️ 歌曲收藏管理与播放历史
- 🔄 音源播放失败时自动跨源匹配
- 📦 JS 自定义音源加载（兼容 lx-music 社区脚本）
- 🎨 可配置颜色主题
- 🖱️ 鼠标支持（点击切换标签页、拖拽进度条）
- ⚙️ 完整的设置页面

## ☕ 赞助

如果 voicefox 对你的工作和生活有帮助，欢迎请我喝杯咖啡 ❤️

| 支付宝 | 微信 |
|--------|------|
| <img src=".github/alipay.jpg" width="200" alt="支付宝收款码"> | <img src=".github/wechat.png" width="200" alt="微信收款码"> |

## 许可证

MIT

## 致谢

- [lx-music-desktop](https://github.com/lyswhut/lx-music-desktop) — 项目灵感来源
- [lx-music-source](https://github.com/pdone/lx-music-source) — 社区音源脚本
- [rmpc](https://github.com/mierak/rmpc) — TUI 架构参考
- [go-musicfox](https://github.com/go-musicfox/go-musicfox) — 播放器设计参考
- [azusa-player-mobile](https://github.com/lovegaoshi/azusa-player-mobile) — 哔哩哔哩音源模块参考
