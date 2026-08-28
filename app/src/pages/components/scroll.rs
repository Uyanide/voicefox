//! 列表滚动辅助 — 让选中行始终落在可视窗口内。

/// 调整 `offset`，保证 `selected` 行落在 `visible` 高度的窗口内。
///
/// `visible`/`total` 为 0 时把偏移归零；末尾不足一屏时向下对齐。
/// leaderboard 与 playlists 等列表页共用，避免逐字重复实现。
pub fn ensure_visible(selected: usize, visible: usize, total: usize, offset: &mut usize) {
    if visible == 0 || total == 0 {
        *offset = 0;
        return;
    }
    if selected >= offset.saturating_add(visible) {
        *offset = selected.saturating_sub(visible - 1);
    } else if selected < *offset {
        *offset = selected;
    }
    *offset = (*offset).min(total.saturating_sub(visible));
}

#[cfg(test)]
mod tests {
    use super::ensure_visible;

    #[test]
    fn keeps_selection_inside_the_window() {
        let mut offset = 0;
        ensure_visible(12, 10, 30, &mut offset);
        assert_eq!(offset, 3);
        ensure_visible(1, 10, 30, &mut offset);
        assert_eq!(offset, 1);
        ensure_visible(5, 10, 30, &mut offset);
        assert_eq!(offset, 1);
    }

    #[test]
    fn empty_list_resets_offset() {
        let mut offset = 7;
        ensure_visible(0, 0, 10, &mut offset);
        assert_eq!(offset, 0);
        ensure_visible(0, 10, 0, &mut offset);
        assert_eq!(offset, 0);
    }

    #[test]
    fn aligns_to_the_last_full_window() {
        let mut offset = 0;
        ensure_visible(29, 10, 30, &mut offset);
        assert_eq!(offset, 20);
    }
}
