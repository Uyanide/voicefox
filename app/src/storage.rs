//! 存储层薄封装。
//!
//! 实现已统一到 `voicefox-runtime`：仓库里曾经同时存在 `app/src/storage.rs`
//! 与 `runtime/src/storage.rs` 两份 1900 余行的拷贝，两边各自演化（一份加了
//! 锁外落盘，另一份加了旧目录迁移），结果 0.3.12 切换数据目录时只有 runtime
//! 那份带迁移逻辑，而 app 编译的是没有迁移逻辑的那份，导致升级后收藏、历史
//! 和播放会话全部读不到。现在只保留一份实现，这里只做再导出。

pub use voicefox_runtime::storage::*;
