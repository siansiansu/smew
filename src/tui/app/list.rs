//! List-mode behavior: filtering, sorting, vim-style navigation, and the
//! instance-table geometry.

use crossterm::event::KeyEvent;

use crate::inventory::{Instance, Utilization};
use crate::resources::ResourceKind;

use super::{ConfirmKind, Mode, Model, SortKey};

/// Screen lines per instance row: compact single-line rows, k9s-style.
pub(crate) const LIST_ROW_H: usize = 1;

impl Model {
    /// Routes keys while the instance list is on screen.
    pub(super) fn update_list(&mut self, k: &KeyEvent, s: &str) {
        if self.commanding {
            return self.update_command(k, s);
        }
        if self.filtering {
            return self.update_filtering(k, s);
        }

        // vim-style count + gg/G jump (e.g. "10gg" → row 10). Accumulate
        // digits; any other key cancels the pending motion.
        if let [b] = s.as_bytes()
            && b.is_ascii_digit()
        {
            self.count_buf.push_str(s);
            self.g_pending = false;
            return;
        }
        match s {
            "g" => {
                if self.g_pending {
                    // second g of gg
                    let n = self.count_target(1);
                    self.jump_to(n);
                    self.g_pending = false;
                    self.count_buf.clear();
                } else {
                    self.g_pending = true;
                }
                return;
            }
            "G" => {
                let n = self.count_target(self.row_count());
                self.jump_to(n);
                self.g_pending = false;
                self.count_buf.clear();
                return;
            }
            _ => {}
        }
        // any other key cancels the motion
        self.g_pending = false;
        self.count_buf.clear();

        // Non-instance views get the reduced key set (no sessions, marks,
        // reboot or port-forward — those are EC2 concepts).
        if self.view != ResourceKind::Instances {
            return self.update_resource_list(s);
        }

        match s {
            "q" | "ctrl+c" => self.quit = true,
            "?" => {
                self.overlay_scroll = 0;
                self.mode = Mode::Help;
            }
            "R" => {
                if let Some(inst) = self.filtered.get(self.cursor).cloned() {
                    if !inst.state.eq_ignore_ascii_case("running") {
                        self.status = format!(
                            "cannot reboot {}: state is {:?} (must be running)",
                            inst.name, inst.state
                        );
                        return;
                    }
                    self.confirm = inst;
                    self.confirm_action = ConfirmKind::Reboot;
                    self.mode = Mode::Confirm;
                }
            }
            "c" => {
                if !self.profiles.is_empty() {
                    self.mode = Mode::Profiles;
                }
            }
            "F" => {
                if let Some(inst) = self.filtered.get(self.cursor).cloned() {
                    if inst.is_connectable() {
                        self.open_forward_form(inst);
                    } else {
                        self.status = format!("not connectable via SSM: {}", inst.name);
                    }
                }
            }
            "enter" | "d" => {
                if let Some(inst) = self.filtered.get(self.cursor).cloned() {
                    self.detail = inst;
                    self.overlay_scroll = 0;
                    self.mode = Mode::Detail;
                }
            }
            "s" => {
                if self.adding_pane {
                    self.add_pane_from_list();
                } else {
                    let targets = self.selected_targets();
                    self.start_session(targets);
                }
            }
            "i" => {
                let targets = self.selected_ssh_targets();
                self.start_ssh_session(targets);
            }
            "space" => {
                if let Some(inst) = self.filtered.get(self.cursor)
                    && inst.is_connectable()
                {
                    let id = inst.instance_id.clone();
                    if !self.marked.remove(&id) {
                        self.marked.insert(id);
                    }
                    self.status = format!("{} marked for multi-open", self.marked.len());
                }
            }
            "left" | "right" => {
                let step = 8usize;
                if s == "left" {
                    self.h_offset = self.h_offset.saturating_sub(step);
                } else {
                    self.h_offset += step;
                }
                self.clamp_h_offset();
            }
            "/" | "f" => {
                // reopen with the current query intact so it can be refined
                self.filtering = true;
                self.ensure_cursor_visible(); // the prompt box takes 3 rows
            }
            ":" => self.open_command(),
            "esc" => {
                // after a drill-down, esc walks back to the originating view
                if let Some((kind, cursor)) = self.drill_from.take() {
                    self.drill_back(kind, cursor);
                    return;
                }
                if !self.filter.value().is_empty() {
                    self.filter.clear();
                    self.apply_filter();
                    self.table_to_top();
                    return;
                }
                if self.adding_pane {
                    // cancel add-pane, return to the session
                    self.adding_pane = false;
                    self.mode = Mode::Session;
                    self.status.clear();
                }
            }
            "r" | "ctrl+r" => {
                // Refresh in place — keep the table visible; the row set
                // updates when the reload returns.
                self.status = "refreshing…".to_string();
                self.spawn_load_active();
            }
            "N" => self.sort_by_key(SortKey::Name),
            "S" => self.sort_by_key(SortKey::State),
            "T" => self.sort_by_key(SortKey::Type),
            "C" if self.metrics_enabled => self.sort_by_key(SortKey::Cpu),
            "M" if self.metrics_enabled => self.sort_by_key(SortKey::Mem),
            "A" => self.sort_by_key(SortKey::Launch),
            "P" => self.sort_by_key(SortKey::Ip),
            _ => self.table_key(s),
        }
    }

