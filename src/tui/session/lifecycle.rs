//! Pane lifecycle: starting sessions, adding panes, closing them, and
//! reaping panes whose process has ended.

use std::sync::Arc;

use super::pane_key;
use crate::inventory::Instance;
use crate::session::Pane;
use crate::tui::{ConfirmKind, Mode, Model};

impl Model {
    /// Starts `aws ssm start-session` for the instance on a fresh PTY pane.
    /// Reports a failure via the status bar and returns None.
    fn spawn_pane(&mut self, inst: &Instance, cols: u16, rows: u16) -> Option<Arc<Pane>> {
        let drv = self.driver.clone()?;
        let argv = drv.shell_command(&inst.instance_id);
        match Pane::start(&inst.name, &argv, cols, rows, self.pane_notifier()) {
            Ok(p) => Some(p),
            Err(e) => {
                self.status = format!("failed to start {}: {e}", inst.name);
                None
            }
        }
    }

    /// Rough initial pane height; relayout_session fixes it right after.
    fn initial_pane_rows(&self) -> u16 {
        self.height.saturating_sub(4).max(1)
    }

    /// Opens the given instances as tiled panes and enters the session view.
    /// Each pane runs `aws ssm start-session` on its own PTY.
    pub(crate) fn start_session(&mut self, targets: Vec<Instance>) {
        if targets.is_empty() {
            self.status = "nothing connectable selected".to_string();
            return;
        }
        if self.driver.is_none() {
            return;
        }
        let cols = ((self.width as usize / targets.len())
            .saturating_sub(2)
            .max(1)) as u16;
        let rows = self.initial_pane_rows();
        // Release any panes from a previous session before replacing them.
        for p in self.panes.drain(..) {
            p.close();
        }
        self.broadcast_group.clear();
        for inst in &targets {
            if let Some(p) = self.spawn_pane(inst, cols, rows) {
                self.panes.push(p);
            }
        }
        if self.panes.is_empty() {
            return;
        }
        self.mode = Mode::Session;
        self.focus = 0;
        self.focus_nav = false;
        self.leader_pending = false;
        self.adding_pane = false;
        self.zoomed = false;
        self.scrolling = false;
        self.scroll_offset = 0;
        self.status.clear();
        self.relayout_session();
    }

    /// Adds the list-selected host as a new pane and returns to the session
    /// (used by the in-session "add pane" flow).
    pub(crate) fn add_pane_from_list(&mut self) {
        let Some(inst) = self.filtered.get(self.cursor).cloned() else {
            return;
        };
        if !inst.is_connectable() {
            self.status = format!("not connectable via SSM: {}", inst.name);
            return;
        }
        let rows = self.initial_pane_rows();
        if let Some(p) = self.spawn_pane(&inst, 20, rows) {
            self.panes.push(p);
            self.focus = self.panes.len() - 1;
            self.adding_pane = false;
            self.mode = Mode::Session;
            self.status.clear();
            self.relayout_session();
        }
    }

    /// Opens a port-forwarding pane running the given argv: appended to the
    /// running session, or as a fresh single-pane session. Focuses it.
    pub(crate) fn start_forward_pane(
        &mut self,
        title: &str,
        argv: &[String],
    ) -> Result<(), String> {
        let rows = self.initial_pane_rows();
        let p =
            Pane::start(title, argv, 20, rows, self.pane_notifier()).map_err(|e| e.to_string())?;
        if self.panes.is_empty() {
            // fresh session: reset multiplexer state like start_session does
            self.broadcast_group.clear();
            self.zoomed = false;
            self.scrolling = false;
            self.scroll_offset = 0;
            self.focus_nav = false;
            self.leader_pending = false;
            self.adding_pane = false;
        }
        self.panes.push(p);
        self.focus = self.panes.len() - 1;
        self.mode = Mode::Session;
        self.status.clear();
        self.relayout_session();
        Ok(())
    }

    pub(crate) fn close_session(&mut self) {
        for p in &self.panes {
            p.close();
        }
        self.panes.clear();
        self.broadcast_group.clear();
        self.marked.clear();
        self.mode = Mode::List;
        self.status = "session closed".to_string();
    }

