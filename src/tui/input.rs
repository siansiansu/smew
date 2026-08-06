//! A minimal single-line text input for the filter and profile-picker fields.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Default, Clone)]
pub struct Input {
    value: String,
    cursor: usize, // byte offset, always on a char boundary
}

impl Input {
    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Inserts text (e.g. a paste) at the cursor.
    pub fn insert_str(&mut self, s: &str) {
        self.value.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Applies a key to the input. Returns true when it consumed the key.
    pub fn handle(&mut self, k: &KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Char('u') if ctrl => {
                self.value.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => self.delete_word_back(),
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.value.len(),
            KeyCode::Char(c) if !ctrl && !k.modifiers.contains(KeyModifiers::ALT) => {
                self.value.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if let Some((i, _)) = self.value[..self.cursor].char_indices().next_back() {
                    self.value.remove(i);
                    self.cursor = i;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.value.len() {
                    self.value.remove(self.cursor);
                }
            }
            KeyCode::Left => {
                if let Some((i, _)) = self.value[..self.cursor].char_indices().next_back() {
                    self.cursor = i;
                }
            }
            KeyCode::Right => {
                if let Some(c) = self.value[self.cursor..].chars().next() {
                    self.cursor += c.len_utf8();
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.value.len(),
            _ => return false,
        }
        true
    }

    /// Splits the value at the cursor for rendering: (prefix, cursor_char,
    /// suffix) — the cursor char is styled reversed by the caller.
    pub fn render_parts(&self) -> (&str, String, &str) {
        let before = &self.value[..self.cursor];
        match self.value[self.cursor..].chars().next() {
            Some(c) => {
                let end = self.cursor + c.len_utf8();
                (before, c.to_string(), &self.value[end..])
            }
            None => (before, " ".to_string(), ""),
        }
    }

    fn delete_word_back(&mut self) {
        let before = &self.value[..self.cursor];
        let trimmed = before.trim_end();
        let cut = trimmed
            .rfind(char::is_whitespace)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.value.replace_range(cut..self.cursor, "");
        self.cursor = cut;
    }
}
