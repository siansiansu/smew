//! Pane lifecycle: starting the single session pane, resizing it, closing
//! it, and reaping it when its process ends.

use crate::inventory::Instance;
use crate::session::Pane;
use crate::tui::{ConfirmKind, Mode, Model};

impl Model {
    /// The session pane's emulator size: full width, one line each for the
    /// header and the pane title.
    fn pane_dims(&self) -> (u16, u16) {
        (self.width.max(1), self.height.saturating_sub(2).max(1))
    }

    /// Opens an SSM shell to the instance as the (single) session pane and
    /// enters the session view.
    pub(crate) fn start_session(&mut self, inst: Option<Instance>) {
        let Some(inst) = inst.filter(|i| i.is_connectable()) else {
            self.status = "not connectable via SSM".to_string();
            return;
        };
        let Some(drv) = self.driver.clone() else {
            return;
        };
        let argv = drv.shell_command(&inst.instance_id);
        self.open_pane(&inst.name, &argv);
    }

    /// Opens an SSH login pane: `smew eic-ssh` pushes a 60-second EC2
    /// Instance Connect key, then runs `ssh user@ip` over the network
    /// (public IP preferred). SSM plays no part — the host's port 22 must
    /// be reachable from here.
    pub(crate) fn start_ssh_session(&mut self, inst: Option<Instance>) {
        let has_ip = |i: &Instance| !i.public_ip.is_empty() || !i.private_ip.is_empty();
        let Some(inst) = inst.filter(has_ip) else {
            self.status = "no host with an IP selected".to_string();
            return;
        };
        let Some(drv) = self.driver.clone() else {
            return;
        };
        let ip = if inst.public_ip.is_empty() {
            &inst.private_ip
        } else {
            &inst.public_ip
        };
        match drv.ssh_shell_command(&inst.instance_id, ip, &self.ssh_user, &self.ssh_key) {
            Ok(argv) => self.open_pane(&format!("{} (ssh)", inst.name), &argv),
            Err(e) => self.status = format!("ssh {}: {e}", inst.name),
        }
    }

    /// Opens a port-forwarding pane running the given argv as its own
    /// session (sessions are single-pane; a running one is replaced).
    pub(crate) fn start_forward_pane(
        &mut self,
        title: &str,
        argv: &[String],
    ) -> Result<(), String> {
        self.open_pane(title, argv);
        if self.pane.is_none() {
            return Err(self.status.clone());
        }
        Ok(())
    }

    /// Replaces the session pane (if any) with one running the given argv
    /// and enters the session view.
    fn open_pane(&mut self, title: &str, argv: &[String]) {
        if let Some(p) = self.pane.take() {
            p.close();
        }
        let (cols, rows) = self.pane_dims();
        match Pane::start(title, argv, cols, rows, self.pane_notifier()) {
            Ok(p) => self.pane = Some(p),
            Err(e) => {
                self.status = format!("failed to start {title}: {e}");
                return;
            }
        }
        self.mode = Mode::Session;
        self.leader_pending = false;
        self.scrolling = false;
        self.scroll_offset = 0;
        self.status.clear();
    }

    /// Fits the session pane's emulator + PTY to the current screen.
    pub(crate) fn relayout_session(&self) {
        if let Some(p) = &self.pane {
            let (cols, rows) = self.pane_dims();
            p.resize(cols, rows);
        }
    }

    pub(crate) fn close_session(&mut self) {
        if let Some(p) = self.pane.take() {
            p.close();
        }
        self.mode = Mode::List;
        self.status = "session closed".to_string();
    }

    /// Closes the session when its process has ended (e.g. the user typed
    /// `exit` in the remote shell). Without this the session view keeps
    /// forwarding keys to a dead PTY and appears frozen.
    pub(crate) fn reap_exited_panes(&mut self) {
        let Some(p) = self.pane.as_ref().filter(|p| p.is_done()) else {
            return;
        };
        let tail = p.last_line();
        let note = if tail.is_empty() {
            p.title.clone()
        } else {
            format!("{}: {tail}", p.title)
        };
        let p = self.pane.take().unwrap();
        p.close(); // release the PTY
        self.scrolling = false;
        self.scroll_offset = 0;
        if self.mode == Mode::Session
            || (self.mode == Mode::Confirm && self.confirm_action == ConfirmKind::CloseSession)
        {
            self.mode = Mode::List;
        }
        self.status = format!("session ended — {note}");
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{exited_pane, live_pane};
    use super::*;

    // Typing `exit` in the pane must end the session and return to the
    // list (the reported freeze: the session view kept forwarding keys to a
    // dead PTY).
    #[test]
    fn reap_dead_pane_ends_session() {
        let mut m = crate::tui::test_model();
        m.mode = Mode::Session;
        m.pane = Some(exited_pane());
        m.reap_exited_panes();
        assert_eq!(m.mode, Mode::List);
        assert!(m.pane.is_none());
        assert!(
            m.status.contains("session ended"),
            "status = {:?}",
            m.status
        );
    }

    // A live pane is left alone.
    #[test]
    fn reap_keeps_live_pane() {
        let mut m = crate::tui::test_model();
        m.mode = Mode::Session;
        m.pane = Some(live_pane());
        m.reap_exited_panes();
        assert_eq!(m.mode, Mode::Session);
        assert!(m.pane.is_some());
        m.pane.take().unwrap().close();
    }

    // Opening a session replaces any running pane (sessions are single-pane).
    #[test]
    fn open_replaces_running_pane() {
        let mut m = crate::tui::test_model();
        m.driver = Some(crate::session::PluginDriver::dev());
        let inst = |name: &str| Instance {
            instance_id: format!("i-{name}"),
            name: name.to_string(),
            state: "running".to_string(),
            ssm: Some(crate::inventory::SsmStatus {
                online: true,
                agent_version: "3.3".into(),
                ping_status: "Online".into(),
            }),
            ..Default::default()
        };
        m.start_session(Some(inst("web")));
        assert_eq!(m.mode, Mode::Session);
        assert_eq!(m.pane.as_ref().unwrap().title, "web");
        // opening another host replaces the pane (never a second one)
        m.start_session(Some(inst("db")));
        assert_eq!(m.pane.as_ref().unwrap().title, "db");
        m.pane.take().unwrap().close();

        // a non-connectable host only reports
        m.start_session(Some(Instance::default()));
        assert!(m.pane.is_none());
        assert!(m.status.contains("not connectable"), "{}", m.status);
    }
}
