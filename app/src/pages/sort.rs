use std::cmp::Ordering;

use lx_core::model::config::TableColumnConfig;
use lx_core::model::song::{EXTRA_FILE_MODIFIED_UNIX_NANOS, SongInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortTarget {
    Favorites,
    History,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Newest,
    Oldest,
    TitleAsc,
    TitleDesc,
    ArtistAsc,
    AlbumAsc,
    DurationAsc,
    DurationDesc,
    SourceAsc,
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            Self::Newest => Self::Oldest,
            Self::Oldest => Self::TitleAsc,
            Self::TitleAsc => Self::TitleDesc,
            Self::TitleDesc => Self::ArtistAsc,
            Self::ArtistAsc => Self::AlbumAsc,
            Self::AlbumAsc => Self::DurationAsc,
            Self::DurationAsc => Self::DurationDesc,
            Self::DurationDesc => Self::SourceAsc,
            Self::SourceAsc => Self::Newest,
        }
    }

    pub fn label(self, target: SortTarget) -> &'static str {
        match (self, target) {
            (Self::Newest, SortTarget::Favorites) => "最新收藏",
            (Self::Newest, SortTarget::History) => "最近播放",
            (Self::Newest, SortTarget::Local) => "最新修改",
            (Self::Oldest, SortTarget::Favorites) => "最早收藏",
            (Self::Oldest, SortTarget::History) => "最早播放",
            (Self::Oldest, SortTarget::Local) => "最早修改",
            (Self::TitleAsc, _) => "名称升序",
            (Self::TitleDesc, _) => "名称降序",
            (Self::ArtistAsc, _) => "歌手升序",
            (Self::AlbumAsc, _) => "专辑升序",
            (Self::DurationAsc, _) => "时长升序",
            (Self::DurationDesc, _) => "时长降序",
            (Self::SourceAsc, SortTarget::Local) => "路径升序",
            (Self::SourceAsc, _) => "来源升序",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SortState {
    pub selected: usize,
    pub scroll: usize,
    pub mode: SortMode,
    pub page_key: &'static str,
    pub columns: Vec<TableColumnConfig>,
    pub column_resize: Option<crate::pages::components::song_table::ColumnResizeState>,
}

impl SortState {
    /// 兜底取消进行中的列宽拖拽（鼠标在列表外松开、切页、终端 resize 等）。
    ///
    /// 不做这件事的话，`column_resize` 会永久停在 `Some`，而各页面的
    /// 拖拽分支对其它鼠标事件是 `return None` —— 该页从此点不动。
    pub fn cancel_column_resize(&mut self) {
        self.column_resize = None;
    }

    pub fn new(mode: SortMode, page_key: &'static str) -> Self {
        Self {
            selected: 0,
            scroll: 0,
            mode,
            page_key,
            columns: Vec::new(),
            column_resize: None,
        }
    }

    pub fn cycle(&mut self) -> SortMode {
        self.mode = self.mode.next();
        self.reset_position();
        self.mode
    }

    pub fn reset_position(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }
}

/// 排序列表缓存：只在数据版本或排序方式变化时重新拉取并排序。
///
/// 列表页每帧渲染都会读取整份数据（本地曲库 / 收藏 / 历史），
/// 直接 clone + 排序会让内存水位被高频分配顶高；本缓存把
/// “数据没变就不重建”的逻辑收敛到一处，渲染路径退化为零拷贝。
#[derive(Default)]
pub struct SortedListCache {
    version: u64,
    mode: Option<SortMode>,
    songs: Vec<SongInfo>,
}

impl SortedListCache {
    /// 返回与 `version` / `mode` 匹配的已排序列表；缓存命中时直接借用，
    /// 未命中时才调用 `source` 拉取数据并排序。
    pub fn get_or_build(
        &mut self,
        version: u64,
        mode: SortMode,
        target: SortTarget,
        source: impl FnOnce() -> Vec<SongInfo>,
    ) -> &[SongInfo] {
        if self.version != version || self.mode != Some(mode) {
            self.version = version;
            self.mode = Some(mode);
            self.songs = sorted_songs(source(), mode, target);
        }
        &self.songs
    }
}

/// 返回排序后的下标。`sorted_songs` 基于本函数实现；页面渲染统一走
/// `SortedListCache`，本函数目前仅由测试直接使用。
#[allow(dead_code)]
pub fn sorted_indices(songs: &[SongInfo], mode: SortMode, target: SortTarget) -> Vec<usize> {
    let mut indices = (0..songs.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        compare_songs(&songs[*left], *left, &songs[*right], *right, mode, target)
    });
    indices
}

pub fn sorted_songs(songs: Vec<SongInfo>, mode: SortMode, target: SortTarget) -> Vec<SongInfo> {
    let mut indexed = songs.into_iter().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        compare_songs(left, *left_index, right, *right_index, mode, target)
    });
    indexed.into_iter().map(|(_, song)| song).collect()
}

