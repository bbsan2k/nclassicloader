//! Host-neutral assembly of held gameplay inputs into the SNES button layout.

pub const B: u16 = 1 << 0;
pub const Y: u16 = 1 << 1;
pub const SELECT: u16 = 1 << 2;
pub const START: u16 = 1 << 3;
pub const UP: u16 = 1 << 4;
pub const DOWN: u16 = 1 << 5;
pub const LEFT: u16 = 1 << 6;
pub const RIGHT: u16 = 1 << 7;
pub const A: u16 = 1 << 8;
pub const X: u16 = 1 << 9;
pub const L: u16 = 1 << 10;
pub const R: u16 = 1 << 11;

/// Assemble snapshots ordered B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R.
/// Stick inputs originate from `Stick::mask`;
/// only its four supported direction bits are accepted, without stripping other sources.
pub fn button_mask(
    keyboard: [bool; 12],
    gamepad: [bool; 12],
    stick_mask: u16,
    pad_blocked: bool,
) -> u16 {
    let keyboard_mask = keyboard
        .into_iter()
        .enumerate()
        .fold(0, |mask, (i, held)| mask | (u16::from(held) << i));
    let pad_mask = if pad_blocked {
        0
    } else {
        gamepad
            .into_iter()
            .enumerate()
            .fold(0, |mask, (i, held)| mask | (u16::from(held) << i))
            | (stick_mask & (UP | DOWN | LEFT | RIGHT))
    };
    keyboard_mask | pad_mask
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: [bool; 12] = [false; 12];

    #[test]
    fn each_bit_and_keyboard_pad_equivalence() {
        for i in 0..12 {
            let mut input = EMPTY;
            input[i] = true;
            assert_eq!(button_mask(input, EMPTY, 0, false), 1 << i);
            assert_eq!(button_mask(EMPTY, input, 0, false), 1 << i);
        }
    }

    #[test]
    fn neutral_combinations_nes_positions_and_stick() {
        assert_eq!(button_mask(EMPTY, EMPTY, 0, false), 0);
        assert_eq!(A, 1 << 8);
        assert_eq!(B, 1);
        let mut keys = EMPTY;
        keys[0] = true;
        keys[8] = true;
        let mut pad = EMPTY;
        pad[1] = true;
        assert_eq!(
            button_mask(keys, pad, UP | LEFT, false),
            B | Y | A | UP | LEFT
        );
        assert_eq!(button_mask(EMPTY, EMPTY, 0xf000, false), 0);
    }

    #[test]
    fn blocked_pad_suppresses_pad_and_stick_but_not_keyboard() {
        let mut keys = EMPTY;
        keys[8] = true;
        let mut pad = EMPTY;
        pad[0] = true;
        assert_eq!(button_mask(keys, pad, UP, true), A);
    }

    #[test]
    fn opposing_directions_are_combined() {
        let mut keys = EMPTY;
        keys[4] = true;
        let mut pad = EMPTY;
        pad[5] = true;
        assert_eq!(button_mask(keys, pad, 0, false), UP | DOWN);
    }
}
