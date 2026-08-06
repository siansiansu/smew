//! Key routing in the session view: the leader-prefixed multiplexer
//! commands, broadcast groups, scroll (copy) mode, and input fan-out.

use std::sync::Arc;

use crossterm::event::KeyEvent;

use super::pane_key;
use crate::session::Pane;
use crate::tui::{ConfirmKind, Mode, Model, keymap, leader_label};

impl Model {
    /// Routes keys in the session view. The leader prefix key starts a
    /// command: the next key drives the multiplexer. Everything else is
    /// forwarded to the focused pane (or all panes when broadcast is on).
    pub(crate) fn update_session(&mut self, k: &KeyEvent, s: &str) {
        // A pending leader command takes priority in every sub-state
        // (including scroll mode), so zoom / focus / etc. work while
        // scrolling back.
        if self.leader_pending {
            return self.leader_command(k, s);
        }

        // Scroll (copy) mode: navigate the focused pane's scrollback. The
        // leader still works; other keys are not forwarded to the shell.
        if self.scrolling {
            if s == self.leader {
                self.leader_pending = true;
                return;
            }
            return self.update_scroll(s);
        }

        // Continuous focus navigation: after a leader focus command, arrow
        // keys keep moving focus without re-pressing the leader.
        if self.focus_nav {
            match s {
                "left" => return self.focus_prev(),
                "right" => return self.focus_next(),
                "up" => return self.focus_up(),
                "down" => return self.focus_down(),
                "space" => {
                    // toggle the focused pane's broadcast membership, stay in
                    // nav so you can move + toggle several panes in one flow
                    self.toggle_broadcast_member();
                    return;
                }
                "esc" | "enter" => {
                    self.focus_nav = false;
                    return;
                }
                _ => self.focus_nav = false, // any other key exits nav and is forwarded
            }
        }

        if s == self.leader {
            self.leader_pending = true;
            return;
        }

        self.write_input(k);
    }

    /// Moves focus to the previous pane, if any.
    fn focus_prev(&mut self) {
        if self.focus > 0 {
            self.focus -= 1;
            self.after_focus_change();
        }
    }

    /// Moves focus to the next pane, if any.
    fn focus_next(&mut self) {
        if self.focus + 1 < self.panes.len() {
            self.focus += 1;
            self.after_focus_change();
        }
    }

    /// Moves focus one grid row up (no-op in a single-row layout).
    fn focus_up(&mut self) {
        let (ncols, _) = self.grid_dims();
        if self.focus >= ncols {
            self.focus -= ncols;
            self.after_focus_change();
        }
    }

    /// Moves focus one grid row down, clamped to the last pane when the
    /// bottom row is shorter (no-op in a single-row layout).
    fn focus_down(&mut self) {
        let (ncols, nrows) = self.grid_dims();
        if self.focus / ncols + 1 < nrows {
            self.focus = (self.focus + ncols).min(self.panes.len().saturating_sub(1));
            self.after_focus_change();
        }
    }

    /// While broadcasting, `action` is blocked: sets a hint and returns true.
    fn broadcast_blocks(&mut self, action: &str) -> bool {
        if !self.broadcasting() {
            return false;
        }
        self.status = format!(
            "broadcasting — press {} b to clear the group before {action}",
            leader_label(&self.leader)
        );
        true
    }

