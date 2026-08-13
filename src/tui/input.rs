//! A minimal single-line text input for the filter and profile-picker
//! fields, plus the multi-line editor of the run-command form.

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

/// A minimal multi-line editor: enter splits the line, backspace at a line
/// start joins, arrows/home/end move. Enough for typing a short script; not
/// a text editor.
#[derive(Debug, Clone)]
pub struct MultiInput {
    lines: Vec<String>,
    row: usize,
    col: usize, // byte offset into lines[row], always on a char boundary
}

impl Default for MultiInput {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }
}

impl MultiInput {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// (row, column-in-chars) of the cursor, for rendering.
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.lines[self.row][..self.col].chars().count())
    }

    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.trim().is_empty())
    }

    /// Inserts text (e.g. a paste) at the cursor; embedded newlines split.
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            if c == '\n' {
                self.split_line();
            } else if c != '\r' {
                self.lines[self.row].insert(self.col, c);
                self.col += c.len_utf8();
            }
        }
    }

    fn split_line(&mut self) {
        let rest = self.lines[self.row].split_off(self.col);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
    }

    /// Applies a key. Returns true when it consumed the key.
    pub fn handle(&mut self, k: &KeyEvent) -> bool {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Enter => self.split_line(),
            KeyCode::Char(c) if !ctrl && !k.modifiers.contains(KeyModifiers::ALT) => {
                self.lines[self.row].insert(self.col, c);
                self.col += c.len_utf8();
            }
            KeyCode::Backspace => {
                if let Some((i, _)) = self.lines[self.row][..self.col].char_indices().next_back() {
                    self.lines[self.row].remove(i);
                    self.col = i;
                } else if self.row > 0 {
                    // at a line start: join with the previous line
                    let cur = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                    self.lines[self.row].push_str(&cur);
                }
            }
            KeyCode::Left => {
                if let Some((i, _)) = self.lines[self.row][..self.col].char_indices().next_back() {
                    self.col = i;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                }
            }
            KeyCode::Right => {
                if let Some(c) = self.lines[self.row][self.col..].chars().next() {
                    self.col += c.len_utf8();
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            KeyCode::Up if self.row > 0 => {
                self.row -= 1;
                self.clamp_col();
            }
            KeyCode::Down if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.clamp_col();
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.lines[self.row].len(),
            _ => return false,
        }
        true
    }

    /// Keeps the byte column on a char boundary after a vertical move.
    fn clamp_col(&mut self) {
        let line = &self.lines[self.row];
        if self.col >= line.len() {
            self.col = line.len();
            return;
        }
        while !line.is_char_boundary(self.col) {
            self.col -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn multiline_edit_split_and_join() {
        let mut e = MultiInput::default();
        assert!(e.is_blank());
        e.insert_str("echo hi");
        e.handle(&key(KeyCode::Enter));
        e.insert_str("uptime");
        assert_eq!(e.lines(), ["echo hi", "uptime"]);
        assert!(!e.is_blank());
        // backspace at the start of line 2 joins the lines
        e.handle(&key(KeyCode::Home));
        e.handle(&key(KeyCode::Backspace));
        assert_eq!(e.lines(), ["echo hiuptime"]);
        assert_eq!(e.cursor(), (0, 7));
        // pasting embedded newlines splits again
        e.handle(&key(KeyCode::End));
        e.insert_str("\ndf -h\nfree");
        assert_eq!(e.lines(), ["echo hiuptime", "df -h", "free"]);
        // vertical moves clamp the column
        e.handle(&key(KeyCode::Up));
        assert_eq!(e.cursor(), (1, 4));
        e.handle(&key(KeyCode::Up));
        e.handle(&key(KeyCode::Down));
        assert_eq!(e.cursor().0, 1);
    }
}
