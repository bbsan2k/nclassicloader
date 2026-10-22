//! Pure directional navigation for the toolbar's language grid.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguageInput {
    Up,
    Down,
    Left,
    Right,
    Next,
    Stay,
}

pub fn next_index(index: usize, count: usize, input: LanguageInput) -> usize {
    if count == 0 {
        return 0;
    }
    let index = index.min(count - 1);
    let next = match input {
        LanguageInput::Left if (4..8).contains(&index) => index - 4,
        LanguageInput::Right if index < 4 => index + 4,
        LanguageInput::Right if index == 8 => 7,
        LanguageInput::Up if index == 8 => 3,
        LanguageInput::Up if index != 0 && index != 4 => index - 1,
        LanguageInput::Down if index == 3 => 8,
        LanguageInput::Down if index != 7 && index != 8 => index + 1,
        LanguageInput::Next => (index + 1) % count,
        _ => index,
    };
    if next < count { next } else { index }
}

#[cfg(test)]
mod tests {
    use super::{LanguageInput::*, next_index};

    #[test]
    fn language_grid_arrows_follow_columns() {
        assert_eq!(next_index(0, 9, Right), 4);
        assert_eq!(next_index(4, 9, Left), 0);
        assert_eq!(next_index(3, 9, Down), 8);
        assert_eq!(next_index(4, 9, Up), 4);
        assert_eq!(next_index(7, 9, Down), 7);
    }

    #[test]
    fn legacy_single_language_cannot_escape_grid() {
        for input in [Up, Down, Left, Right, Next] {
            assert_eq!(next_index(0, 1, input), 0);
        }
    }

    #[test]
    fn empty_grid_and_invalid_index_are_clamped() {
        assert_eq!(next_index(99, 0, Right), 0);
        assert_eq!(next_index(99, 3, Stay), 2);
    }

    #[test]
    fn eight_and_nine_entry_grid_edges_are_preserved() {
        assert_eq!(next_index(3, 8, Down), 3);
        assert_eq!(next_index(7, 8, Right), 7);
        assert_eq!(next_index(8, 9, Right), 7);
        assert_eq!(next_index(8, 9, Up), 3);
    }

    #[test]
    fn next_wraps_and_stay_is_noop() {
        assert_eq!(next_index(8, 9, Next), 0);
        assert_eq!(next_index(2, 9, Stay), 2);
    }
}
