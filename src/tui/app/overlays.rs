//! Update handlers for the small modal screens: the profile picker, the
//! detail view, help, and the confirmation dialog.

use crossterm::event::KeyEvent;

use super::{ConfirmKind, Mode, Model};

impl Model {
    pub(super) fn update_profiles(&mut self, k: &KeyEvent, s: &str) {
        if self.picker_typing {
            match s {
                // ctrl+c must quit even while typing in the filter.
                "ctrl+c" => self.quit = true,
                "esc" => {
                    self.picker_typing = false;
                    self.picker_input.clear();
                    self.picker_query.clear();
                    self.picker_cursor = 0;
                }
                "enter" => {
                    self.picker_query = self.picker_input.value().to_string();
                    self.picker_typing = false;
                }
                _ => {
                    if self.picker_input.handle(k) {
                        self.picker_cursor = 0;
                    }
                }
            }
            return;
        }
        match s {
            "ctrl+c" | "q" => self.quit = true,
            "esc" => {
                if self.inventory.is_some() {
                    // cancel only if we already have a session
                    self.mode = Mode::List;
                }
            }
            "enter" => {
                if let Some(p) = self.picker_filtered().get(self.picker_cursor).cloned() {
                    self.select_profile(&p);
                }
            }
            "/" => {
                self.picker_typing = true;
                self.picker_input.clear();
                self.picker_query.clear();
                self.picker_cursor = 0;
            }
            "up" | "k" => self.picker_cursor = self.picker_cursor.saturating_sub(1),
            "down" | "j" => {
                let max = self.picker_filtered().len().saturating_sub(1);
                self.picker_cursor = (self.picker_cursor + 1).min(max);
            }
            "g" | "home" => self.picker_cursor = 0,
            "G" | "end" => self.picker_cursor = self.picker_filtered().len().saturating_sub(1),
            "pgup" => self.picker_cursor = self.picker_cursor.saturating_sub(10),
            "pgdown" => {
                let max = self.picker_filtered().len().saturating_sub(1);
                self.picker_cursor = (self.picker_cursor + 10).min(max);
            }
            _ => {}
        }
    }

    /// The profile names passing the picker's substring filter. Profiles are
    /// a small, known, structured set, so substring is more predictable than
    /// fuzzy (e.g. "cloud" → all Cloud.*; a typo like "cloude" → nothing).
    pub(crate) fn picker_filtered(&self) -> Vec<String> {
        let q = if self.picker_typing {
            self.picker_input.value()
        } else {
            &self.picker_query
        };
        let q = q.trim().to_lowercase();
        self.profiles
            .iter()
            .filter(|p| q.is_empty() || p.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }

    /// Rebuilds the inventory/driver for the chosen profile and reloads.
    pub(super) fn select_profile(&mut self, p: &str) {
        match (self.build)(p) {
            Err(e) => {
                self.status = format!("profile load error: {e}");
            }
            Ok((inv, drv, region)) => {
                self.inventory = Some(inv);
                self.driver = Some(drv);
                self.profile = p.to_string();
                self.region = region;
                self.mode = Mode::List;
                self.loading = true;
                self.status = format!("loading… ({p})");
                self.all.clear();
                self.filtered.clear();
                self.filter.clear();
                self.filter_stack.clear();
                self.table_to_top();
                self.update_available = false; // re-check against the new profile
                self.spawn_load();
                self.spawn_version_check();
            }
        }
    }

    pub(super) fn update_detail(&mut self, s: &str) {
        match s {
            "q" | "ctrl+c" => self.quit = true,
            "esc" | "d" | "enter" => self.mode = Mode::List,
            "s" => {
                let t = vec![self.detail.clone()];
                self.start_session(t);
            }
            _ => self.overlay_scroll_key(s, self.detail_lines().len()),
        }
    }

    pub(super) fn update_help(&mut self, s: &str) {
        match s {
            "q" | "ctrl+c" => self.quit = true,
            "esc" | "?" => {
                // Return to the session when panes are still open — returning
                // to the list would strand any live session (help is
                // reachable via leader-?).
                self.mode = if self.panes.is_empty() {
                    Mode::List
                } else {
                    Mode::Session
                };
            }
            _ => self.overlay_scroll_key(s, self.help_lines().len()),
        }
    }

    /// Scroll keys shared by the detail and help screens. `total` is the
    /// content line count; one screen row is reserved for the hint bar.
    pub(super) fn overlay_scroll_key(&mut self, s: &str, total: usize) {
        let vis = (self.height as usize).saturating_sub(1).max(1);
        let max = total.saturating_sub(vis) as i64;
        let page = vis as i64 - 1;
        let cur = self.overlay_scroll as i64;
        let next = match s {
            "down" | "j" => cur + 1,
            "up" | "k" => cur - 1,
            "pgdown" | "ctrl+f" | "space" => cur + page,
            "pgup" | "ctrl+b" => cur - page,
            "ctrl+d" => cur + page / 2,
            "ctrl+u" => cur - page / 2,
            "g" | "home" => 0,
            "G" | "end" => max,
            _ => return,
        };
        self.overlay_scroll = next.clamp(0, max.max(0)) as usize;
    }

    pub(super) fn update_confirm(&mut self, s: &str) {
        let confirmed = s == "y" || s == "Y";
        match self.confirm_action {
            ConfirmKind::CloseSession => {
                if confirmed {
                    self.close_session();
                } else {
                    self.mode = Mode::Session; // cancel → back to the panes
                }
            }
            ConfirmKind::Reboot => {
                if confirmed {
                    let inst = self.confirm.clone();
                    self.mode = Mode::List;
                    self.status = format!("rebooting {}…", inst.name);
                    self.spawn_reboot(inst);
                } else {
                    self.mode = Mode::List;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_model;

    #[test]
    fn detail_scroll_keys_clamp() {
        let mut m = test_model();
        m.mode = crate::tui::Mode::Detail;
        m.height = 10; // content overflows → scrolling possible
        m.update_detail("j");
        assert_eq!(m.overlay_scroll, 1);
        m.update_detail("k");
        m.update_detail("k"); // clamps at 0
        assert_eq!(m.overlay_scroll, 0);
        m.update_detail("G");
        let max = m.overlay_scroll;
        assert!(max > 0, "G must jump to the bottom");
        m.update_detail("ctrl+f"); // already at max → clamped
        assert_eq!(m.overlay_scroll, max);
        m.update_detail("g");
        assert_eq!(m.overlay_scroll, 0);
    }

    #[test]
    fn profile_picker_filter() {
        let mut m = test_model();
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        m.picker_query = "cloud".into();
        assert_eq!(m.picker_filtered().len(), 2);
        m.picker_query = "cloude".into();
        assert!(m.picker_filtered().is_empty());
    }
}
