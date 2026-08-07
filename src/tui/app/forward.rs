//! The port-forward form (Mode::Forward): field navigation, validation, and
//! starting the forwarding pane.

use crossterm::event::KeyEvent;

use super::{ForwardForm, Mode, Model};
use crate::inventory::Instance;
use crate::tui::input::Input;

impl Model {
    /// Opens the port-forward form for an instance.
    pub(crate) fn open_forward_form(&mut self, target: Instance) {
        self.fwd = ForwardForm {
            target,
            ..Default::default()
        };
        self.mode = Mode::Forward;
    }

    /// The Input of the currently focused form field.
    pub(crate) fn forward_field_mut(&mut self) -> &mut Input {
        match self.fwd.field {
            0 => &mut self.fwd.host,
            1 => &mut self.fwd.port,
            _ => &mut self.fwd.local,
        }
    }

    pub(super) fn update_forward(&mut self, k: &KeyEvent, s: &str) {
        match s {
            "ctrl+c" => self.quit = true,
            "esc" => self.mode = Mode::List,
            "tab" | "down" | "enter" if s != "enter" || self.fwd.field < 2 => {
                // enter advances until the last field, where it submits
                self.fwd.field = (self.fwd.field + 1) % 3;
            }
            "shift+tab" | "up" => self.fwd.field = (self.fwd.field + 2) % 3,
            "enter" => self.submit_forward(),
            _ => {
                self.forward_field_mut().handle(k);
                self.fwd.error.clear();
            }
        }
    }

    fn submit_forward(&mut self) {
        let host = self.fwd.host.value().trim().to_string();
        // The host lands inside the --parameters JSON: restrict it to
        // hostname/IP characters so it cannot break out of the string.
        if !host.is_empty()
            && !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
        {
            self.fwd.error = "remote host: letters, digits, . - _ : only".to_string();
            return;
        }
        let Ok(port) = self.fwd.port.value().trim().parse::<u16>() else {
            self.fwd.error = "remote port: required, 1–65535".to_string();
            return;
        };
        let local_raw = self.fwd.local.value().trim();
        let local = if local_raw.is_empty() {
            port // default: same local port
        } else {
            match local_raw.parse::<u16>() {
                Ok(p) => p,
                Err(_) => {
                    self.fwd.error = "local port: 1–65535 (empty = same as remote)".to_string();
                    return;
                }
            }
        };
        if port == 0 || local == 0 {
            self.fwd.error = "port 0 is not forwardable".to_string();
            return;
        }
        let Some(drv) = self.driver.clone() else {
            self.fwd.error = "no AWS profile loaded".to_string();
            return;
        };
        let argv = drv.port_forward_command(&self.fwd.target.instance_id, local, &host, port);
        let title = if host.is_empty() {
            format!("fwd :{local} → {}:{port}", self.fwd.target.name)
        } else {
            format!("fwd :{local} → {host}:{port} via {}", self.fwd.target.name)
        };
        if let Err(e) = self.start_forward_pane(&title, &argv) {
            self.fwd.error = format!("failed to start: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_model;
    use crate::inventory::Instance;
    use crate::tui::Mode;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(m: &mut crate::tui::Model, code: KeyCode) {
        let k = KeyEvent::new(code, KeyModifiers::NONE);
        let s = crate::tui::keymap::key_name(&k);
        m.update_forward(&k, &s);
    }

    fn type_str(m: &mut crate::tui::Model, s: &str) {
        for c in s.chars() {
            key(m, KeyCode::Char(c));
        }
    }

    #[test]
    fn form_navigation_and_validation() {
        let mut m = test_model();
        m.open_forward_form(Instance {
            instance_id: "i-1".into(),
            name: "db-bastion".into(),
            ..Default::default()
        });
        assert_eq!(m.mode, Mode::Forward);
        assert_eq!(m.fwd.field, 0);

        // enter on a non-last field advances instead of submitting
        key(&mut m, KeyCode::Enter);
        assert_eq!(m.fwd.field, 1);
        key(&mut m, KeyCode::Tab);
        assert_eq!(m.fwd.field, 2);
        key(&mut m, KeyCode::Up);
        assert_eq!(m.fwd.field, 1);

        // submit without a remote port → validation error
        key(&mut m, KeyCode::Down);
        key(&mut m, KeyCode::Enter);
        assert!(m.fwd.error.contains("remote port"), "{}", m.fwd.error);
        assert_eq!(m.mode, Mode::Forward);

        // bad host charset is rejected (JSON-injection guard)
        m.fwd.field = 0;
        type_str(&mut m, "bad\"host");
        m.fwd.field = 1;
        type_str(&mut m, "5432");
        m.fwd.field = 2;
        key(&mut m, KeyCode::Enter);
        assert!(m.fwd.error.contains("remote host"), "{}", m.fwd.error);

        // esc returns to the list
        key(&mut m, KeyCode::Esc);
        assert_eq!(m.mode, Mode::List);
    }

    #[test]
    fn typing_clears_error() {
        let mut m = test_model();
        m.open_forward_form(Instance::default());
        m.fwd.field = 2;
        key(&mut m, KeyCode::Enter); // invalid submit
        assert!(!m.fwd.error.is_empty());
        key(&mut m, KeyCode::Char('8'));
        assert!(m.fwd.error.is_empty(), "typing must clear the error");
    }
}
