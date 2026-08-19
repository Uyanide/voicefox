# voicefox 配置说明

voicefox 的所有可配置项都集中在 `~/.config/voicefox/config.toml`。首次启动时若文件不存在会自动生成默认配置；旧版本配置会在启动时自动迁移，缺失的新字段使用默认值，因此升级后无需手动补全。

也可以用命令行指定其它配置文件：

```bash
voicefox --config /path/to/config.toml
```

所有配置项都支持在**设置页**（`8`）里直接修改，修改后会写回配置文件；键位自定义见本文[键位配置](#键位配置)，默认快捷键见 [KEYBINDINGS.md](../KEYBINDINGS.md)。

## 推荐配置

下面是一份覆盖日常使用、经过整理的推荐配置：开启播放状态记忆与换源、保留桌面通知和封面，本地音乐按需填写目录。直接复制到 `config.toml` 后按注释调整即可。

```toml
[player]
engine = "mpv"
quality = "flac"            # 请求档位：128k / 320k / flac / flac24bit
volume = 80
playback_speed = 1.0
audio_device = "auto"       # "auto" 使用系统默认输出设备
replaygain_mode = "track"   # off / track / album，音量不齐时建议 track
replaygain_preamp = 0.0
replaygain_clip = true      # 预放大后限制削波，防止爆音
channel_mode = "auto"       # auto / stereo / mono / left / right
balance = 0.0               # -1 全左，1 全右
# 均衡器，格式：[{ frequency_hz = 100.0, gain_db = 2.0 }]；留空表示关闭
equalizer_bands = []
fade_in_ms = 0              # 新曲淡入毫秒，0 关闭；如 200
fade_out_ms = 0             # 曲末淡出毫秒，0 关闭；如 800
play_mode = "list-loop"     # list-loop / single-loop / random / list / none
remember_playback_state = true
history_limit = 200

[source]
enabled = ["kw", "kg", "tx", "wy", "mg", "bili"]
default = "kw"
auto_toggle = true          # 播放失败时自动尝试其它音源的同曲匹配
# JS 自定义音源（lx-music user API 协议），数组顺序即解析优先级：
# js_sources = [
#   "https://ghproxy.net/raw.githubusercontent.com/pdone/lx-music-source/main/juhe/latest.js",
# ]
js_sources = []

[lyric]
show_translation = true
show_yrc = true             # 逐字歌词高亮（YRC/QRC）
offset = 0                  # 歌词整体偏移毫秒，正数提前，负数延后

[network]
proxy_url = ""              # 例如 "http://127.0.0.1:7890"
timeout = 15                # HTTP 超时秒数

[ui]
enable_mouse = true
wrap_navigation = true
scroll_amount = 3
aggregate_search = true     # 搜索时聚合所有已启用音源
show_cover = true
cover_protocol = "auto"     # auto / kitty / sixel / iterm2 / halfblocks
max_fps = 20                # 1-60，越小越省电
status_bar_items = [
  "state", "song", "time", "volume", "play-mode", "quality",
]

[notification]
enable = true               # 桌面系统通知（Linux D-Bus）
inApp = true                # TUI 内浮动 toast
inAppTimeout = 4            # toast 停留秒数，运行时限制 1-60
albumCover = true           # 桌面通知附带专辑封面
trackChange = true          # 切歌时发送桌面通知

[integration]
mpris = true                # Linux MPRIS / Waybar / 媒体键，修改后重启生效

[local_music]
enabled = true
paths = ["/home/user/Music"]  # 改成你的音乐目录，可配置多个
max_depth = 0                 # 扫描深度，0 为不限制
```

### 请求音质与实际音质

`player.quality` 只表示向音源请求的音质档位，不保证音源一定能提供该档位。不同音源可能降级、回退或返回未严格标注码率的地址。

播放开始后，状态栏的 `quality` 字段会优先显示 libmpv 从实际音频流读取的编码、码率和采样率，例如 `MP3 320K 44.1kHz`。如果播放器尚未拿到音频流参数，则暂时显示配置中的请求档位。

## 配置字段说明

### `[player]` 播放器

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `engine` | `"mpv"` | 播放器引擎，目前仅支持 `mpv` |
| `quality` | `"320k"` | 请求音质：`128k` / `320k` / `flac` / `flac24bit` |
| `volume` | `80` | 初始音量 0-100，修改后自动写回 |
| `playback_speed` | `1.0` | 播放速度倍率 |
| `audio_device` | `"auto"` | libmpv 音频输出设备 |
| `replaygain_mode` | `"off"` | ReplayGain：`off` / `track` / `album` |
| `replaygain_preamp` | `0.0` | ReplayGain 预放大（dB） |
| `replaygain_clip` | `false` | 预放大后是否限制削波 |
| `channel_mode` | `"auto"` | `auto` / `stereo` / `mono` / `left` / `right` |
| `balance` | `0.0` | 左右平衡，-1 全左，1 全右 |
| `equalizer_bands` | `[]` | 均衡器频段数组 `[{ frequency_hz, gain_db }]` |
| `fade_in_ms` | `0` | 新曲淡入毫秒，0 关闭 |
| `fade_out_ms` | `0` | 曲末淡出毫秒，0 关闭 |
| `play_mode` | `"list-loop"` | `list-loop` / `single-loop` / `random` / `list` / `none` |
| `remember_playback_state` | `true` | 退出时保存队列与进度，下次启动恢复 |
| `history_limit` | `100` | 播放历史保留条数 |

### `[source]` 音源

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `enabled` | 全部内置音源 | 参与搜索和换源的平台，值为 `kw` / `kg` / `tx` / `wy` / `mg` / `bili` |
| `default` | `"kw"` | 默认音源 |
| `auto_toggle` | `true` | 播放失败时自动尝试其它音源的同曲匹配 |
| `js_sources` | `[]` | JS 音源脚本 URL 或本地路径，数组顺序即优先级 |

### `[lyric]` 歌词

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `show_translation` | `true` | 显示翻译歌词 |
| `show_yrc` | `true` | 显示逐字歌词高亮 |
| `offset` | `0` | 歌词时间偏移毫秒，正数提前，负数延后 |

### `[network]` 网络

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `proxy_url` | `""` | HTTP 代理，如 `http://127.0.0.1:7890` |
| `timeout` | `15` | 请求超时秒数（1-300） |

### `[ui]` 界面

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `enable_mouse` | `true` | 启用鼠标支持 |
| `wrap_navigation` | `true` | 列表选择是否循环 |
| `scroll_amount` | `3` | 滚轮滚动步长 |
| `aggregate_search` | `true` | 聚合搜索所有已启用音源 |
| `show_cover` | `true` | 显示封面 |
| `cover_protocol` | `"auto"` | `auto` / `kitty` / `sixel` / `iterm2` / `halfblocks` |
| `max_fps` | `20` | 渲染帧率上限 1-60 |
| `status_bar_items` | 全部字段 | 状态栏内容与顺序，见下表 |

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
| `quality` | 实际播放音频的编码、码率和采样率；尚未获取时显示请求音质 |
| `queue` | 当前歌曲在播放队列中的位置 |
| `js-source-state` | JS 自定义音源是否可用 |

字段按数组顺序从左到右显示，窄终端放不下的后续字段会自动省略；未知字段在加载时忽略，重复字段自动去重。窄终端建议只保留 `state`、`song`、`time`、`volume`、`play-mode` 和 `quality`。

### 音源健康检测

进入设置页（`8`），将焦点切换到 **JS 音源** 面板后按 `h`，或点击面板中的“检测”。voicefox 会并发对当前启用的内置音源和已加载的 JS 音源执行轻量搜索，并显示每个音源的成功状态、延迟、返回数量和失败原因。

健康检测结果用于快速判断搜索服务是否可访问，不等同于对每一首歌曲的播放地址作保证；遇到搜索成功但播放失败的情况，仍由播放时的自动换源机制继续尝试其它音源。JS 音源的远程脚本仍建议保留本地备份，以应对代理或上游地址暂时不可用。

### `[notification]` 通知

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `enable` | `true` | Linux 桌面系统通知 |
| `inApp` | `true` | TUI 内浮动 toast |
| `inAppTimeout` | `4` | toast 停留秒数，运行时限制 1-60 |
| `albumCover` | `true` | 桌面通知附带专辑封面 |
| `trackChange` | `true` | 切歌时发送通知 |

### `[integration]` 桌面集成

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `mpris` | `true` | Linux MPRIS 服务（Waybar、playerctl、媒体键），修改后重启生效 |

### `[local_music]` 本地音乐

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `enabled` | `true` | 是否启用本地音乐 |
| `paths` | `[]` | 音乐目录列表，可多个 |
| `max_depth` | `0` | 扫描深度，0 为不限制 |

### `[theme]` 主题

默认主题是 Catppuccin Mocha 配色，约 30 个颜色字段（`accent`、`text`、`base`、`surface_*`、`overlay_*`、`subtext_*` 及语义色等），全部为十六进制颜色字符串。不配置该节即使用默认主题；完整字段可从生成的默认配置文件中查看。

## 数据文件

用户数据与配置分离，存放在 `~/.config/voicefox/data/`：

| 文件 | 内容 |
|------|------|
| `favorites.json` | 收藏的歌曲 |
| `favorite_playlists.json` | 收藏的歌单 |
| `custom_playlists.json` | 自建歌单 |
| `history.json` | 播放历史 |
| `playback_state.json` | 上次播放队列与进度 |
| `backups/` | 导入数据前的自动备份 |

导出/导入均为版本化 JSON，写入使用原子替换；收藏、历史和自建歌单也可以通过 `voicefox --export-data` / `--import-data` 在命令行完成。

## 键位配置

voicefox 支持通过配置文件自定义快捷键，无需修改代码。未在配置中指定的键位保持默认行为，因此你只需要修改想要改变的键位即可。

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
| `global_seek_forward` | `"]"` | 队列页面快进 5 秒 |
| `global_seek_backward` | `"["` | 队列页面后退 5 秒 |
| `global_volume_up` | `"."` | 音量增加 |
| `global_volume_down` | `","` | 音量减少 |
| `global_next_tab` | `"Tab"` | 下一个标签页 |
| `global_prev_tab` | `"Shift+Tab"` | 上一个标签页 |
| `global_go_to_main` | `"Esc"` | 自定义返回主界面动作；默认裸 `Esc` 由当前页面消费 |
| `global_toggle_favorite` | `"Ctrl+l"` | 收藏 / 取消收藏当前播放中的歌曲 |

### 页面级可配置动作

在 `[keybindings.pages.<页面名>]` 下配置。以下动作只在对应页面生效。

**通用列表动作**（搜索、队列、排行榜、歌单、收藏、历史、本地音乐页面支持）：

| 配置项 | 默认值 | 功能 |
|--------|--------|------|
| `list_select_up` | `"k"` | 选择上一项 |
| `list_select_down` | `"j"` | 选择下一项 |
| `list_select_first` | `"g"` | 跳到第一项 |
| `list_select_last` | `"G"` | 跳到最后一项 |
| `list_page_up` | `"Ctrl+u"` | 向上翻页 |
| `list_page_down` | `"Ctrl+d"` | 向下翻页 |
| `list_activate` | `"Enter"` 或 `"l"` | 播放 / 进入 / 激活 |
| `list_toggle_favorite` | `"f"` | 收藏 / 取消收藏当前选中的歌曲或歌单 |
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

---

返回 [README](../README.md)
