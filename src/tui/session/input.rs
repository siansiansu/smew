//! Key routing in the session view: the leader-prefixed commands, scroll
//! (copy) mode, and forwarding everything else to the pane.

use crossterm::event::KeyEvent;

use crate::tui::{ConfirmKind, Mode, Model, keymap};

impl Model {
    /// Routes keys in the session view. The leader prefix key starts a
    /// command: the next key drives scrollback / close / help. Everything
    /// else is forwarded to the pane.
    pub(crate) fn update_session(&mut self, k: &KeyEvent, s: &str) {
        // A pending leader command takes priority in every sub-state
        // (including scroll mode).
        if self.leader_pending {
            return self.leader_command(k, s);
        }

        // Scroll (copy) mode: navigate the pane's scrollback. The leader
        // still works; other keys are not forwarded to the shell.
        if self.scrolling {
            if s == self.leader {
                self.leader_pending = true;
                return;
            }
            return self.update_scroll(s);
        }

        if s == self.leader {
            self.leader_pending = true;
            return;
        }

        self.write_input(k);
    }

    /// Runs a session command (the key after the leader prefix).
    fn leader_command(&mut self, k: &KeyEvent, s: &str) {
        self.leader_pending = false;
        match s {
            "[" => {
                self.scrolling = true;
                self.scroll_offset = 0;
            }
            "x" | "d" => {
                self.confirm_action = ConfirmKind::CloseSession;
                self.mode = Mode::Confirm;
            }
            "?" => {
                self.overlay_scroll = 0;
                self.mode = Mode::Help;
            }
            // leader pressed twice → send a literal leader byte to the shell
            _ if s == self.leader => self.write_input(k),
            _ => {} // any other key after the leader is swallowed (cancel)
        }
    }

    /// Handles keys while in scroll (copy) mode.
    fn update_scroll(&mut self, s: &str) {
        let (max, page) = self.pane.as_ref().map_or((0, 10), |p| {
            let rows = p.rows() as usize;
            let page = if rows > 1 { rows - 1 } else { 10 };
            (p.scrollback_len(), page)
        });
        let off = self.scroll_offset;
        let next = match s {
            "up" | "k" => off.saturating_add(1),
            "down" | "j" => off.saturating_sub(1),
            "pgup" | "b" | "ctrl+b" => off.saturating_add(page),
            "pgdown" | "f" | "ctrl+f" | "space" => off.saturating_sub(page),
            "g" | "home" => max,
            "G" | "end" => 0,
            "esc" | "q" | "]" => {
                self.scrolling = false;
                self.scroll_offset = 0;
                return;
            }
            _ => off,
        };
        self.scroll_offset = next.min(max);
    }

    /// Sends a key to the pane. Arrows/home/end are encoded per the pane's
    /// own input modes (DECCKM).
    pub(crate) fn write_input(&mut self, k: &KeyEvent) {
        if let Some(p) = self.pane.as_ref().filter(|p| !p.is_done()) {
            let bytes = keymap::key_to_bytes(k, p.application_cursor());
            p.write(&bytes);
        }
    }

