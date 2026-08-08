//! Mouse handling: wheel scrolling, click-to-select, double-click-to-connect
//! in the list, and click-to-focus / wheel scrollback in the session view.
//! Keyboard flows are untouched — the mouse is an additive input path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::tui::keymap;

use super::list::LIST_ROW_H;
use super::{Mode, Model};

/// Lines scrolled per wheel notch.
const WHEEL_STEP: usize = 3;

/// Two clicks on the same row within this window count as a double-click.
const DOUBLE_CLICK_MS: u128 = 400;

/// First item row of the profile picker (title + prompt line above).
const PICKER_DATA_Y: u16 = 2;

impl Model {
    pub(super) fn handle_mouse(&mut self, me: &MouseEvent) {
        match self.mode {
            Mode::List => self.mouse_list(me),
            Mode::Session => self.mouse_session(me),
            Mode::Detail => self.mouse_overlay(me, self.detail_content_height()),
            Mode::Help => self.mouse_overlay(me, self.help_lines().len()),
            Mode::Profiles => self.mouse_profiles(me),
            Mode::Confirm => {} // keyboard-only: y / n
            Mode::Forward => {} // keyboard-only form
        }
    }

    /// Records a left click and reports whether it completed a double-click
    /// on the same row of the same screen.
    fn register_click(&mut self, mode: Mode, idx: usize) -> bool {
        let now = std::time::Instant::now();
        let double = matches!(
            self.last_click,
            Some((t, m, i))
                if m == mode && i == idx && now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS
        );
        // A completed double-click resets, so a triple click doesn't fire twice.
        self.last_click = if double { None } else { Some((now, mode, idx)) };
        double
    }

    // ---- instance list ----

    fn mouse_list(&mut self, me: &MouseEvent) {
        match me.kind {
            MouseEventKind::ScrollUp => self.cursor_up(WHEEL_STEP),
            MouseEventKind::ScrollDown => self.cursor_down(WHEEL_STEP),
            MouseEventKind::ScrollLeft => {
                self.h_offset = self.h_offset.saturating_sub(8);
            }
            MouseEventKind::ScrollRight => {
                self.h_offset += 8;
                self.clamp_h_offset();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Data rows start below the header/prompt/panel-border chrome
                // (view/list.rs draw_list geometry, mirrored by list_data_y).
                let data_y = self.list_data_y();
                if me.row < data_y {
                    return;
                }
                let idx = self.row_offset + (me.row - data_y) as usize / LIST_ROW_H;
                if idx >= self.filtered.len() {
                    return;
                }
                let double = self.register_click(Mode::List, idx);
                self.cursor = idx;
                self.ensure_cursor_visible();
                if double {
                    if self.adding_pane {
                        self.add_pane_from_list();
                    } else if let Some(inst) = self
                        .filtered
                        .get(idx)
                        .filter(|i| i.is_connectable())
                        .cloned()
                    {
                        self.start_session(vec![inst]);
                    }
                }
            }
            _ => {}
        }
    }

    // ---- session view ----