    fn update_filtering(&mut self, k: &KeyEvent, s: &str) {
        match s {
            "esc" => {
                // clear the query and show everything again
                self.filtering = false;
                self.filter.clear();
                self.apply_filter();
                self.table_to_top();
            }
            "enter" => {
                // close the input; the query stays applied
                self.filtering = false;
                self.ensure_cursor_visible(); // the table got its rows back
            }
            _ => {
                self.filter.handle(k);
                self.apply_filter();
                self.table_to_top(); // reset cursor + viewport as the query changes
            }
        }
    }

    // ---- table navigation ----

    /// Visible data rows in the instance table for the current height.
    /// Chrome: top panel (7) + panel border (2) + table header + rule +
    /// bottom bar = 12 rows, plus the prompt box while it is open.
    pub(crate) fn visible_data_rows(&self) -> usize {
        let chrome = 12 + self.prompt_h() as usize;
        ((self.height as usize).saturating_sub(chrome) / LIST_ROW_H).max(1)
    }

    /// Height currently taken by the filter/command prompt box.
    pub(crate) fn prompt_h(&self) -> u16 {
        if self.filtering || self.commanding {
            crate::tui::view::PROMPT_H
        } else {
            0
        }
    }

    /// First screen row of table data — the mouse hit-testing counterpart of
    /// view/list.rs draw_list geometry: top panel, optional prompt box, then
    /// the panel's top border, the header row and the rule line.
    pub(crate) fn list_data_y(&self) -> u16 {
        crate::tui::view::HEADER_H + self.prompt_h() + 3
    }

    /// Rows of whichever table is active (drives all cursor math).
    pub(crate) fn row_count(&self) -> usize {
        if self.view == ResourceKind::Instances {
            self.filtered.len()
        } else {
            self.res_filtered.len()
        }
    }

    pub(super) fn table_key(&mut self, s: &str) {
        let vis = self.visible_data_rows();
        match s {
            "up" => self.cursor_up(1),
            "down" => self.cursor_down(1),
            "k" => self.cursor_up(1),
            "j" => self.cursor_down(1),
            "ctrl+b" | "pgup" => self.cursor_up(vis),
            "ctrl+f" | "pgdown" => self.cursor_down(vis),
            "ctrl+u" => self.cursor_up(vis / 2),
            "ctrl+d" => self.cursor_down(vis / 2),
            "home" => self.table_to_top(),
            "end" => self.jump_to(self.row_count()),
            _ => {}
        }
    }

    pub(super) fn cursor_up(&mut self, n: usize) {
        self.cursor = self.cursor.saturating_sub(n.max(1));
        self.ensure_cursor_visible();
    }

    pub(super) fn cursor_down(&mut self, n: usize) {
        if self.row_count() == 0 {
            return;
        }
        self.cursor = (self.cursor + n.max(1)).min(self.row_count() - 1);
        self.ensure_cursor_visible();
    }

    /// Resets the cursor AND the viewport offset to the top.
    pub(super) fn table_to_top(&mut self) {
        self.cursor = 0;
        self.row_offset = 0;
    }