    /// Pastes text into the pane, honoring bracketed paste. Sent as one
    /// write so emulator reply bytes can't interleave between the paste
    /// markers and the body.
    pub(crate) fn session_paste(&mut self, s: &str) {
        let Some(p) = self.pane.as_ref().filter(|p| !p.is_done()) else {
            return;
        };
        let mut buf = Vec::with_capacity(s.len() + 12);
        if p.with_screen(0, |scr| scr.bracketed_paste()) {
            buf.extend_from_slice(b"\x1b[200~");
            buf.extend_from_slice(s.as_bytes());
            buf.extend_from_slice(b"\x1b[201~");
        } else {
            buf.extend_from_slice(s.as_bytes());
        }
        p.write(&buf);
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{argv, exited_pane, no_notify};
    use crate::session::Pane;
    use crate::tui::{Mode, test_model};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    // A pane that exits without any update cycle must not receive forwarded
    // input (defence in depth for the window before the reap runs).
    #[test]
    fn write_input_skips_done_pane() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.pane = Some(exited_pane());
        m.write_input(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)); // must not panic or block
    }

    // leader then x/d asks for confirmation instead of closing outright.
    #[test]
    fn leader_close_confirms() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.pane = Some(exited_pane());
        m.update_session(
            &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            "ctrl+b",
        );
        assert!(m.leader_pending);
        m.update_session(&KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE), "d");
        assert!(!m.leader_pending);
        assert_eq!(m.mode, Mode::Confirm);
        m.pane.take().unwrap().close();
    }

    fn wait_render(p: &Arc<Pane>, substr: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if p.contents_text().contains(substr) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    // Regression: F-keys and shift+tab must reach the pane's PTY with real
    // terminal encodings (an earlier keymap silently dropped them, making
    // htop's F1–F10 UI unusable). cat's TTY echoes input with
    // control bytes rendered visibly (ESC → ^[), letting us assert the exact
    // sequence that reached the PTY.
    #[test]
    fn function_keys_reach_pane() {
        let p = Pane::start("cat", &argv(&["cat"]), 80, 24, no_notify()).unwrap();
        let mut m = test_model();
        m.mode = Mode::Session;
        m.pane = Some(p.clone());

        let send = |m: &mut crate::tui::Model, code: KeyCode| {
            let k = KeyEvent::new(code, KeyModifiers::NONE);
            let s = crate::tui::keymap::key_name(&k);
            m.update_session(&k, &s);
        };

        send(&mut m, KeyCode::F(5));
        assert!(
            wait_render(&p, "^[[15~"),
            "F5 did not reach the PTY as ESC[15~:\n{}",
            p.contents_text()
        );
        send(&mut m, KeyCode::F(1));
        assert!(
            wait_render(&p, "^[OP"),
            "F1 did not reach the PTY as ESC OP:\n{}",
            p.contents_text()
        );
        send(&mut m, KeyCode::BackTab);
        assert!(
            wait_render(&p, "^[[Z"),
            "shift+tab did not reach the PTY as ESC[Z:\n{}",
            p.contents_text()
        );
        p.close();
    }

    // Navigation keys typed in the session view must reach a full-screen app
    // and move its view. Arrows are encoded per the app's input modes
    // (application cursor keys — DECCKM), which only the emulator knows.
    #[test]
    fn session_keys_reach_full_screen_app() {
        if !std::process::Command::new("sh")
            .args(["-c", "command -v less"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("skipping: less not installed");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.log");
        let mut content = String::new();
        for i in 1..=200 {
            content.push_str(&format!("line-{i:03}\n"));
        }
        std::fs::write(&file, content).unwrap();

        let script = format!("LESS= TERM=xterm-256color exec less {}", file.display());
        let p = Pane::start("less", &argv(&["sh", "-c", &script]), 80, 24, no_notify()).unwrap();
        let mut m = test_model();
        m.mode = Mode::Session;
        m.pane = Some(p.clone());

        assert!(
            wait_render(&p, "line-001"),
            "less never drew:\n{}",
            p.contents_text()
        );

        let send = |m: &mut crate::tui::Model, code: KeyCode, mods: KeyModifiers| {
            let k = KeyEvent::new(code, mods);
            let s = crate::tui::keymap::key_name(&k);
            m.update_session(&k, &s);
        };

        send(&mut m, KeyCode::Char('G'), KeyModifiers::SHIFT); // less: jump to end
        assert!(
            wait_render(&p, "line-200"),
            "G did not reach less:\n{}",
            p.contents_text()
        );

        send(&mut m, KeyCode::Char('g'), KeyModifiers::NONE); // back to top
        assert!(
            wait_render(&p, "line-001"),
            "g did not reach less:\n{}",
            p.contents_text()
        );

        // One line down from the top: line-024 appears (24-row screen shows
        // 23 content lines + the prompt).
        send(&mut m, KeyCode::Down, KeyModifiers::NONE);
        assert!(
            wait_render(&p, "line-024"),
            "down arrow did not scroll less:\n{}",
            p.contents_text()
        );
        send(&mut m, KeyCode::Up, KeyModifiers::NONE);
        assert!(
            wait_render(&p, "line-001"),
            "up arrow did not scroll back:\n{}",
            p.contents_text()
        );
        p.close();
    }
}
