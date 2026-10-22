use super::*;

impl App {
    pub(super) fn create_native_stage(&self) -> Result<Texture, String> {
        let mut stage = self
            .art
            .creator
            .create_texture(PixelFormat::RGBA8888, TextureAccess::Target, 1280, 720)
            .map_err(|error| error.to_string())?;
        stage.set_blend_mode(BlendMode::None);
        stage.set_scale_mode(ScaleMode::Linear);
        Ok(stage)
    }

    pub(super) fn draw_native_frame(
        &mut self,
        canvas: &mut Canvas<Window>,
        stage: &mut Texture,
    ) -> Result<(), String> {
        // Resolve atlas pixels on a fixed stage before scaling to Retina/window size.
        canvas
            .with_texture_canvas(stage, |canvas| {
                canvas.set_draw_color(Color::BLACK);
                canvas.clear();
                if self.playing && self.panel.is_empty() {
                    self.render_game(canvas);
                } else {
                    self.render_menu(canvas);
                }
            })
            .map_err(|error| error.to_string())?;
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        canvas
            .copy(stage, None, FRect::new(0.0, 0.0, 1280.0, 720.0))
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