    pub(super) fn ensure_cursor_visible(&mut self) {
        let vis = self.visible_data_rows();
        if self.cursor < self.row_offset {
            self.row_offset = self.cursor;
        }
        if self.cursor >= self.row_offset + vis {
            self.row_offset = self.cursor + 1 - vis;
        }
        let max_off = self.row_count().saturating_sub(vis);
        self.row_offset = self.row_offset.min(max_off);
    }

    /// The pending numeric prefix (1-based), or def if none.
    fn count_target(&self, def: usize) -> usize {
        if self.count_buf.is_empty() {
            return def;
        }
        match self.count_buf.parse::<usize>() {
            Ok(n) if n >= 1 => n,
            _ => def,
        }
    }

    /// Moves the cursor to the 1-based row n, keeping the viewport correct.
    pub(super) fn jump_to(&mut self, n: usize) {
        let total = self.row_count();
        if total == 0 {
            return;
        }
        self.cursor = n.clamp(1, total) - 1;
        self.ensure_cursor_visible();
    }

    // ---- sorting / filtering ----

    fn sort_by_key(&mut self, k: SortKey) {
        if self.sort_by == k {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_by = k;
            self.sort_asc = true;
        }
        self.sort_all();
        self.clamp_h_offset();
        let dir = if self.sort_asc { "↑" } else { "↓" };
        self.status = format!("sorted by {} {}", self.sort_by.label(), dir);
    }

    pub(super) fn sort_all(&mut self) {
        let key = self.sort_by;
        let asc = self.sort_asc;
        let util = &self.util;
        // Utilization sorts on the numeric value; hosts without data sort
        // below 0% (they'd otherwise interleave as strings, the k9s wart).
        let pct = |inst: &Instance, f: fn(&Utilization) -> Option<f64>| {
            util.get(&inst.instance_id).and_then(f).unwrap_or(-1.0)
        };
        self.all.sort_by(|x, y| {
            let (a, b) = if asc { (x, y) } else { (y, x) };
            match key {
                SortKey::State => a.state.cmp(&b.state),
                SortKey::Type => a.instance_type.cmp(&b.instance_type),
                SortKey::Cpu => pct(a, |u| u.cpu).total_cmp(&pct(b, |u| u.cpu)),
                SortKey::Mem => pct(a, |u| u.mem).total_cmp(&pct(b, |u| u.mem)),
                SortKey::Launch => a.launch_time.cmp(&b.launch_time),
                SortKey::Ip => a.private_ip.cmp(&b.private_ip),
                SortKey::Name => a.name.cmp(&b.name),
            }
        });
        self.apply_filter();
    }

    pub(crate) fn apply_filter(&mut self) {
        let cur = self.filter.value().trim().to_string();
        self.filtered = self
            .all
            .iter()
            .filter(|inst| matches_term(inst, &cur))
            .cloned()
            .collect();
        self.res_filtered = self
            .res_all
            .iter()
            .filter(|row| matches_tokens(&row.cells.join(" ").to_lowercase(), &cur))
            .cloned()
            .collect();
        if self.cursor >= self.row_count() {
            self.cursor = 0;
        }
        self.ensure_cursor_visible();
    }

    /// The instances to open: all marked (if any), else the instance under
    /// the cursor. Non-connectable instances are dropped.
    /// SSH connect targets: needs an IP, not SSM reachability.
    fn selected_ssh_targets(&self) -> Vec<Instance> {
        let has_ip = |i: &Instance| !i.public_ip.is_empty() || !i.private_ip.is_empty();
        if !self.marked.is_empty() {
            return self
                .all
                .iter()
                .filter(|inst| self.marked.contains(&inst.instance_id) && has_ip(inst))
                .cloned()
                .collect();
        }
        self.filtered
            .get(self.cursor)
            .filter(|inst| has_ip(inst))
            .cloned()
            .map(|i| vec![i])
            .unwrap_or_default()
    }

    fn selected_targets(&self) -> Vec<Instance> {
        if !self.marked.is_empty() {
            return self
                .all
                .iter()
                .filter(|inst| self.marked.contains(&inst.instance_id) && inst.is_connectable())
                .cloned()
                .collect();
        }
        self.filtered
            .get(self.cursor)
            .filter(|inst| inst.is_connectable())
            .cloned()
            .map(|i| vec![i])
            .unwrap_or_default()
    }

