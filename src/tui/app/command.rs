//! The `:` command mode, k9s-style: a prompt with inline (fish-style)
//! suggestions that switches resource views, profiles, and runs the few
//! non-view commands (:q, :help). Mirrors the `filtering` pattern — an
//! Input plus a flag, no separate Mode; the list keeps rendering beneath.

use crossterm::event::KeyEvent;

use crate::resources::{KINDS, ResourceKind};

use super::{Mode, Model};

/// Commands that take an argument (completed from live data, not aliases).
const ARG_COMMANDS: [&str; 2] = ["profile", "ctx"];

/// Every bare command name: the resource-view aliases (AWS CLI
/// conventions) plus the built-ins.
fn command_names() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for k in std::iter::once(ResourceKind::Instances).chain(KINDS) {
        v.extend(k.aliases());
    }
    v.extend(["profile", "ctx", "help", "quit", "q"]);
    v
}

impl Model {
    /// Opens the command prompt.
    pub(super) fn open_command(&mut self) {
        self.commanding = true;
        self.cmd.clear();
        self.cmd_sel = 0;
        // the prompt box steals rows from the table — keep the cursor visible
        self.ensure_cursor_visible();
    }

    /// Routes keys while the `:` prompt is open.
    pub(super) fn update_command(&mut self, k: &KeyEvent, s: &str) {
        match s {
            "ctrl+c" => self.quit = true,
            "esc" => {
                self.commanding = false;
                self.cmd.clear();
                self.ensure_cursor_visible(); // the table got its rows back
            }
            "enter" => {
                let line = self.resolved_command();
                self.commanding = false;
                self.cmd.clear();
                self.ensure_cursor_visible();
                self.execute_command(&line);
            }
            "tab" => {
                // accept the current suggestion
                let sug = self.cmd_suggestion();
                if !sug.is_empty() {
                    self.cmd.insert_str(&sug);
                }
            }
            "up" | "down" => {
                // recall the last command on an empty prompt; otherwise cycle
                // through the suggestion candidates in place
                if self.cmd.value().is_empty() && !self.cmd_last.is_empty() {
                    let last = self.cmd_last.clone();
                    self.cmd.insert_str(&last);
                    return;
                }
                let n = self.cmd_candidates().len();
                if n > 0 {
                    self.cmd_sel = if s == "down" {
                        (self.cmd_sel + 1) % n
                    } else {
                        (self.cmd_sel + n - 1) % n
                    };
                }
            }
            _ => {
                if self.cmd.handle(k) {
                    self.cmd_sel = 0;
                }
            }
        }
    }

    /// The typed line, or — when it uniquely prefixes a candidate — that
    /// candidate (k9s accepts unique prefixes on enter).
    fn resolved_command(&self) -> String {
        let line = self.cmd.value().trim().to_string();
        if line.is_empty() {
            return line;
        }
        let cands = self.cmd_candidates();
        if let Some(c) = cands.get(self.cmd_sel.min(cands.len().saturating_sub(1)))
            && !cands.is_empty()
        {
            // exact input wins; otherwise take the selected/unique candidate
            let exact = match line.split_once(' ') {
                None => command_names().contains(&line.as_str()),
                Some((cmd, arg)) => {
                    ARG_COMMANDS.contains(&cmd) && self.profiles.iter().any(|p| p == arg)
                }
            };
            if !exact && (cands.len() == 1 || self.cmd_sel > 0) {
                return c.clone();
            }
        }
        line
    }

    /// Completion candidates for the current input: command names by prefix,
    /// or — after `profile ` / `ctx ` — profile names by fuzzy match.
    pub(crate) fn cmd_candidates(&self) -> Vec<String> {
        let line = self.cmd.value().trim_start();
        if let Some((cmd, arg)) = line.split_once(' ') {
            if !ARG_COMMANDS.contains(&cmd) {
                return Vec::new();
            }
            return crate::fuzzy::rank(arg.trim(), self.profiles.iter().map(String::as_str))
                .into_iter()
                .map(|(i, _)| format!("{cmd} {}", self.profiles[i]))
                .collect();
        }
        if line.is_empty() {
            return Vec::new();
        }
        command_names()
            .into_iter()
            .filter(|c| c.starts_with(line) && *c != line)
            .map(str::to_string)
            .collect()
    }

    /// The inline ghost text: the remainder of the selected candidate.
    pub(crate) fn cmd_suggestion(&self) -> String {
        let cands = self.cmd_candidates();
        let Some(c) = cands.get(self.cmd_sel.min(cands.len().saturating_sub(1))) else {
            return String::new();
        };
        let typed = self.cmd.value().trim_start();
        c.strip_prefix(typed).unwrap_or_default().to_string()
    }