fn compare_songs(
    left: &SongInfo,
    left_index: usize,
    right: &SongInfo,
    right_index: usize,
    mode: SortMode,
    target: SortTarget,
) -> Ordering {
    let order = match mode {
        SortMode::Newest => match target {
            SortTarget::Favorites => right_index.cmp(&left_index),
            SortTarget::History => left_index.cmp(&right_index),
            SortTarget::Local => modified_nanos(right)
                .cmp(&modified_nanos(left))
                .then_with(|| compare_text(&left.name, &right.name))
                .then_with(|| left.file_path.cmp(&right.file_path)),
        },
        SortMode::Oldest => match target {
            SortTarget::Favorites => left_index.cmp(&right_index),
            SortTarget::History => right_index.cmp(&left_index),
            SortTarget::Local => modified_nanos(left)
                .cmp(&modified_nanos(right))
                .then_with(|| compare_text(&left.name, &right.name))
                .then_with(|| left.file_path.cmp(&right.file_path)),
        },
        SortMode::TitleAsc => compare_text(&left.name, &right.name)
            .then_with(|| compare_text(&left.singer, &right.singer)),
        SortMode::TitleDesc => compare_text(&right.name, &left.name)
            .then_with(|| compare_text(&right.singer, &left.singer)),
        SortMode::ArtistAsc => compare_text(&left.singer, &right.singer)
            .then_with(|| compare_text(&left.name, &right.name)),
        SortMode::AlbumAsc => compare_text(&left.album_name, &right.album_name)
            .then_with(|| compare_text(&left.name, &right.name)),
        SortMode::DurationAsc => left
            .duration
            .cmp(&right.duration)
            .then_with(|| compare_text(&left.name, &right.name)),
        SortMode::DurationDesc => right
            .duration
            .cmp(&left.duration)
            .then_with(|| compare_text(&left.name, &right.name)),
        SortMode::SourceAsc => match target {
            SortTarget::Local => left
                .file_path
                .cmp(&right.file_path)
                .then_with(|| compare_text(&left.name, &right.name)),
            SortTarget::Favorites | SortTarget::History => left
                .source
                .as_str()
                .cmp(right.source.as_str())
                .then_with(|| compare_text(&left.name, &right.name)),
        },
    };
    order.then_with(|| left_index.cmp(&right_index))
}

fn modified_nanos(song: &SongInfo) -> u128 {
    song.extra
        .get(EXTRA_FILE_MODIFIED_UNIX_NANOS)
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

/// 逐字符小写后比较：与 `str::to_lowercase()` 不同，**不分配**。
///
/// 列表排序的比较器里调用 `to_lowercase()` 会产生 `O(n log n)` 次临时字符串
/// （每帧重排一次列表时尤其明显），这里统一用免分配版本。
pub fn compare_text(left: &str, right: &str) -> Ordering {
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use lx_core::model::song::{EXTRA_FILE_MODIFIED_UNIX_NANOS, SongInfo};
    use lx_core::model::source::SourceId;

    use super::{SortMode, SortTarget, sorted_indices, sorted_songs};

    fn song(id: &str, name: &str, singer: &str, duration: u64) -> SongInfo {
        let mut song = SongInfo::new(
            id.to_string(),
            SourceId::Kw,
            name.to_string(),
            singer.to_string(),
        );
        song.duration = Duration::from_secs(duration);
        song
    }

    #[test]
    fn favorites_and_history_use_their_storage_order_for_recency() {
        let songs = vec![
            song("1", "A", "X", 1),
            song("2", "B", "Y", 2),
            song("3", "C", "Z", 3),
        ];

        assert_eq!(
            sorted_indices(&songs, SortMode::Newest, SortTarget::Favorites),
            vec![2, 1, 0]
        );
        assert_eq!(
            sorted_indices(&songs, SortMode::Newest, SortTarget::History),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn local_recency_uses_cached_file_modification_time() {
        let mut older = song("1", "older", "X", 1);
        older.extra.insert(
            EXTRA_FILE_MODIFIED_UNIX_NANOS.to_string(),
            "100".to_string(),
        );
        let mut newer = song("2", "newer", "Y", 2);
        newer.extra.insert(
            EXTRA_FILE_MODIFIED_UNIX_NANOS.to_string(),
            "200".to_string(),
        );

        let sorted = sorted_songs(vec![older, newer], SortMode::Newest, SortTarget::Local);

        assert_eq!(sorted[0].id, "2");
    }

    #[test]
    fn title_and_duration_modes_sort_the_visible_song_list() {
        let songs = vec![song("1", "beta", "X", 90), song("2", "Alpha", "Y", 30)];

        let by_title = sorted_songs(songs.clone(), SortMode::TitleAsc, SortTarget::Favorites);
        let by_duration = sorted_songs(songs, SortMode::DurationDesc, SortTarget::Favorites);

        assert_eq!(by_title[0].name, "Alpha");
        assert_eq!(by_duration[0].duration, Duration::from_secs(90));
    }
}