    fn mouse_session(&mut self, me: &MouseEvent) {
        match me.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(i) = self.pane_at(me.column, me.row) {
                    self.focus_pane_click(i);
                }
            }
            MouseEventKind::ScrollUp => self.session_wheel(me.column, me.row, 1),
            MouseEventKind::ScrollDown => self.session_wheel(me.column, me.row, -1),
            _ => {}
        }
    }

    /// The pane under the screen position, if any (row 0 is the header).
    fn pane_at(&self, x: u16, y: u16) -> Option<usize> {
        if self.panes.is_empty() || y == 0 {
            return None;
        }
        if self.is_fullscreen() {
            return Some(self.focus.min(self.panes.len() - 1));
        }
        self.pane_rects()
            .iter()
            .position(|r| x >= r.x && x < r.x + r.outer_w && y >= r.y && y < r.y + r.outer_h)
    }

    /// Focuses the clicked pane. Unlike leader focus moves this does not
    /// enter focus-nav — the next keystrokes should go to the shell.
    fn focus_pane_click(&mut self, i: usize) {
        self.focus_nav = false;
        if i == self.focus {
            return;
        }
        self.focus = i;
        if self.scrolling {
            self.scroll_offset = 0; // view the newly focused pane live
        }
        if self.zoomed || self.scrolling {
            self.relayout_session();
        }
    }

    /// Wheel over a pane: scroll its scrollback (entering scroll mode), or —
    /// for alternate-screen apps with no scrollback (less, vim, htop…) —
    /// forward arrow keys so the wheel scrolls inside the app (tmux behavior).
    fn session_wheel(&mut self, x: u16, y: u16, dir: i32) {
        let Some(i) = self.pane_at(x, y) else {
            return;
        };
        self.focus_pane_click(i);
        let Some(p) = self.panes.get(self.focus) else {
            return;
        };
        let max = p.scrollback_len();
        if max == 0 && !self.scrolling {
            let code = if dir > 0 { KeyCode::Up } else { KeyCode::Down };
            let bytes = keymap::key_to_bytes(
                &KeyEvent::new(code, KeyModifiers::NONE),
                p.application_cursor(),
            );
            if !p.is_done() {
                for _ in 0..WHEEL_STEP {
                    p.write(&bytes);
                }
            }
            return;
        }
        if dir > 0 {
            self.scrolling = true;
            self.scroll_offset = (self.scroll_offset + WHEEL_STEP).min(max);
        } else if self.scrolling {
            self.scroll_offset = self.scroll_offset.saturating_sub(WHEEL_STEP);
            if self.scroll_offset == 0 {
                self.scrolling = false; // back to live
            }
        }
    }

    // ---- detail / help ----

    fn mouse_overlay(&mut self, me: &MouseEvent, total: usize) {
        let s = match me.kind {
            MouseEventKind::ScrollUp => "up",
            MouseEventKind::ScrollDown => "down",
            _ => return,
        };
        for _ in 0..WHEEL_STEP {
            self.overlay_scroll_key(s, total, self.page_rows());
        }
    }

    // ---- profile picker ----

    fn mouse_profiles(&mut self, me: &MouseEvent) {
        let items = self.picker_filtered();
        if items.is_empty() {
            return;
        }
        match me.kind {
            MouseEventKind::ScrollUp => {
                self.picker_cursor = self.picker_cursor.saturating_sub(WHEEL_STEP);
            }
            MouseEventKind::ScrollDown => {
                self.picker_cursor = (self.picker_cursor + WHEEL_STEP).min(items.len() - 1);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if me.row < PICKER_DATA_Y {
                    return;
                }
                // Mirror the picker's viewport math in view/overlays.rs.
                let vis = (self.height as usize).saturating_sub(3).max(1);
                let offset = (self.picker_cursor + 1).saturating_sub(vis);
                let idx = offset + (me.row - PICKER_DATA_Y) as usize;
                if idx >= items.len() {
                    return;
                }
                let double = self.register_click(Mode::Profiles, idx);
                self.picker_cursor = idx;
                if double {
                    self.select_profile(&items[idx].clone());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_model;
    use super::*;
    use crate::inventory::Instance;

    fn inst(name: &str, online: bool) -> Instance {
        Instance {
            instance_id: format!("i-{name}"),
            name: name.to_string(),
            state: "running".to_string(),
            ssm: online.then(|| crate::inventory::SsmStatus {
                online: true,
                agent_version: "3.3".into(),
                ping_status: "Online".into(),
            }),
            ..Default::default()
        }
    }

    fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn list_wheel_and_click_select() {
        let mut m = test_model();
        m.all = (0..30).map(|i| inst(&format!("h{i}"), true)).collect();
        m.apply_filter();

        m.handle_mouse(&mouse(MouseEventKind::ScrollDown, 10, 10));
        assert_eq!(m.cursor, 3, "wheel must move the cursor");
        m.handle_mouse(&mouse(MouseEventKind::ScrollUp, 10, 10));
        assert_eq!(m.cursor, 0);

        // click on the 3rd visible data row (each row spans LIST_ROW_H lines;
        // its content line is the last one) selects it
        let data_y = m.list_data_y();
        let row3_content = data_y + 3 * LIST_ROW_H as u16 - 1;
        let down = MouseEventKind::Down(MouseButton::Left);
        m.handle_mouse(&mouse(down, 10, row3_content));
        assert_eq!(m.cursor, 2, "click must select the row under the pointer");
        m.handle_mouse(&mouse(down, 10, data_y + 3 * LIST_ROW_H as u16));
        assert_eq!(m.cursor, 3, "next row slot must hit the row below");
        // clicks above the table are ignored
        m.handle_mouse(&mouse(down, 10, 0));
        assert_eq!(m.cursor, 3);
        // double-click on a connectable host tries to connect; without a
        // driver in tests this is a guarded no-op — it must not panic
        m.handle_mouse(&mouse(down, 10, row3_content));
        m.handle_mouse(&mouse(down, 10, row3_content));
        assert_eq!(m.cursor, 2);
    }

    #[test]
    fn session_click_focuses_and_wheel_scrolls() {
        let mut m = test_model();
        m.mode = Mode::Session;
        let argv: Vec<String> = ["sleep", "60"].iter().map(|s| s.to_string()).collect();
        let notify = std::sync::Arc::new(|| {});
        m.panes = vec![
            crate::session::Pane::start("a", &argv, 40, 10, notify.clone()).unwrap(),
            crate::session::Pane::start("b", &argv, 40, 10, notify).unwrap(),
        ];
        m.relayout_session();

        // columns layout on a 100-wide screen: pane 1 starts at x=50
        let down = MouseEventKind::Down(MouseButton::Left);
        m.handle_mouse(&mouse(down, 75, 5));
        assert_eq!(m.focus, 1, "click must focus the pane under the pointer");
        assert!(!m.focus_nav, "a click must not enter focus-nav");
        m.handle_mouse(&mouse(down, 25, 5));
        assert_eq!(m.focus, 0);
        // the header row is not a pane
        m.focus = 1;
        m.handle_mouse(&mouse(down, 25, 0));
        assert_eq!(m.focus, 1, "header clicks must be ignored");

        for p in &m.panes {
            p.close();
        }
    }

    #[test]
    fn detail_and_picker_wheel() {
        let mut m = test_model();
        m.mode = Mode::Detail;
        m.height = 10; // detail content overflows → scrollable
        m.handle_mouse(&mouse(MouseEventKind::ScrollDown, 5, 5));
        assert_eq!(m.overlay_scroll, 3, "wheel must scroll the detail view");
        m.handle_mouse(&mouse(MouseEventKind::ScrollUp, 5, 5));
        assert_eq!(m.overlay_scroll, 0);

        m.mode = Mode::Profiles;
        m.profiles = vec!["dev".into(), "stg".into(), "prod".into()];
        let down = MouseEventKind::Down(MouseButton::Left);
        m.handle_mouse(&mouse(down, 5, PICKER_DATA_Y + 1));
        assert_eq!(m.picker_cursor, 1, "click must select the profile row");
        m.handle_mouse(&mouse(MouseEventKind::ScrollDown, 5, 5));
        assert_eq!(m.picker_cursor, 2, "wheel is clamped to the last profile");
    }
}