    // ---- geometry ----

    /// The full rendered table width (all columns + cell padding), capped so
    /// the render buffer, the horizontal-scroll bound, and the blit window
    /// all agree even for pathological Name-tag widths.
    pub(crate) fn content_width(&self) -> usize {
        if self.view != ResourceKind::Instances {
            let w = self.res_col_widths();
            return (w.iter().sum::<usize>() + 2 * w.len()).min(8192);
        }
        let cols = self.columns();
        (cols.iter().map(|c| c.width).sum::<usize>() + 2 * cols.len()).min(8192)
    }

    /// How far the table can scroll horizontally. The visible window is the
    /// panel interior: terminal width minus the two border columns.
    fn max_h_offset(&self) -> usize {
        self.content_width()
            .saturating_sub((self.width as usize).saturating_sub(2))
    }

    pub(crate) fn clamp_h_offset(&mut self) {
        self.h_offset = self.h_offset.min(self.max_h_offset());
    }
}

/// Whether an instance passes the filter query. Matching is by substring
/// (predictable — no fzf-style character scatter). Whitespace splits the
/// query into tokens that are AND-ed, so "prod redis" matches names/fields
/// containing both, in any order; a "!" prefix negates a token ("prod !db"
/// = contains prod AND not db). An empty query matches everything.
fn matches_term(inst: &Instance, term: &str) -> bool {
    matches_tokens(&search_text(inst), term)
}

/// The token/negation matcher over a pre-lowercased haystack (shared by the
/// instance table and the generic resource tables).
pub(super) fn matches_tokens(hay: &str, term: &str) -> bool {
    term.to_lowercase()
        .split_whitespace()
        .all(|tok| match tok.strip_prefix('!') {
            Some(rest) if !rest.is_empty() => !hay.contains(rest),
            Some(_) => true, // a bare "!" mid-typing matches everything
            None => hay.contains(tok),
        })
}

/// The lowercased, space-joined haystack of an instance's fields. Security
/// group ids/names are included so the sg view can drill into instances.
fn search_text(inst: &Instance) -> String {
    let mut parts = [
        inst.name.as_str(),
        inst.instance_id.as_str(),
        inst.private_ip.as_str(),
        inst.public_ip.as_str(),
        inst.instance_type.as_str(),
        inst.az.as_str(),
        inst.vpc_id.as_str(),
        inst.subnet_id.as_str(),
        inst.state.as_str(),
    ]
    .join(" ");
    // SG ids only — names are generic by convention ("web", "default") and
    // would pollute name-based filtering; drill always navigates by id.
    for sg in &inst.security_groups {
        parts.push(' ');
        parts.push_str(&sg.id);
    }
    for (k, v) in &inst.tags {
        parts.push(' ');
        parts.push_str(k);
        parts.push('=');
        parts.push_str(v);
    }
    parts.to_lowercase()
}

pub(crate) fn max_name_width(insts: &[Instance]) -> usize {
    insts
        .iter()
        .map(|inst| unicode_width::UnicodeWidthStr::width(inst.name.as_str()))
        .max()
        .unwrap_or(0)
        .max("NAME".len())
}

#[cfg(test)]
mod tests {
    use super::super::test_model;
    use super::*;

