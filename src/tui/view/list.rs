//! Rendering for the instance list: header lines, the scrollable table, and
//! the status/hint bars.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{StateClass, classify_state, hints_line, pad1, refresh_label, state_mark};
use crate::theme;
use crate::tui::{Model, SortKey, age_label};
use crate::version;

const MARK_ONLINE: &str = "🟢"; // reachable via SSM
const MARK_OFFLINE: &str = "🔴"; // not reachable
const MARK_SELECTED: &str = "✅"; // space-marked for multi-open
const MARK_NONE: &str = "  ";

const NAME_COL: usize = 3; // index of the NAME column

pub(crate) struct Column {
    pub title: String,
    pub width: usize,
}

const BASE_COLUMNS: [(&str, usize); 11] = [
    ("#", 5), // row number (serial)
    ("", 3),  // multi-open mark (✅)
    ("SSM", 4),
    ("NAME", 34),
    ("INSTANCE-ID", 20),
    ("STATE", 14),
    ("TYPE", 11),
    ("AGE", 6),
    ("AZ", 15),
    ("PRIVATE-IP", 15),
    ("VPC", 22),
];

impl Model {
    /// Sizes NAME and marks the sort column. NAME is at least the longest
    /// name (never truncated) but flexes wider to fill the terminal; when
    /// names exceed the width the table scrolls horizontally instead.
    pub(crate) fn columns(&self) -> Vec<Column> {
        let mut cols: Vec<Column> = BASE_COLUMNS
            .iter()
            .map(|(t, w)| Column {
                title: t.to_string(),
                width: *w,
            })
            .collect();

        let mut name_w = self.name_width;
        if name_w < "NAME".len() {
            name_w = BASE_COLUMNS[NAME_COL].1; // fallback before data is loaded
        }
        // Flex NAME to fill leftover terminal width (never below longest name).
        let fixed: usize = cols
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != NAME_COL)
            .map(|(_, c)| c.width)
            .sum();
        let fill = self.width as i64 - fixed as i64 - 2 * cols.len() as i64;
        if fill > name_w as i64 {
            name_w = fill as usize;
        }
        cols[NAME_COL].width = name_w;

        let idx = match self.sort_by {
            SortKey::Name => 3,
            SortKey::State => 5,
            SortKey::Type => 6,
            SortKey::Launch => 7,
            SortKey::Ip => 9,
        };
        let arrow = if self.sort_asc { " ↑" } else { " ↓" };
        cols[idx].title.push_str(arrow);
        cols
    }
}

pub(super) fn draw_list(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < 5 || area.width < 10 {
        return;
    }
    let row = |y: u16, h: u16| Rect::new(area.x, area.y + y, area.width, h.min(area.height - y));

    f.render_widget(Paragraph::new(pad1(title_line(m))), row(0, 1));
    f.render_widget(Paragraph::new(pad1(summary_line(m))), row(1, 1));
    if let Some(fl) = filter_line(m) {
        f.render_widget(Paragraph::new(pad1(fl)), row(2, 1));
    }

    let data_rows = m.visible_data_rows() as u16;
    let table_area = row(3, data_rows + 2);
    draw_table(m, f.buffer_mut(), table_area);

    let status_y = 3 + (data_rows + 2).min(area.height.saturating_sub(3));
    if status_y + 1 < area.height {
        f.render_widget(Paragraph::new(pad1(status_bar(m))), row(status_y, 1));
        let hints = if m.filtering {
            pad1(Line::from(Span::styled(
                "type to filter · enter = add nested level · esc = cancel · prefix ! to exclude",
                Style::new().dim(),
            )))
        } else {
            pad1(hints_line(&[
                ("↑↓", "move"),
                ("/", "filter"),
                ("space", "mark"),
                ("s", "connect"),
                ("enter", "details"),
                ("c", "profile"),
                ("R", "reboot"),
                ("r", "refresh"),
                ("?", "help"),
                ("q", "quit"),
            ]))
        };
        f.render_widget(Paragraph::new(hints), row(status_y + 1, 1));
    }
}