    /// Runs a completed command line. Unknown commands flash an error and
    /// leave the view untouched (k9s behavior).
    fn execute_command(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.cmd_last = line.to_string();
        let (cmd, arg) = match line.split_once(' ') {
            Some((c, a)) => (c, a.trim()),
            None => (line, ""),
        };
        match (cmd, arg) {
            ("q" | "quit", _) => self.quit = true,
            ("help" | "?", _) => {
                self.overlay_scroll = 0;
                self.mode = Mode::Help;
            }
            ("profile" | "ctx", "") => {
                if self.profiles.is_empty() {
                    self.status = "no AWS profiles found".to_string();
                } else {
                    self.picker_input.clear();
                    self.picker_cursor = 0;
                    self.mode = Mode::Profiles;
                }
            }
            ("profile" | "ctx", name) => {
                // fuzzy-resolve the argument: best match wins
                let ranked = crate::fuzzy::rank(name, self.profiles.iter().map(String::as_str));
                match ranked.first() {
                    Some((i, _)) => {
                        let p = self.profiles[*i].clone();
                        self.select_profile(&p);
                    }
                    None => self.status = format!("no profile matches {name:?}"),
                }
            }
            _ => match ResourceKind::from_alias(cmd) {
                Some(kind) => self.switch_view(kind),
                None => self.status = format!("command not found: {line:?}"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_model;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(m: &mut crate::tui::Model, code: KeyCode) {
        let k = KeyEvent::new(code, KeyModifiers::NONE);
        let s = crate::tui::keymap::key_name(&k);
        m.update_command(&k, &s);
    }

    fn type_str(m: &mut crate::tui::Model, s: &str) {
        for c in s.chars() {
            key(m, KeyCode::Char(c));
        }
    }

    #[test]
    fn colon_opens_and_esc_closes() {
        let mut m = test_model();
        let k = KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE);
        m.update_list(&k, ":");
        assert!(m.commanding);
        key(&mut m, KeyCode::Esc);
        assert!(!m.commanding);
        assert!(m.cmd.value().is_empty());
    }

    #[test]
    fn suggests_by_prefix_and_tab_completes() {
        let mut m = test_model();
        m.open_command();
        type_str(&mut m, "pr");
        assert_eq!(m.cmd_candidates(), vec!["profile".to_string()]);
        assert_eq!(m.cmd_suggestion(), "ofile");
        key(&mut m, KeyCode::Tab);
        assert_eq!(m.cmd.value(), "profile");
    }

    #[test]
    fn quit_and_help_commands() {
        let mut m = test_model();
        m.open_command();
        type_str(&mut m, "q");
        key(&mut m, KeyCode::Enter);
        assert!(m.should_quit());

        let mut m = test_model();
        m.open_command();
        type_str(&mut m, "help");
        key(&mut m, KeyCode::Enter);
        assert_eq!(m.mode, crate::tui::Mode::Help);
    }

    #[test]
    fn unknown_command_flashes_error() {
        let mut m = test_model();
        m.open_command();
        type_str(&mut m, "bogus");
        key(&mut m, KeyCode::Enter);
        assert!(!m.commanding);
        assert!(
            m.status.contains("command not found"),
            "status: {}",
            m.status
        );
    }

    #[test]
    fn profile_command_opens_picker_and_arg_switches() {
        let mut m = test_model();
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into()];
        m.open_command();
        type_str(&mut m, "ctx");
        key(&mut m, KeyCode::Enter);
        assert_eq!(m.mode, crate::tui::Mode::Profiles);

        // with an argument: fuzzy-resolved and switched directly (the test
        // build fn errors, which surfaces as a status — that's fine, it
        // proves select_profile ran on the right name)
        m.mode = crate::tui::Mode::List;
        m.open_command();
        type_str(&mut m, "profile prd");
        key(&mut m, KeyCode::Enter);
        assert!(
            m.status.contains("profile load error"),
            "expected select_profile to run: {}",
            m.status
        );
    }

    #[test]
    fn up_recalls_last_command() {
        let mut m = test_model();
        m.open_command();
        type_str(&mut m, "help");
        key(&mut m, KeyCode::Enter);
        m.mode = crate::tui::Mode::List;
        m.open_command();
        key(&mut m, KeyCode::Up);
        assert_eq!(m.cmd.value(), "help");
    }

    #[test]
    fn profile_arg_candidates_are_fuzzy() {
        let mut m = test_model();
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        m.open_command();
        type_str(&mut m, "profile cld");
        let c = m.cmd_candidates();
        assert_eq!(c.len(), 2, "{c:?}");
        assert!(c[0].starts_with("profile Cloud."), "{c:?}");
    }
}
