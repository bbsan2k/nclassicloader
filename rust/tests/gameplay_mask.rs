//! An alternative host needs only held-button snapshots, not SDL key types.
use clover_portable::frontend::gameplay::button_mask;

#[test]
fn snapshots_match_the_legacy_combination_for_all_keyboard_masks() {
    for keyboard_mask in 0_u16..4096 {
        let keyboard = std::array::from_fn(|bit| keyboard_mask & (1 << bit) != 0);
        let pad_mask = keyboard_mask.rotate_left(3) & 0x0fff;
        let pad = std::array::from_fn(|bit| pad_mask & (1 << bit) != 0);
        for directions in 0..16 {
            let stick = directions << 4;
            for blocked in [false, true] {
                let expected = keyboard_mask | if blocked { 0 } else { pad_mask | stick };
                assert_eq!(button_mask(keyboard, pad, stick, blocked), expected);
            }
        }
    }
}
