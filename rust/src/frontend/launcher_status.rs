//! Pure status-line wrapping policy for the native launcher's font-measurement
//! handshake.

use super::launcher_scene::{DrawCommand, FontRole, Rgba, TextCommand};

const MUTED: Rgba = Rgba {
    r: 156,
    g: 171,
    b: 186,
    a: 255,
};
const ERROR: Rgba = Rgba {
    r: 255,
    g: 163,
    b: 156,
    a: 255,
};

/// Incremental status wrapping policy. The host measures each candidate with
/// its existing font and advances by that scalar width; emitted text is owned.
pub struct StatusText<'a> {
    words: std::str::SplitWhitespace<'a>,
    current_word: Option<&'a str>,
    line: String,
    candidate: String,
    y: f32,
    error: bool,
    stopped: bool,
}

impl<'a> StatusText<'a> {
    pub fn new(status: &'a str, error: bool) -> Self {
        let mut text = Self {
            words: status.split_whitespace(),
            current_word: None,
            line: String::new(),
            candidate: String::new(),
            y: 657.0,
            error,
            stopped: false,
        };
        text.prepare_candidate();
        text
    }

    /// The exact next legacy candidate, including its leading space when the
    /// current line is empty.
    pub fn candidate(&self) -> Option<&str> {
        self.current_word.map(|_| self.candidate.as_str())
    }

    /// Consume the measured word, optionally emitting a completed prior line.
    pub fn advance(&mut self, measured_width: f32) -> Option<DrawCommand> {
        let word = self.current_word.take()?;
        let mut emitted = None;
        if measured_width > 550.0 && !self.line.is_empty() {
            emitted = Some(Self::command(
                self.error,
                std::mem::take(&mut self.line),
                self.y,
            ));
            self.y += 21.0;
            if self.y > 700.0 {
                self.stopped = true;
                self.candidate.clear();
                return emitted;
            }
        }
        if !self.line.is_empty() {
            self.line.push(' ');
        }
        self.line.push_str(word);
        self.prepare_candidate();
        emitted
    }

    /// Consume the policy and return the legacy terminal line, even if empty.
    pub fn finish(self) -> DrawCommand {
        Self::command(self.error, self.line, self.y)
    }

    fn prepare_candidate(&mut self) {
        if self.stopped {
            return;
        }
        self.current_word = self.words.next();
        if let Some(word) = self.current_word {
            self.candidate.clear();
            self.candidate.push_str(&self.line);
            self.candidate.push(' ');
            self.candidate.push_str(word);
        } else {
            self.candidate.clear();
        }
    }

    fn command(error: bool, value: String, y: f32) -> DrawCommand {
        DrawCommand::Text(TextCommand {
            value,
            x: 272.0,
            y,
            size: 15,
            font: FontRole::Regular,
            color: if error { ERROR } else { MUTED },
            max_width: 550.0,
        })
    }
}
