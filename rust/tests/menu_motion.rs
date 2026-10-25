use clover_portable::frontend::menu_motion::{self, MotionInput, MotionPlan};

fn input() -> MotionInput {
    MotionInput {
        suspend_layout: false,
        motion: true,
        animation_elapsed: 1.25,
        scroll_elapsed: 0.12,
        scroll_from: 2.0,
        scroll_to: 6.0,
        wallpaper_duration: 1.0,
    }
}

#[test]
fn motion_plan_preserves_normal_and_suspended_layout_and_timing() {
    assert_eq!(
        menu_motion::motion(input()),
        MotionPlan {
            title_shift: 0.0,
            card_shift: 0.0,
            scroll_current: 4.0,
            wallpaper_offset: 384.0,
            selection_time: 1.25,
        }
    );
    let mut suspended = input();
    suspended.suspend_layout = true;
    let plan = menu_motion::motion(suspended);
    assert_eq!((plan.title_shift, plan.card_shift), (-52.0, -69.0));
}

#[test]
fn scroll_interpolation_clamps_start_midpoint_and_end_and_disabled_motion_snaps() {
    let mut value = input();
    for (elapsed, expected) in [(-1.0, 2.0), (0.12, 4.0), (1.0, 6.0), (4.0, 6.0)] {
        value.scroll_elapsed = elapsed;
        assert_eq!(menu_motion::motion(value).scroll_current, expected);
    }
    value.motion = false;
    value.scroll_elapsed = 0.0;
    assert_eq!(menu_motion::motion(value).scroll_current, 6.0);
    assert_eq!(menu_motion::motion(value).selection_time, 0.5);
}

#[test]
fn wallpaper_wraps_and_retains_duration_floor_and_disabled_behavior() {
    let mut value = input();
    value.animation_elapsed = 2.25;
    assert_eq!(menu_motion::motion(value).wallpaper_offset, 384.0);
    value.animation_elapsed = 0.25;
    value.wallpaper_duration = 0.0;
    assert_eq!(menu_motion::motion(value).wallpaper_offset, 767.99994);
    value.wallpaper_duration = -2.0;
    assert_eq!(menu_motion::motion(value).wallpaper_offset, 767.99994);
    value.motion = false;
    assert_eq!(menu_motion::motion(value).wallpaper_offset, 0.0);
}

#[test]
fn arithmetic_keeps_nonfinite_results_unsanitized() {
    let mut value = input();
    value.scroll_elapsed = f32::NAN;
    assert!(menu_motion::motion(value).scroll_current.is_nan());
    value.scroll_elapsed = 0.0;
    value.animation_elapsed = f32::INFINITY;
    assert!(menu_motion::motion(value).wallpaper_offset.is_nan());
}