    fn inst(name: &str, id: &str, state: &str, ip: &str, online: bool) -> Instance {
        Instance {
            instance_id: id.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            instance_type: "t3.large".to_string(),
            private_ip: ip.to_string(),
            ssm: online.then(|| crate::inventory::SsmStatus {
                online: true,
                agent_version: "3.3".into(),
                ping_status: "Online".into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn filter_terms_and_negation() {
        let a = inst("web-prod-01", "i-0aaa", "running", "10.0.1.23", true);
        assert!(matches_term(&a, "prod"));
        assert!(matches_term(&a, "PROD web"));
        assert!(!matches_term(&a, "prod redis"));
        assert!(matches_term(&a, "!redis"));
        assert!(!matches_term(&a, "!prod"));
        assert!(matches_term(&a, "prod !redis")); // mixed include + exclude
        assert!(!matches_term(&a, "web !prod"));
        assert!(matches_term(&a, "10.0.1"));
        assert!(matches_term(&a, "")); // empty matches everything
    }

    #[test]
    fn filter_includes_tags() {
        let mut a = inst("db", "i-1", "running", "10.0.0.1", false);
        a.tags.insert("env".into(), "prod".into());
        assert!(matches_term(&a, "env=prod"));
        assert!(matches_term(&a, "prod"));
    }

    #[test]
    fn filter_narrows_and_esc_clears() {
        let mut m = test_model();
        m.all = vec![
            inst("web-prod-01", "i-1", "running", "10.0.0.1", true),
            inst("web-stg-01", "i-2", "running", "10.0.0.2", true),
            inst("db-prod-01", "i-3", "stopped", "10.0.0.3", false),
        ];
        m.filter.insert_str("prod !db");
        m.apply_filter();
        assert_eq!(m.filtered.len(), 1);
        assert_eq!(m.filtered[0].name, "web-prod-01");
        // esc in list mode clears the applied query
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let k = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        m.update_list(&k, "esc");
        assert!(m.filter.value().is_empty());
        assert_eq!(m.filtered.len(), 3);
    }

    #[test]
    fn sorting_and_reverse() {
        let mut m = test_model();
        m.all = vec![
            inst("bbb", "i-1", "stopped", "10.0.0.2", false),
            inst("aaa", "i-2", "running", "10.0.0.1", true),
        ];
        m.sort_all();
        assert_eq!(m.filtered[0].name, "aaa");
        m.sort_by_key(SortKey::Name); // same key → reverse
        assert_eq!(m.filtered[0].name, "bbb");
        m.sort_by_key(SortKey::State); // new key → ascending
        assert_eq!(m.filtered[0].state, "running");
    }

    #[test]
    fn f_filters_and_ctrl_keys_page() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut m = test_model();
        m.all = (0..50)
            .map(|i| inst(&format!("h{i}"), &format!("i-{i}"), "running", "", true))
            .collect();
        m.apply_filter();
        let key = |m: &mut crate::tui::Model, code: KeyCode, mods: KeyModifiers| {
            let k = KeyEvent::new(code, mods);
            let s = crate::tui::keymap::key_name(&k);
            m.update_list(&k, &s);
        };
        // ctrl+f / ctrl+b page; the old letter bindings must be gone
        key(&mut m, KeyCode::Char('f'), KeyModifiers::CONTROL);
        assert!(m.cursor > 0, "ctrl+f must page down");
        let at = m.cursor;
        key(&mut m, KeyCode::Char('b'), KeyModifiers::CONTROL);
        assert!(m.cursor < at, "ctrl+b must page up");
        // f is now a filter alias
        key(&mut m, KeyCode::Char('f'), KeyModifiers::NONE);
        assert!(m.filtering, "f must start filtering");
    }

    #[test]
    fn count_and_jump() {
        let mut m = test_model();
        m.all = (0..50)
            .map(|i| {
                inst(
                    &format!("host-{i:02}"),
                    &format!("i-{i}"),
                    "running",
                    "",
                    true,
                )
            })
            .collect();
        m.apply_filter();
        m.count_buf = "10".to_string();
        let n = m.count_target(1);
        m.jump_to(n);
        assert_eq!(m.cursor, 9);
        m.jump_to(9999);
        assert_eq!(m.cursor, 49);
        m.jump_to(0);
        assert_eq!(m.cursor, 0);
    }

    #[test]
    fn selected_targets_marked_and_cursor() {
        let mut m = test_model();
        m.all = vec![
            inst("a", "i-1", "running", "", true),
            inst("b", "i-2", "running", "", false), // not connectable
            inst("c", "i-3", "running", "", true),
        ];
        m.apply_filter();
        // cursor target
        m.cursor = 0;
        assert_eq!(m.selected_targets().len(), 1);
        m.cursor = 1; // not connectable → empty
        assert!(m.selected_targets().is_empty());
        // marked targets win over cursor; non-connectable dropped
        m.marked.insert("i-1".to_string());
        m.marked.insert("i-2".to_string());
        m.marked.insert("i-3".to_string());
        let t = m.selected_targets();
        assert_eq!(t.len(), 2);
    }
}
