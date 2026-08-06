//! Key handling: naming keys for dispatch (human-readable strings matching
//! config values like session_leader "ctrl+b") and encoding keys into the
//! byte sequences a terminal would send to a PTY.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Renders a key event as a readable name: "a", "G", "ctrl+b",
/// "space", "esc", "enter", "up", "pgdown", "shift+tab", "ctrl+ " …
/// Update dispatch matches on these names.
pub fn key_name(k: &KeyEvent) -> String {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let base = match k.code {
        KeyCode::Char(' ') => {
            if ctrl {
                return "ctrl+ ".into();
            }
            "space".into()
        }
        KeyCode::Char(c) => {
            if ctrl {
                return format!("ctrl+{}", c.to_lowercase());
            }
            if alt {
                return format!("alt+{c}");
            }
            c.to_string()
        }
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "shift+tab".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pgup".into(),
        KeyCode::PageDown => "pgdown".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => String::new(),
    };
    if ctrl && !base.is_empty() {
        return format!("ctrl+{base}");
    }
    base
}

/// Converts a key event into the byte sequence a terminal would send to a
/// PTY, so keystrokes can be forwarded to the remote shell. `app_cursor`
/// selects DECCKM application encoding for arrows/home/end — full-screen
/// apps (less, vim, htop…) enable it, and only the pane's emulator knows
/// whether it is active. F-keys / insert / shift+tab have fixed encodings.
pub fn key_to_bytes(k: &KeyEvent, app_cursor: bool) -> Vec<u8> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);

    // Control combos: ctrl+a..ctrl+z -> 0x01..0x1a.
    if ctrl && let KeyCode::Char(c) = k.code {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return vec![c as u8 - b'a' + 1];
        }
        return Vec::new();
    }

    let cursor = |app: &[u8], normal: &[u8]| {
        if app_cursor {
            app.to_vec()
        } else {
            normal.to_vec()
        }
    };

    match k.code {
        KeyCode::Char(c) => {
            let mut b = Vec::new();
            if alt {
                b.push(0x1b);
            }
            let mut chb = [0u8; 4];
            b.extend_from_slice(c.encode_utf8(&mut chb).as_bytes());
            b
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => cursor(b"\x1bOA", b"\x1b[A"),
        KeyCode::Down => cursor(b"\x1bOB", b"\x1b[B"),
        KeyCode::Right => cursor(b"\x1bOC", b"\x1b[C"),
        KeyCode::Left => cursor(b"\x1bOD", b"\x1b[D"),
        KeyCode::Home => cursor(b"\x1bOH", b"\x1b[H"),
        KeyCode::End => cursor(b"\x1bOF", b"\x1b[F"),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn strings() {
        assert_eq!(key_name(&key(KeyCode::Char('g'))), "g");
        assert_eq!(
            key_name(&KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
            "G"
        );
        assert_eq!(
            key_name(&KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
            "ctrl+b"
        );
        assert_eq!(
            key_name(&KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL)),
            "ctrl+ "
        );
        assert_eq!(key_name(&key(KeyCode::Char(' '))), "space");
        assert_eq!(key_name(&key(KeyCode::BackTab)), "shift+tab");
        assert_eq!(key_name(&key(KeyCode::PageDown)), "pgdown");
    }

    #[test]
    fn bytes_fixed() {
        assert_eq!(key_to_bytes(&key(KeyCode::F(5)), false), b"\x1b[15~");
        assert_eq!(key_to_bytes(&key(KeyCode::F(1)), false), b"\x1bOP");
        assert_eq!(key_to_bytes(&key(KeyCode::BackTab), false), b"\x1b[Z");
        assert_eq!(key_to_bytes(&key(KeyCode::Enter), false), b"\r");
        assert_eq!(
            key_to_bytes(
                &KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                false
            ),
            vec![0x03]
        );
        assert_eq!(
            key_to_bytes(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT), false),
            b"\x1bx"
        );
    }

    #[test]
    fn bytes_decckm() {
        assert_eq!(key_to_bytes(&key(KeyCode::Up), false), b"\x1b[A");
        assert_eq!(key_to_bytes(&key(KeyCode::Up), true), b"\x1bOA");
        assert_eq!(key_to_bytes(&key(KeyCode::Home), false), b"\x1b[H");
        assert_eq!(key_to_bytes(&key(KeyCode::End), true), b"\x1bOF");
    }
}
