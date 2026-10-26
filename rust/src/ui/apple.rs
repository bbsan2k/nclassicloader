//! Callback adapter for the real Classic renderer/navigation. It does not run a
//! second event loop, own a core, or use the desktop audio-driven game pacer.
use super::*;
use crate::frontend::menu_input::{Key, KeyEvent};
use std::time::Duration;

pub(crate) struct ClassicMenu {
    app: App,
    previous: u16,
    repeat_at: Instant,
    frames: u64,
    blocked: bool,
}

impl ClassicMenu {
    pub(crate) fn new(
        canvas: &Canvas<Window>,
        sdl: &sdl3::Sdl,
        assets: &Path,
        data: &Path,
        cache: &Path,
        home: &Path,
    ) -> Result<Self, String> {
        let subsystem = sdl.gamepad().map_err(|e| e.to_string())?;
        let app = App::new_with_embedding(
            canvas,
            assets.to_owned(),
            data.to_owned(),
            subsystem,
            cache.to_owned(),
            home.to_owned(),
            true,
        )?;
        Ok(Self {
            app,
            previous: 0,
            repeat_at: Instant::now(),
            frames: 0,
            blocked: false,
        })
    }
    pub(crate) fn draw(&mut self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        self.app.render_menu(canvas);
        self.frames += 1;
        if self.app.panel == "notice" && self.app.scroll_started.elapsed().as_secs_f32() > 2.5 {
            self.app.panel.clear();
        }
        Ok(())
    }
    pub(crate) fn block(&mut self, blocked: bool) {
        self.blocked = blocked;
        self.previous = 0;
        self.app.held.clear();
        self.app.pad_held.clear();
        self.app.stick.clear();
        self.app.pad_repeat.clear();
        if blocked {
            self.app.embedded_launch = None;
            self.app.embedded_exit = false;
        }
    }
    pub(crate) fn key(&mut self, key: Key, repeat: bool, canvas: &Canvas<Window>) {
        if self.blocked {
            return;
        }
        if self.app.panel.is_empty() && self.app.menu_focus == MenuFocus::Games {
            if key == Key::Back && !repeat {
                self.app.embedded_exit = true;
                return;
            }
            if key == Key::Down {
                self.error("Suspend slots are not yet connected to the Apple game host.".into());
                return;
            }
        }
        self.app.handle_menu_input(KeyEvent { key, repeat }, canvas);
    }
    pub(crate) fn mask(&mut self, mask: u16, canvas: &Canvas<Window>) {
        if self.blocked {
            self.previous = 0;
            return;
        }
        let pressed = mask & !self.previous;
        let direction = mask & (16 | 32 | 64 | 128);
        let repeating = direction != 0 && mask == self.previous && Instant::now() >= self.repeat_at;
        for (bit, key) in [
            (16, Key::Up),
            (32, Key::Down),
            (64, Key::Left),
            (128, Key::Right),
            (1, Key::Confirm),
            (256, Key::Back),
            (8, Key::Confirm),
            (4, Key::Next),
        ] {
            if pressed & bit != 0 || (repeating && direction & bit != 0) {
                self.key(key, repeating, canvas);
            }
        }
        if mask != self.previous {
            self.repeat_at = Instant::now() + Duration::from_millis(350);
        } else if repeating {
            self.repeat_at = Instant::now() + Duration::from_millis(100);
        }
        self.previous = mask;
    }
    pub(crate) fn point(&mut self, x: f32, y: f32, canvas: &Canvas<Window>) {
        if self.blocked {
            return;
        }
        let action =
            crate::frontend::menu_frame::last_hit(&self.app.hits, x, y, |h| from_frect(h.rect))
                .map(|h| h.action.clone());
        if let Some(action) = action {
            self.app.activate(&action, canvas);
        }
    }
    pub(crate) fn take_action(&mut self) -> Value {
        if std::mem::take(&mut self.app.embedded_exit) {
            return json!({"action":"collections"});
        }
        if let Some(id) = self.app.embedded_launch.take() {
            return json!({"action":"game","game":id});
        }
        Value::Null
    }
    pub(crate) fn error(&mut self, message: String) {
        self.app.panel = "message".into();
        self.app.message = message;
    }
    pub(crate) fn report(&self) -> Value {
        json!({"kind":"rust-classic","frames":self.frames,"selected":self.app.selected().id,
        "game_count":self.app.games.len(),"panel":self.app.panel,"assets":self.app.assets_path,"embedded_core_loaded":self.app.core.is_some()})
    }
    pub(crate) fn finish(&mut self) -> Result<(), String> {
        self.app.save_settings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(canvas: &Canvas<Window>) -> Vec<u8> {
        let surface = canvas
            .read_pixels(None)
            .unwrap()
            .convert_format(PixelFormat::RGBA32)
            .unwrap();
        let mut output = Vec::new();
        surface.with_lock(|bytes| {
            for row in 0..surface.height() as usize {
                let start = row * surface.pitch() as usize;
                output.extend_from_slice(&bytes[start..start + surface.width() as usize * 4]);
            }
        });
        output
    }
    #[test]
    #[ignore = "requires caller SDL dummy/software drivers and CLOVER_CLASSIC_ASSETS"]
    fn embedded_menu_matches_desktop_and_never_creates_a_core() {
        assert_eq!(std::env::var("SDL_VIDEODRIVER").as_deref(), Ok("dummy"));
        let assets = PathBuf::from(
            std::env::var("CLOVER_CLASSIC_ASSETS").expect("caller-prepared Classic assets"),
        );
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".build/apple-classic-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let _owned = Owned(root.clone());
        for name in ["desktop", "embedded"] {
            fs::create_dir(root.join(name)).unwrap();
            fs::write(
                root.join(name).join("preferences.json"),
                b"{\"sound\":false,\"motion\":false}",
            )
            .unwrap();
        }
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let mut canvas = video
            .window("Classic adapter check", 1280, 720)
            .build()
            .unwrap()
            .into_canvas();
        let mut desktop = App::new(
            &canvas,
            assets.clone(),
            root.join("desktop"),
            sdl.gamepad().unwrap(),
            root.join("profiles"),
            root.clone(),
        )
        .unwrap();
        let mut embedded = ClassicMenu::new(
            &canvas,
            &sdl,
            &assets,
            &root.join("embedded"),
            &root.join("profiles"),
            &root,
        )
        .unwrap();
        for panel in ["", "settings", "help"] {
            desktop.panel = panel.into();
            embedded.app.panel = panel.into();
            canvas.set_draw_color(Color::BLACK);
            canvas.clear();
            desktop.render_menu(&mut canvas);
            let expected = pixels(&canvas);
            canvas.clear();
            embedded.draw(&mut canvas).unwrap();
            assert_eq!(pixels(&canvas), expected, "same Classic renderer: {panel}");
        }
        embedded.app.panel.clear();
        let original = embedded.app.selected().id.clone();
        embedded.key(Key::Right, false, &canvas);
        assert_ne!(embedded.app.selected().id, original);
        embedded.key(Key::Left, false, &canvas);
        assert_eq!(embedded.app.selected().id, original);
        assert!(embedded.take_action().is_null());
        assert!(embedded.app.core.is_none());
        embedded.key(Key::Confirm, false, &canvas);
        assert_eq!(
            embedded.take_action(),
            json!({"action":"game","game":original})
        );
        assert!(embedded.app.core.is_none());
        embedded.key(Key::Confirm, false, &canvas);
        embedded.block(true);
        embedded.key(Key::Confirm, false, &canvas);
        assert!(embedded.take_action().is_null());
        embedded.block(false);
        embedded.key(Key::Back, false, &canvas);
        assert_eq!(embedded.take_action(), json!({"action":"collections"}));
        embedded.key(Key::Down, false, &canvas);
        assert_eq!(embedded.app.panel, "message");
        embedded.key(Key::Back, false, &canvas);
        for action in [
            "rom",
            "firmware",
            "import-image",
            "resume-panel",
            "resume",
            "delete",
            "confirm-save",
        ] {
            embedded.app.activate(action, &canvas);
            assert_eq!(
                embedded.app.panel, "message",
                "unsupported action: {action}"
            );
            assert!(embedded.take_action().is_null());
            assert!(embedded.app.core.is_none());
            embedded.key(Key::Back, false, &canvas);
        }
        embedded.draw(&mut canvas).unwrap();
        let start = embedded
            .app
            .hits
            .iter()
            .find(|hit| hit.action == "start")
            .expect("Classic start hit")
            .rect;
        embedded.point(start.x + start.w / 2.0, start.y + start.h / 2.0, &canvas);
        assert_eq!(
            embedded.take_action(),
            json!({"action":"game","game":original})
        );
        assert!(embedded.app.core.is_none());
        embedded.finish().unwrap();
    }
}
