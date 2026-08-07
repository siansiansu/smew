//! Update handlers for the small modal screens: the profile picker, the
//! detail view, help, and the confirmation dialog.

use crossterm::event::KeyEvent;

use super::{ConfirmKind, Mode, Model};

impl Model {
    /// fzf-style picker: printable keys type into the query and the list
    /// re-ranks immediately; arrows/ctrl+n/p move; enter selects; esc clears
    /// the query, then cancels (or quits when there is nothing to return to).
    pub(super) fn update_profiles(&mut self, k: &KeyEvent, s: &str) {
        match s {
            "ctrl+c" => self.quit = true,
            "esc" => {
                if !self.picker_input.value().is_empty() {
                    self.picker_input.clear();
                    self.picker_cursor = 0;
                } else if self.inventory.is_some() {
                    self.mode = Mode::List; // cancel only if we have a session
                } else {
                    self.quit = true; // startup picker: nothing to go back to
                }
            }
            "enter" => {
                if let Some(p) = self.picker_filtered().get(self.picker_cursor).cloned() {
                    self.select_profile(&p);
                }
            }
            "up" | "ctrl+p" => self.picker_cursor = self.picker_cursor.saturating_sub(1),
            "down" | "ctrl+n" => {
                let max = self.picker_filtered().len().saturating_sub(1);
                self.picker_cursor = (self.picker_cursor + 1).min(max);
            }
            "home" => self.picker_cursor = 0,
            "end" => self.picker_cursor = self.picker_filtered().len().saturating_sub(1),
            "pgup" => self.picker_cursor = self.picker_cursor.saturating_sub(10),
            "pgdown" => {
                let max = self.picker_filtered().len().saturating_sub(1);
                self.picker_cursor = (self.picker_cursor + 10).min(max);
            }
            _ => {
                if self.picker_input.handle(k) {
                    self.picker_cursor = 0;
                }
            }
        }
    }

    /// Profiles ranked by fuzzy score against the query (best first), with
    /// the matched char positions for highlighting. Empty query = all
    /// profiles in their natural order.
    pub(crate) fn picker_ranked(&self) -> Vec<(String, Vec<usize>)> {
        crate::fuzzy::rank(
            self.picker_input.value(),
            self.profiles.iter().map(String::as_str),
        )
        .into_iter()
        .map(|(i, fm)| (self.profiles[i].clone(), fm.positions))
        .collect()
    }

    /// The ranked profile names only (selection / mouse hit-testing).
    pub(crate) fn picker_filtered(&self) -> Vec<String> {
        self.picker_ranked().into_iter().map(|(p, _)| p).collect()
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
                self.identity = None; // re-resolved for the new profile
                self.mode = Mode::List;
                self.loading = true;
                self.status = format!("loading… ({p})");
                self.load_gen += 1; // in-flight loads for the old profile are stale
                self.all.clear();
                self.filtered.clear();
                self.res_all.clear();
                self.res_filtered.clear();
                self.res_kind = self.view; // the cache is gone, whatever it held
                self.drill_from = None;
                self.util.clear();
                self.last_util_fetch = None; // metrics must reload right away
                self.filter.clear();
                self.table_to_top();
                self.update_available = false; // re-check against the new profile
                self.spawn_load_active();
                self.spawn_identity();
                self.spawn_version_check();
            }
        }
    }

    pub(super) fn update_detail(&mut self, s: &str) {
        let is_instance = self.view == crate::resources::ResourceKind::Instances;
        match s {
            "q" | "ctrl+c" => self.quit = true,
            "esc" | "d" | "enter" => self.mode = Mode::List,
            "s" if is_instance => {
                let t = vec![self.detail.clone()];
                self.start_session(t);
            }
            _ => {
                let total = if is_instance {
                    self.detail_lines().len()
                } else {
                    self.res_detail_lines().len()
                };
                self.overlay_scroll_key(s, total);
            }
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
        let max = total.saturating_sub(vis);
        let page = vis.saturating_sub(1);
        let cur = self.overlay_scroll;
        let next = match s {
            "down" | "j" => cur.saturating_add(1),
            "up" | "k" => cur.saturating_sub(1),
            "pgdown" | "ctrl+f" | "space" => cur.saturating_add(page),
            "pgup" | "ctrl+b" => cur.saturating_sub(page),
            "ctrl+d" => cur.saturating_add(page / 2),
            "ctrl+u" => cur.saturating_sub(page / 2),
            "g" | "home" => 0,
            "G" | "end" => max,
            _ => return,
        };
        self.overlay_scroll = next.min(max);
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
    fn profile_picker_fuzzy_filter() {
        let mut m = test_model();
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        m.picker_input.insert_str("cloud");
        assert_eq!(m.picker_filtered().len(), 2);
        // fuzzy: subsequence with skipped chars still matches, best first
        m.picker_input.clear();
        m.picker_input.insert_str("cldprd");
        assert_eq!(m.picker_filtered(), vec!["Cloud.prod".to_string()]);
        m.picker_input.clear();
        m.picker_input.insert_str("zzz");
        assert!(m.picker_filtered().is_empty());
    }

    #[test]
    fn profile_picker_types_immediately_and_esc_clears() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut m = test_model();
        m.mode = crate::tui::Mode::Profiles;
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        // typing filters without any leading `/` (fzf-style)
        for c in "per".chars() {
            let k = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            let s = crate::tui::keymap::key_name(&k);
            m.update_profiles(&k, &s);
        }
        assert_eq!(m.picker_input.value(), "per");
        assert_eq!(m.picker_filtered(), vec!["personal".to_string()]);
        // esc clears the query first…
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        m.update_profiles(&esc, "esc");
        assert!(m.picker_input.value().is_empty());
        assert_eq!(m.picker_filtered().len(), 3);
        // …and with no query + no session, esc quits
        m.update_profiles(&esc, "esc");
        assert!(m.should_quit());
    }
}
