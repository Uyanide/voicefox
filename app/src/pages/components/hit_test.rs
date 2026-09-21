//! Shared terminal hit-testing helpers. Rendering owns the Rect; mouse handling reuses it.
use ratatui::layout::{Position, Rect};

/// Return the absolute list index for a one-line-per-row list inside `area`.
/// `header_rows` is the number of non-list rows inside the block.
pub fn row_at(
    area: Rect,
    position: Position,
    scroll: usize,
    len: usize,
    header_rows: u16,
) -> Option<usize> {
    let inner = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .inner(area);
    let list = Rect::new(
        inner.x,
        inner.y.saturating_add(header_rows),
        inner.width,
        inner.height.saturating_sub(header_rows),
    );
    if !list.contains(position) {
        return None;
    }
    let index = scroll + position.y.saturating_sub(list.y) as usize;
    (index < len).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn row_hit_test_uses_the_same_rect_boundaries_as_rendering() {
        let area = Rect::new(10, 5, 40, 12);
        assert_eq!(row_at(area, Position::new(11, 7), 3, 20, 1), Some(3));
        assert_eq!(row_at(area, Position::new(10, 7), 3, 20, 1), None);
        assert_eq!(row_at(area, Position::new(20, 17), 3, 20, 1), None);
    }
}
