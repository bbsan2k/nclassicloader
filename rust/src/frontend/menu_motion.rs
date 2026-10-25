/// Native menu motion values translated into a renderer-independent snapshot.
#[derive(Copy, Clone, Debug)]
pub struct MotionInput {
    pub suspend_layout: bool,
    pub motion: bool,
    pub animation_elapsed: f32,
    pub scroll_elapsed: f32,
    pub scroll_from: f32,
    pub scroll_to: f32,
    pub wallpaper_duration: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MotionPlan {
    pub title_shift: f32,
    pub card_shift: f32,
    pub scroll_current: f32,
    pub wallpaper_offset: f32,
    pub selection_time: f32,
}

pub fn motion(input: MotionInput) -> MotionPlan {
    let title_shift = if input.suspend_layout { -52.0 } else { 0.0 };
    let card_shift = if input.suspend_layout { -69.0 } else { 0.0 };
    let progress = (input.scroll_elapsed / 0.24).clamp(0.0, 1.0);
    let scroll_current = if input.motion {
        input.scroll_from + (input.scroll_to - input.scroll_from) * progress
    } else {
        input.scroll_to
    };
    let duration = input.wallpaper_duration.max(0.1);
    let wallpaper_offset = if input.motion {
        input.animation_elapsed.rem_euclid(duration) / duration * 1536.0
    } else {
        0.0
    };
    MotionPlan {
        title_shift,
        card_shift,
        scroll_current,
        wallpaper_offset,
        selection_time: if input.motion {
            input.animation_elapsed
        } else {
            0.5
        },
    }
}