fn title_line(m: &Model) -> Line<'static> {
    let mut title = format!("skua {} — SSM instances", version::VERSION);
    if !m.profile.is_empty() {
        title.push_str("  ·  ");
        title.push_str(&m.profile);
    }
    if m.adding_pane {
        title.push_str("   [adding pane — s to add · esc to cancel]");
    }
    if m.h_offset > 0 {
        title.push_str(&format!("   (scrolled →{})", m.h_offset));
    }
    if !m.count_buf.is_empty() {
        title.push_str("   goto:");
        title.push_str(&m.count_buf);
    }
    Line::from(Span::styled(
        title,
        Style::new().add_modifier(Modifier::BOLD),
    ))
}

/// The colored instance-count header.
fn summary_line(m: &Model) -> Line<'static> {
    let th = theme::current();
    let (total, running, stopped, other) = counts(m);
    let mut spans = vec![
        Span::raw(format!("Total: {total}    ")),
        Span::styled(format!("Running: {running}"), Style::new().fg(th.green)),
        Span::raw("    "),
        Span::styled(format!("Stopped: {stopped}"), Style::new().fg(th.red)),
        Span::raw("    "),
        Span::styled(format!("Other: {other}"), Style::new().fg(th.orange)),
    ];
    if m.update_available {
        spans.push(Span::raw("    "));
        spans.push(Span::styled(
            format!(" ⬆ update {} ", m.latest_version),
            Style::new()
                .bg(th.orange)
                .fg(th.notice_fg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" (you have {})", version::VERSION),
            Style::new().dim(),
        ));
    }
    Line::from(spans)
}

/// Tallies instances by lifecycle state for the summary line.
fn counts(m: &Model) -> (usize, usize, usize, usize) {
    let (mut running, mut stopped, mut other) = (0, 0, 0);
    for inst in &m.all {
        match classify_state(&inst.state) {
            StateClass::Running => running += 1,
            StateClass::Down => stopped += 1,
            StateClass::Other => other += 1,
        }
    }
    (m.all.len(), running, stopped, other)
}

/// Shows the committed nested filter levels and any in-progress term.
fn filter_line(m: &Model) -> Option<Line<'static>> {
    if !m.filtering && m.filter_stack.is_empty() {
        return None;
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    if !m.filter_stack.is_empty() {
        spans.push(Span::raw(format!(
            "filters: {}",
            m.filter_stack.join(" › ")
        )));
    }
    if m.filtering {
        if !spans.is_empty() {
            spans.push(Span::raw("  +  "));
        }
        spans.push(Span::raw("/"));
        let (before, cur, after) = m.filter.render_parts();
        if m.filter.value().is_empty() {
            spans.push(Span::styled(
                cur,
                Style::new().add_modifier(Modifier::REVERSED),
            ));
            spans.push(Span::styled(
                "filter (substring · space = AND · ! = exclude): name / id / ip / type / az / vpc / tag",
                Style::new().dim(),
            ));
        } else {
            spans.push(Span::raw(before.to_string()));
            spans.push(Span::styled(
                cur,
                Style::new().add_modifier(Modifier::REVERSED),
            ));
            spans.push(Span::raw(after.to_string()));
        }
    } else {
        spans.push(Span::raw(format!(
            "   ({}/{} · esc pops a level)",
            m.filtered.len(),
            m.all.len()
        )));
    }
    Some(Line::from(spans))
}

/// The colored bottom status bar.
fn status_bar(m: &Model) -> Line<'static> {
    let th = theme::current();
    let label = Style::new().fg(th.gray);
    let sep = Span::styled("  │  ", label);
    let mut spans = vec![
        Span::styled("Found ", label),
        Span::styled(
            format!("{}", m.filtered.len()),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(" instances", label),
        sep.clone(),
        Span::styled("Region: ", label),
        Span::styled(
            if m.region.is_empty() {
                "(default)".to_string()
            } else {
                m.region.clone()
            },
            Style::new().fg(th.cyan),
        ),
        sep.clone(),
        Span::styled("Synced: ", label),
        Span::styled(
            m.last_sync
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "—".into()),
            Style::new().fg(th.green),
        ),
        sep.clone(),
        Span::styled("Auto: ", label),
        Span::styled(auto_label(m), Style::new().fg(th.orange)),
    ];
    if !m.status.is_empty() {
        spans.push(sep);
        spans.push(Span::styled(m.status.clone(), Style::new().fg(th.red)));
    }
    Line::from(spans)
}

fn auto_label(m: &Model) -> String {
    if m.refresh > std::time::Duration::ZERO {
        format!("auto {}", refresh_label(m.refresh))
    } else {
        "manual".to_string()
    }
}

