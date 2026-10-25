use super::*;

pub(super) fn draw_dialog(canvas: &mut Canvas<Window>, rect: crate::frontend::menu_scene::Rect) {
    canvas.set_draw_color(Color::BLACK);
    let _ = canvas.fill_rect(FRect::new(rect.x, rect.y, rect.w, rect.h));
    canvas.set_draw_color(Color::WHITE);
    for inset in 0..3 {
        let _ = canvas.draw_rect(FRect::new(
            rect.x + inset as f32,
            rect.y + inset as f32,
            rect.w - inset as f32 * 2.0,
            rect.h - inset as f32 * 2.0,
        ));
    }
}