    /// Closes and removes panes whose process has ended (e.g. the user typed
    /// `exit` in the remote shell). Without this the session view keeps
    /// forwarding keys to a dead PTY and appears frozen. When the last pane
    /// goes, the session ends and the UI returns to the instance list.
    pub(crate) fn reap_exited_panes(&mut self) {
        let focused = self.panes.get(self.focus).map(pane_key);
        let before = self.panes.len();
        let mut note = String::new();
        let mut kept = Vec::with_capacity(before);
        for p in std::mem::take(&mut self.panes) {
            if !p.is_done() {
                kept.push(p);
                continue;
            }
            let tail = p.last_line();
            note = if tail.is_empty() {
                p.title.clone()
            } else {
                format!("{}: {}", p.title, tail)
            };
            p.close(); // release the PTY
            self.broadcast_group.remove(&pane_key(&p));
        }
        self.panes = kept;
        if self.panes.len() == before {
            return;
        }

        if self.panes.is_empty() {
            self.marked.clear();
            self.zoomed = false;
            self.scrolling = false;
            self.scroll_offset = 0;
            self.focus_nav = false;
            self.leader_pending = false;
            self.adding_pane = false;
            if self.mode == Mode::Session
                || (self.mode == Mode::Confirm && self.confirm_action == ConfirmKind::CloseSession)
            {
                self.mode = Mode::List;
            }
            self.status = format!("session ended — {note}");
            return;
        }

        // Keep focus on the pane it was on if that one survived; else clamp.
        let idx = match self.panes.iter().position(|p| Some(pane_key(p)) == focused) {
            Some(i) => i,
            None => {
                // the focused pane itself exited
                self.scrolling = false;
                self.scroll_offset = 0;
                self.focus.min(self.panes.len() - 1)
            }
        };
        self.focus = idx;
        self.status = format!("pane ended — {note}");
        self.relayout_session();
    }

    pub(super) fn close_focused_pane(&mut self) {
        if self.focus >= self.panes.len() {
            return;
        }
        let closed = self.panes.remove(self.focus);
        closed.close();
        self.broadcast_group.remove(&pane_key(&closed));
        if self.panes.is_empty() {
            return self.close_session();
        }
        if self.focus >= self.panes.len() {
            self.focus = self.panes.len() - 1;
        }
        self.scrolling = false;
        self.scroll_offset = 0;
        self.relayout_session();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{exited_pane, live_pane};
    use super::*;

    // Typing `exit` in the last pane must end the session and return to the
    // list (the reported freeze: the session view kept forwarding keys to a
    // dead PTY).
    #[test]
    fn reap_last_pane_ends_session() {
        let mut m = crate::tui::test_model();
        m.mode = Mode::Session;
        m.panes = vec![exited_pane()];
        m.reap_exited_panes();
        assert_eq!(m.mode, Mode::List);
        assert!(m.panes.is_empty());
        assert!(
            m.status.contains("session ended"),
            "status = {:?}",
            m.status
        );
    }

    // When one of several panes exits, only that pane closes; the session
    // stays up, focus lands on a survivor, and the dead pane leaves the
    // broadcast group.
    #[test]
    fn reap_keeps_session_with_survivors() {
        let mut m = crate::tui::test_model();
        m.mode = Mode::Session;
        let dead = exited_pane();
        let live = live_pane();
        let dead_key = pane_key(&dead);
        let live_key = pane_key(&live);
        m.panes = vec![dead, live];
        m.focus = 1;
        m.broadcast_group.insert(dead_key);
        m.reap_exited_panes();
        assert_eq!(m.mode, Mode::Session);
        assert_eq!(m.panes.len(), 1);
        assert_eq!(pane_key(&m.panes[0]), live_key, "want just the live pane");
        assert_eq!(m.focus, 0, "focus moved to the surviving pane");
        assert!(
            !m.broadcast_group.contains(&dead_key),
            "dead pane still in the broadcast group"
        );
        m.panes[0].close();
    }
}