/// Renders the instance table into a full-content-width buffer and blits the
/// horizontally-scrolled window into the frame.
fn draw_table(m: &Model, dst: &mut Buffer, area: Rect) {
    if area.height < 3 {
        return;
    }
    let cols = m.columns();
    // content_width() is capped internally, so the buffer, the scroll bound
    // (max_h_offset) and this blit all agree on the same width.
    let content_w = m.content_width() as u16;
    let data_rows = (area.height - 2).min(m.visible_data_rows() as u16);
    let mut wide = Buffer::empty(Rect::new(0, 0, content_w, 2 + data_rows));

    // header + rule
    let th = theme::current();
    let header_style = Style::new().fg(th.cyan).add_modifier(Modifier::BOLD);
    let mut x = 0u16;
    for c in &cols {
        wide.set_stringn(x + 1, 0, &c.title, c.width, header_style);
        x += c.width as u16 + 2;
    }
    for rx in 0..content_w {
        wide[(rx, 1)].set_symbol("─");
    }

    for (vy, idx) in (m.row_offset..m.filtered.len())
        .take(data_rows as usize)
        .enumerate()
    {
        let inst = &m.filtered[idx];
        let y = 2 + vy as u16;
        let selected = idx == m.cursor;
        let row_style = if selected {
            Style::new()
                .fg(th.sel_fg)
                .bg(th.sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        if selected {
            for rx in 0..content_w {
                wide[(rx, y)].set_style(row_style);
            }
        }
        let cells: [String; 11] = [
            format!("{}", idx + 1),
            if m.marked.contains(&inst.instance_id) {
                MARK_SELECTED
            } else {
                MARK_NONE
            }
            .into(),
            if inst.is_connectable() {
                MARK_ONLINE
            } else {
                MARK_OFFLINE
            }
            .into(),
            inst.name.clone(),
            inst.instance_id.clone(),
            format!("{} {}", state_mark(&inst.state), inst.state),
            inst.instance_type.clone(),
            age_label(inst.launch_time),
            inst.az.clone(),
            inst.private_ip.clone(),
            inst.vpc_id.clone(),
        ];
        let mut x = 0u16;
        for (c, cell) in cols.iter().zip(cells.iter()) {
            wide.set_stringn(x + 1, y, cell, c.width, row_style);
            x += c.width as u16 + 2;
        }
    }

    // blit the visible window [h_offset, h_offset+width)
    let off = m.h_offset as u16;
    for y in 0..wide.area.height.min(area.height) {
        for x in 0..area.width.min(content_w.saturating_sub(off)) {
            if let Some(src) = wide.cell((x + off, y)) {
                dst[(area.x + x, area.y + y)] = src.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{listed_model, render};

    #[test]
    fn renders_list_table() {
        let m = listed_model();
        let s = render(&m, 120, 30);
        assert!(s.contains("SSM instances"), "title missing:\n{s}");
        assert!(s.contains("NAME ↑"), "sorted NAME header missing:\n{s}");
        assert!(s.contains("INSTANCE-ID"), "header missing:\n{s}");
        assert!(s.contains("web-prod-01"), "row missing:\n{s}");
        assert!(s.contains("i-0bbb2222"), "row missing:\n{s}");
        assert!(s.contains("Running: 1"), "summary missing:\n{s}");
        assert!(s.contains("Found 2 instances"), "status bar missing:\n{s}");
        assert!(s.contains("Region: ap-northeast-1"), "region missing:\n{s}");
    }

    #[test]
    fn renders_hscrolled_table() {
        let mut m = listed_model();
        m.h_offset = 12;
        let s = render(&m, 60, 30);
        assert!(
            s.contains("(scrolled →12)"),
            "scroll indicator missing:\n{s}"
        );
    }

    #[test]
    fn renders_filter_line_and_marks() {
        let mut m = listed_model();
        m.filter_stack = vec!["prod".to_string()];
        m.marked.insert("i-0aaa1111".to_string());
        m.apply_filter();
        let s = render(&m, 120, 30);
        assert!(s.contains("filters: prod"), "filter line missing:\n{s}");
        assert!(
            s.contains("(1/2 · esc pops a level)"),
            "filter count missing:\n{s}"
        );
        assert!(s.contains("✅"), "mark missing:\n{s}");
    }
}
