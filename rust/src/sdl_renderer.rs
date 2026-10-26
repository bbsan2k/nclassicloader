//! SDL leaf execution shared by the desktop adapter and Apple preview.
//! Callers must resolve texture IDs against their current owner before supplying
//! a texture; this function does not grant resource or action authority.

use sdl3::{
    pixels::Color,
    render::{Canvas, ScaleMode, Texture},
    video::Window,
};

pub(crate) fn draw_command(
    canvas: &mut Canvas<Window>,
    command: &crate::frontend::menu_scene::Command,
    texture: Option<&mut Texture>,
) -> bool {
    use crate::frontend::menu_scene::{Command, Filter};

    match command {
        Command::Fill { rect, color } => {
            canvas.set_draw_color(Color::RGBA(color.r, color.g, color.b, color.a));
            let _ = canvas.fill_rect(sdl3::render::FRect::new(rect.x, rect.y, rect.w, rect.h));
            true
        }
        Command::Texture(quad) => {
            let Some(texture) = texture else {
                return false;
            };
            texture.set_color_mod(quad.tint.r, quad.tint.g, quad.tint.b);
            texture.set_alpha_mod(quad.tint.a);
            texture.set_scale_mode(match quad.filter {
                Filter::Nearest => ScaleMode::Nearest,
                Filter::Linear => ScaleMode::Linear,
            });
            let destination = sdl3::render::FRect::new(
                quad.destination.x,
                quad.destination.y,
                quad.destination.w,
                quad.destination.h,
            );
            let source = quad
                .source
                .map(|rect| sdl3::render::FRect::new(rect.x, rect.y, rect.w, rect.h));
            if quad.flip_x {
                let _ = canvas.copy_ex(texture, source, destination, 0.0, None, true, false);
            } else {
                let _ = canvas.copy(texture, source, destination);
            }
            true
        }
    }
}