    /// Runs a multiplexer command (the key after the leader prefix).
    fn leader_command(&mut self, k: &KeyEvent, s: &str) {
        self.leader_pending = false;
        match s {
            "h" | "left" | "p" => self.focus_prev(),
            "l" | "right" | "n" => self.focus_next(),
            "k" | "up" => self.focus_up(),
            "j" | "down" => self.focus_down(),
            "b" => {
                self.select_all_broadcast(); // toggle the whole group (all ↔ none)
                self.status.clear();
            }
            "space" => self.toggle_broadcast_member(),
            "z" => {
                if self.broadcast_blocks("zoom") {
                    return;
                }
                self.zoomed = !self.zoomed;
                self.relayout_session();
            }
            "v" => {
                self.layout = self.layout.next(); // columns → rows → grid → …
                self.relayout_session();
            }
            "[" => {
                self.scrolling = true;
                self.scroll_offset = 0;
            }
            "a" => {
                self.scrolling = false;
                self.scroll_offset = 0;
                self.adding_pane = true;
                self.mode = Mode::List;
                self.status = "adding pane: pick a host — s to add, esc to cancel".to_string();
            }
            "x" => {
                if self.broadcast_blocks("closing a pane") {
                    return;
                }
                self.close_focused_pane();
            }
            "d" => {
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

    /// How many open panes are in the broadcast group.
    pub(crate) fn broadcast_count(&self) -> usize {
        self.panes
            .iter()
            .filter(|p| self.broadcast_group.contains(&pane_key(p)))
            .count()
    }

    /// Whether input is fanned out: the group has ≥2 panes.
    pub(crate) fn broadcasting(&self) -> bool {
        self.broadcast_count() >= 2
    }

    /// Adds/removes the focused pane from the broadcast group. Reaching ≥2
    /// members auto-enables broadcast (and exits zoom).
    fn toggle_broadcast_member(&mut self) {
        let Some(p) = self.panes.get(self.focus) else {
            return;
        };
        let key = pane_key(p);
        if !self.broadcast_group.remove(&key) {
            self.broadcast_group.insert(key);
        }
        self.after_broadcast_change();
    }

    /// Toggles the whole broadcast group on/off (leader b).
    fn select_all_broadcast(&mut self) {
        if self.broadcast_count() == self.panes.len() {
            self.broadcast_group.clear(); // all selected → clear
        } else {
            self.broadcast_group = self.panes.iter().map(pane_key).collect();
        }
        self.after_broadcast_change();
    }

    /// Enforces the zoom/broadcast mutual exclusion.
    fn after_broadcast_change(&mut self) {
        if self.broadcasting() && self.zoomed {
            self.zoomed = false;
            self.relayout_session();
        }
    }

    /// Keeps state consistent when the focused pane changes.
    fn after_focus_change(&mut self) {
        if self.scrolling {
            self.scroll_offset = 0; // view the newly focused pane live
        } else {
            self.focus_nav = true;
        }
        if self.zoomed || self.scrolling {
            self.relayout_session(); // resize the newly focused pane if zoomed
        }
    }

    /// Handles keys while in scroll (copy) mode on the focused pane.
    fn update_scroll(&mut self, s: &str) {
        let (max, page) = self.panes.get(self.focus).map_or((0, 10), |p| {
            let rows = p.rows() as usize;
            let page = if rows > 1 { rows - 1 } else { 10 };
            (p.scrollback_len(), page)
        });
        let mut off = self.scroll_offset as i64;
        match s {
            "up" | "k" => off += 1,
            "down" | "j" => off -= 1,
            "pgup" | "b" | "ctrl+b" => off += page as i64,
            "pgdown" | "f" | "ctrl+f" | "space" => off -= page as i64,
            "g" | "home" => off = max as i64,
            "G" | "end" => off = 0,
            "esc" | "q" | "]" => {
                self.scrolling = false;
                self.scroll_offset = 0;
                return;
            }
            _ => {}
        }
        self.scroll_offset = off.clamp(0, max as i64) as usize;
    }

    /// Applies f to every live pane that should receive input: the broadcast
    /// group when broadcasting, else the focused pane.
    fn for_each_receiver(&self, f: impl Fn(&Arc<Pane>)) {
        let send = |p: &Arc<Pane>| {
            if !p.is_done() {
                f(p);
            }
        };
        if self.broadcasting() {
            for p in &self.panes {
                if self.broadcast_group.contains(&pane_key(p)) {
                    send(p);
                }
            }
            return;
        }
        if let Some(p) = self.panes.get(self.focus) {
            send(p);
        }
    }

    /// Sends a key to the focused pane, or to all panes in broadcast mode.
    /// Arrows/home/end are encoded per each pane's own input modes (DECCKM).
    pub(crate) fn write_input(&mut self, k: &KeyEvent) {
        self.for_each_receiver(|p| {
            let bytes = keymap::key_to_bytes(k, p.application_cursor());
            p.write(&bytes);
        });
    }

    /// Pastes text into the receiving pane(s), honoring bracketed paste.
    /// Sent as one write so emulator reply bytes can't interleave between
    /// the paste markers and the body.
    pub(crate) fn session_paste(&mut self, s: &str) {
        self.for_each_receiver(|p| {
            let mut buf = Vec::with_capacity(s.len() + 12);
            if p.with_screen(0, |scr| scr.bracketed_paste()) {
                buf.extend_from_slice(b"\x1b[200~");
                buf.extend_from_slice(s.as_bytes());
                buf.extend_from_slice(b"\x1b[201~");
            } else {
                buf.extend_from_slice(s.as_bytes());
            }
            p.write(&buf);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{argv, exited_pane, live_pane, no_notify};
    use super::*;
    use crate::tui::test_model;
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::time::{Duration, Instant};

    // A pane that exits without any update cycle must not receive forwarded
    // input (defence in depth for the window before the reap runs).
    #[test]
    fn write_input_skips_done_panes() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.panes = vec![exited_pane()];
        m.focus = 0;
        m.write_input(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)); // must not panic or block
    }

    #[test]
    fn broadcast_toggle_and_zoom_exclusion() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.panes = vec![live_pane(), live_pane()];
        m.focus = 0;
        m.zoomed = true;
        // select both panes into the group → auto-broadcast → zoom off
        let none = KeyModifiers::NONE;
        m.update_session(
            &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            "ctrl+b",
        );
        assert!(m.leader_pending);
        m.update_session(&KeyEvent::new(KeyCode::Char('b'), none), "b");
        assert!(!m.leader_pending);
        assert_eq!(m.broadcast_count(), 2);
        assert!(m.broadcasting());
        assert!(!m.zoomed, "auto-broadcast must exit zoom");
        // leader b again clears the group
        m.update_session(
            &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            "ctrl+b",
        );
        m.update_session(&KeyEvent::new(KeyCode::Char('b'), none), "b");
        assert_eq!(m.broadcast_count(), 0);
        for p in &m.panes {
            p.close();
        }
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
        m.panes = vec![p.clone()];
        m.focus = 0;

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
        m.panes = vec![p.clone()];
        m.focus = 0;

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

    #[test]
    fn directional_focus_in_grid() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.panes = vec![live_pane(), live_pane(), live_pane(), live_pane()];
        m.layout = crate::tui::Layout::Grid; // 2×2
        let none = KeyModifiers::NONE;
        let leader = |m: &mut crate::tui::Model| {
            m.update_session(
                &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
                "ctrl+b",
            );
        };
        leader(&mut m);
        m.update_session(&KeyEvent::new(KeyCode::Char('j'), none), "j");
        assert_eq!(m.focus, 2, "leader j must move one grid row down");
        // focus-nav stays active after a move: plain arrows keep moving
        m.update_session(&KeyEvent::new(KeyCode::Up, none), "up");
        assert_eq!(m.focus, 0, "up in focus-nav must move one grid row up");
        // a single-row layout has nowhere to go vertically
        m.layout = crate::tui::Layout::Columns;
        leader(&mut m);
        m.update_session(&KeyEvent::new(KeyCode::Char('j'), none), "j");
        assert_eq!(m.focus, 0, "j must be a no-op in the columns layout");
        for p in &m.panes {
            p.close();
        }
    }

    #[test]
    fn leader_z_blocked_while_broadcasting() {
        let mut m = test_model();
        m.mode = Mode::Session;
        m.panes = vec![live_pane(), live_pane()];
        m.broadcast_group = m.panes.iter().map(pane_key).collect();
        m.leader_pending = true;
        m.update_session(&KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), "z");
        assert!(!m.zoomed);
        assert!(m.status.contains("broadcasting"));
        for p in &m.panes {
            p.close();
        }
    }
}
