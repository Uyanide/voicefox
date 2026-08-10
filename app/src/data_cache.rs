//! 数据层缓存：列表页渲染只读快照，不在渲染路径访问数据源。
//!
//! 各列表页（本地音乐 / 收藏 / 历史）每帧渲染都需要读取整份数据并排序，
//! 直接访问数据源会造成每帧 clone + sort。这里把“已排序快照”集中缓存，
//! 数据代次或排序方式变化时才重建一次，渲染路径退化为零拷贝借用。

use crate::pages::sort::SortedListCache;

/// 集中式列表缓存，按列表类型各自保存一份已排序快照。
#[derive(Default)]
pub struct DataCache {
    pub local: SortedListCache,
    pub favorites: SortedListCache,
    pub history: SortedListCache,
}
