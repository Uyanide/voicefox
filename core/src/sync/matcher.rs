use super::model::{CanonicalSong, MatchConfidence, PlatformSongId, SyncSongMatch, normalize_text};
use crate::model::song::SongInfo;
use std::collections::{HashMap, HashSet};

/// 固定匹配优先级：ISRC > 平台 ID > 精确元数据 > 元数据+时长 > 模糊元数据。
pub fn match_songs(
    source: &[SongInfo],
    target: &[SongInfo],
    duration_tolerance_ms: u64,
    fuzzy_threshold: u16,
) -> Vec<SyncSongMatch> {
    let mut isrc = HashMap::<String, Vec<usize>>::new();
    let mut platform = HashMap::<PlatformSongId, Vec<usize>>::new();
    let mut exact = HashMap::<(String, String, String), Vec<usize>>::new();
    for (i, song) in target.iter().enumerate() {
        let c = CanonicalSong::from_song(song);
        if let Some(v) = c.isrc.as_ref() {
            isrc.entry(v.clone()).or_default().push(i);
        }
        platform
            .entry(PlatformSongId::from_song(song))
            .or_default()
            .push(i);
        exact
            .entry((c.title.clone(), c.artist.clone(), c.album.clone()))
            .or_default()
            .push(i);
    }
    let mut used = HashSet::new();
    source
        .iter()
        .map(|song| {
            let c = CanonicalSong::from_song(song);
            let found = c
                .isrc
                .as_ref()
                .and_then(|v| first_unused(isrc.get(v), &used))
                .map(|i| (i, MatchConfidence::Isrc))
                .or_else(|| {
                    first_unused(platform.get(&PlatformSongId::from_song(song)), &used)
                        .map(|i| (i, MatchConfidence::PlatformId))
                })
                .or_else(|| {
                    first_unused(
                        exact.get(&(c.title.clone(), c.artist.clone(), c.album.clone())),
                        &used,
                    )
                    .map(|i| (i, MatchConfidence::ExactMetadata))
                })
                .or_else(|| {
                    target
                        .iter()
                        .enumerate()
                        .filter(|(i, t)| {
                            !used.contains(i)
                                && duration_close(song, t, duration_tolerance_ms)
                                && metadata_score(song, t) >= 100
                        })
                        .map(|(i, _)| (i, MatchConfidence::MetadataDuration))
                        .next()
                })
                .or_else(|| {
                    target
                        .iter()
                        .enumerate()
                        .filter(|(i, t)| {
                            !used.contains(i) && metadata_score(song, t) >= fuzzy_threshold
                        })
                        .max_by_key(|(_, t)| metadata_score(song, t))
                        .map(|(i, _)| (i, MatchConfidence::FuzzyMetadata))
                });
            if let Some((i, confidence)) = found {
                used.insert(i);
                SyncSongMatch {
                    source: song.clone(),
                    target: Some(target[i].clone()),
                    confidence,
                }
            } else {
                SyncSongMatch {
                    source: song.clone(),
                    target: None,
                    confidence: MatchConfidence::Unmatched,
                }
            }
        })
        .collect()
}
fn first_unused(indices: Option<&Vec<usize>>, used: &HashSet<usize>) -> Option<usize> {
    indices
        .into_iter()
        .flatten()
        .copied()
        .find(|i| !used.contains(i))
}
fn duration_close(a: &SongInfo, b: &SongInfo, tolerance_ms: u64) -> bool {
    let a = a.duration.as_millis().try_into().unwrap_or(u64::MAX);
    let b = b.duration.as_millis().try_into().unwrap_or(u64::MAX);
    a.abs_diff(b) <= tolerance_ms
}
fn metadata_score(a: &SongInfo, b: &SongInfo) -> u16 {
    let title = similarity(&normalize_text(&a.name), &normalize_text(&b.name)) * 60 / 100;
    let artist = similarity(&normalize_text(&a.singer), &normalize_text(&b.singer)) * 30 / 100;
    let album = if !a.album_name.trim().is_empty()
        && !b.album_name.trim().is_empty()
        && normalize_text(&a.album_name) == normalize_text(&b.album_name)
    {
        10
    } else {
        0
    };
    title + artist + album
}
fn similarity(a: &str, b: &str) -> u16 {
    if a == b {
        return 100;
    }
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let common = a.chars().filter(|ch| b.contains(*ch)).count();
    ((common * 2 * 100) / (a.chars().count() + b.chars().count())).min(100) as u16
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::source::SourceId;
    use std::time::Duration;
    fn song(id: &str, source: SourceId, name: &str, artist: &str, ms: u64) -> SongInfo {
        let mut s = SongInfo::new(id.into(), source, name.into(), artist.into());
        s.duration = Duration::from_millis(ms);
        s
    }
    #[test]
    fn same_platform_id_wins() {
        let a = song("1", SourceId::Wy, "A", "B", 1000);
        let b = song("1", SourceId::Wy, "X", "Y", 9000);
        assert_eq!(
            match_songs(&[a], &[b], 5000, 92)[0].confidence,
            MatchConfidence::PlatformId
        );
    }
    #[test]
    fn exact_metadata_cross_platform() {
        let a = song("1", SourceId::Wy, "晴天", "周杰伦", 269000);
        let b = song("2", SourceId::Tx, "晴天", "周杰伦", 270000);
        assert_eq!(
            match_songs(&[a], &[b], 5000, 92)[0].confidence,
            MatchConfidence::ExactMetadata
        );
    }
    #[test]
    fn unmatched_is_explicit() {
        let a = song("1", SourceId::Wy, "A", "B", 1000);
        let b = song("2", SourceId::Tx, "C", "D", 1000);
        let r = match_songs(&[a], &[b], 5000, 92);
        assert!(r[0].target.is_none());
        assert_eq!(r[0].confidence, MatchConfidence::Unmatched);
    }
}
