//! List-mode behavior for the non-instance resource views: the reduced key
//! set (navigation, filter, command, detail, drill-down) — sessions, marks,
//! reboot and port-forward stay EC2-only.

use crate::resources::ResourceKind;

use super::{Mode, Model};

impl Model {
    /// Routes action keys while a non-instance view is on screen. Motions
    /// (counts, gg/G) and the command/filter prompts are already handled by
    /// update_list before it branches here.
    pub(super) fn update_resource_list(&mut self, s: &str) {
        match s {
            "q" | "ctrl+c" => self.quit = true,
            "?" => {
                self.overlay_scroll = 0;
                self.mode = Mode::Help;
            }
            "c" => {
                if !self.profiles.is_empty() {
                    self.picker_input.clear();
                    self.picker_cursor = 0;
                    self.mode = Mode::Profiles;
                }
            }
            "enter" | "d" => {
                let Some(row) = self.res_filtered.get(self.cursor).cloned() else {
                    return;
                };
                // Enter drills into the instances of a vpc/subnet/sg (the
                // k9s namespace-drill analog); d always opens the detail.
                if s == "enter" && self.view.drills_to_instances() {
                    self.drill_to_instances(&row.id);
                } else {
                    self.res_detail = row;
                    self.overlay_scroll = 0;
                    self.mode = Mode::Detail;
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
                self.filtering = true;
                self.ensure_cursor_visible(); // the prompt box takes 3 rows
            }
            ":" => self.open_command(),
            "esc" => {
                if !self.filter.value().is_empty() {
                    self.filter.clear();
                    self.apply_filter();
                    self.table_to_top();
                } else {
                    // nothing to clear: esc returns home (the ec2 view)
                    self.switch_view(ResourceKind::Instances);
                }
            }
            "r" | "ctrl+r" => {
                self.status = "refreshing…".to_string();
                self.spawn_load_resources();
            }
            _ => self.table_key(s),
        }
    }

    /// Column widths of the generic table: a serial `#` column, then every
    /// column auto-fit to its title and widest cell (capped — very long
    /// DESCRIPTIONs truncate; the detail view has the full text). Widths
    /// derive from the unfiltered rows so they don't jump while filtering.
    pub(crate) fn res_col_widths(&self) -> Vec<usize> {
        const CAP: usize = 40;
        let cols = self.view.columns();
        let mut w = vec![4]; // "#" serial
        w.extend(cols.iter().map(|c| c.chars().count()));
        for row in &self.res_all {
            for (i, cell) in row.cells.iter().enumerate() {
                if i + 1 < w.len() {
                    let cw = unicode_width::UnicodeWidthStr::width(cell.as_str()).min(CAP);
                    w[i + 1] = w[i + 1].max(cw);
                }
            }
        }
        w
    }

    /// Jumps to the instances view pre-filtered by a container's id
    /// ("which instances are in this vpc/subnet/sg?"). Esc there pops back
    /// to the originating view, cursor restored.
    pub(super) fn drill_to_instances(&mut self, id: &str) {
        let from = (self.view, self.cursor);
        self.switch_view(ResourceKind::Instances);
        self.filter.insert_str(id);
        self.apply_filter();
        self.table_to_top();
        self.drill_from = Some(from); // after switch_view (which clears it)
        self.status = format!("instances matching {id} — esc goes back");
    }

    /// Pops the one-level drill stack: back to the view Enter was pressed
    /// in, cached rows shown immediately, cursor restored.
    pub(super) fn drill_back(&mut self, kind: ResourceKind, cursor: usize) {
        self.filter.clear();
        self.apply_filter();
        self.switch_view(kind);
        self.cursor = cursor.min(self.row_count().saturating_sub(1));
        self.ensure_cursor_visible();
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_model;
    use crate::resources::{ResourceKind, mock};

    fn key(m: &mut crate::tui::Model, s: &str) {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let code = match s {
            "enter" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            c if c.len() == 1 => KeyCode::Char(c.chars().next().unwrap()),
            _ => panic!("unsupported test key {s}"),
        };
        let k = KeyEvent::new(code, KeyModifiers::NONE);
        m.update_list(&k, s);
    }

    fn resource_model(kind: ResourceKind) -> crate::tui::Model {
        let mut m = test_model();
        m.view = kind;
        m.res_kind = kind; // as Msg::ResourceLoaded would set it
        m.res_all = mock(kind);
        m.apply_filter();
        m
    }

    #[test]
    fn nav_and_detail_on_resource_rows() {
        let mut m = resource_model(ResourceKind::Volumes);
        assert!(m.res_filtered.len() >= 3);
        key(&mut m, "j");
        assert_eq!(m.cursor, 1);
        key(&mut m, "d");
        assert_eq!(m.mode, crate::tui::Mode::Detail);
        assert_eq!(m.res_detail.id, m.res_filtered[1].id);
        // enter on a volume (no drill target) also opens the detail
        m.mode = crate::tui::Mode::List;
        key(&mut m, "enter");
        assert_eq!(m.mode, crate::tui::Mode::Detail);
    }

    #[test]
    fn enter_on_vpc_drills_to_filtered_instances() {
        let mut m = resource_model(ResourceKind::Vpcs);
        let vpc_id = m.res_filtered[0].id.clone();
        key(&mut m, "enter");
        assert_eq!(m.view, ResourceKind::Instances);
        assert_eq!(m.filter.value(), vpc_id);
    }

    #[test]
    fn esc_after_drill_pops_back_with_cursor() {
        let mut m = resource_model(ResourceKind::Vpcs);
        key(&mut m, "j"); // drill from row 1, not row 0
        let at = m.cursor;
        key(&mut m, "enter");
        assert_eq!(m.view, ResourceKind::Instances);
        // esc walks back to the vpc view: cached rows, cursor restored
        key(&mut m, "esc");
        assert_eq!(m.view, ResourceKind::Vpcs);
        assert_eq!(m.cursor, at, "cursor must be restored");
        assert!(!m.res_filtered.is_empty(), "cached rows must render");
        assert!(m.filter.value().is_empty(), "drill filter must be gone");
        // a second esc (no drill context) goes home to ec2
        key(&mut m, "esc");
        assert_eq!(m.view, ResourceKind::Instances);
    }

    #[test]
    fn lateral_view_switch_clears_drill_back() {
        let mut m = resource_model(ResourceKind::SecurityGroups);
        key(&mut m, "enter"); // drill sg → ec2
        assert_eq!(m.view, ResourceKind::Instances);
        m.switch_view(ResourceKind::Volumes); // lateral move (like :vol)
        key(&mut m, "esc");
        // esc must NOT jump back to sg — the drill context died on :vol
        assert_eq!(m.view, ResourceKind::Instances);
    }

    #[test]
    fn filter_narrows_resource_rows() {
        let mut m = resource_model(ResourceKind::Volumes);
        let total = m.res_filtered.len();
        m.filter.insert_str("available");
        m.apply_filter();
        assert!(m.res_filtered.len() < total);
        assert!(!m.res_filtered.is_empty());
        // negation works over cells too
        m.filter.clear();
        m.filter.insert_str("!available");
        m.apply_filter();
        assert!(
            m.res_filtered
                .iter()
                .all(|r| !r.cells.contains(&"available".to_string()))
        );
    }

    #[test]
    fn esc_returns_to_instances() {
        let mut m = resource_model(ResourceKind::Eips);
        key(&mut m, "esc");
        assert_eq!(m.view, ResourceKind::Instances);
    }

    #[test]
    fn command_switches_views() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut m = test_model();
        m.open_command();
        for c in "sg".chars() {
            let k = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            m.update_command(&k, &c.to_string());
        }
        let k = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        m.update_command(&k, "enter");
        assert_eq!(m.view, ResourceKind::SecurityGroups);
        assert!(m.status.contains("loading sg"), "status: {}", m.status);
    }
}
