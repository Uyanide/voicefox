/// 供实现 `MusicSource` 等 trait 的外部/下游 crate 复用同一版本的过程宏。
pub use async_trait;

pub mod events;
pub mod keybinding;
pub mod model;
pub mod traits;
