//! Rendering for the generic (non-instance) resource tables and their
//! key/value detail overlay. Same wide-buffer + blit pattern as the
//! instance table, but uniform styling: plain cells, warn/crit cells in
//! orange/red, the whole selected row highlighted.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;
use crate::tui::{LIST_ROW_H, Model};

/// Renders the resource table into a full-content-width buffer and blits
/// the horizontally-scrolled window into the frame (mirrors draw_table).
pub(super) fn draw_res_table(m: &Model, dst: &mut Buffer, area: Rect) {
    if area.height < 3 {
        return;
    }
    let th = theme::current();
    let titles = m.view.columns();
    let widths = m.res_col_widths();
    let row_h = LIST_ROW_H as u16;
    let content_w = m.content_width() as u16;
    let data_rows = ((area.height - 2) / row_h).min(m.visible_data_rows() as u16);
    let mut wide = Buffer::empty(Rect::new(0, 0, content_w, 2 + data_rows * row_h));

    // header + rule
    let header_style = Style::new().fg(th.cyan).add_modifier(Modifier::BOLD);
    let mut x = 0u16;
    for (title, w) in std::iter::once(&"#").chain(titles.iter()).zip(&widths) {
        wide.set_stringn(x + 1, 0, title, *w, header_style);
        x += *w as u16 + 2;
    }
    for rx in 0..content_w {
        wide[(rx, 1)].set_symbol("─");
    }

    for (vy, idx) in (m.row_offset..m.res_filtered.len())
        .take(data_rows as usize)
        .enumerate()
    {
        let row = &m.res_filtered[idx];
        let y = 2 + vy as u16 * row_h + (row_h - 1);
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
        let mut x = 0u16;
        let serial = format!("{}", idx + 1);
        wide.set_stringn(x + 1, y, &serial, widths[0], row_style);
        x += widths[0] as u16 + 2;
        for (ci, (cell, w)) in row.cells.iter().zip(widths.iter().skip(1)).enumerate() {
            // waste/exhaustion signals pop even on the selected row
            let style = if row.crit_cells.contains(&ci) {
                row_style.fg(th.red).add_modifier(Modifier::BOLD)
            } else if row.warn_cells.contains(&ci) {
                row_style.fg(th.orange).add_modifier(Modifier::BOLD)
            } else {
                row_style
            };
            wide.set_stringn(x + 1, y, cell, *w, style);
            x += *w as u16 + 2;
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

impl Model {
    /// The detail overlay's content for a non-instance resource: the row's
    /// full key/value record in the existing label/value style.
    pub(crate) fn res_detail_lines(&self) -> Vec<Line<'static>> {
        let th = theme::current();
        let row = &self.res_detail;
        let key_w = row
            .detail
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(0)
            .max(8);
        let mut lines = vec![
            Line::from(vec![Span::styled(
                format!(" {} ", row.cells.first().cloned().unwrap_or_default()),
                Style::new()
                    .fg(th.chip_fg)
                    .bg(th.sel_bg)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::raw(""),
            Line::from(Span::styled(
                format!("▍ {}", self.view.title()),
                Style::new().fg(th.cyan).add_modifier(Modifier::BOLD),
            )),
        ];
        for (k, v) in &row.detail {
            lines.push(Line::from(vec![
                Span::styled(format!("  {k:<key_w$} "), Style::new().fg(th.gray)),
                Span::styled(
                    if v.is_empty() {
                        "-".to_string()
                    } else {
                        v.clone()
                    },
                    Style::new().fg(th.value),
                ),
            ]));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::render;
    use crate::resources::{ResourceKind, mock};
    use crate::tui::test_model;

    fn resource_model(kind: ResourceKind) -> crate::tui::Model {
        let mut m = test_model();
        m.loading = false;
        m.view = kind;
        m.res_all = mock(kind);
        m.apply_filter();
        m.width = 140;
        m.height = 40;
        m
    }

    #[test]
    fn renders_volume_table() {
        let m = resource_model(ResourceKind::Volumes);
        let s = render(&m, 140, 40);
        assert!(s.contains("VOLUME-ID"), "header missing:\n{s}");
        assert!(s.contains("ATTACHED-TO"), "header missing:\n{s}");
        assert!(s.contains("vol(default)["), "panel title missing:\n{s}");
        assert!(s.contains("<vol>"), "crumbs chip missing:\n{s}");
        assert!(s.contains("web-01-root"), "row missing:\n{s}");
        assert!(s.contains("in-use"), "cell missing:\n{s}");
    }

    #[test]
    fn renders_subnet_table_with_free_ips() {
        let m = resource_model(ResourceKind::Subnets);
        let s = render(&m, 140, 40);
        assert!(s.contains("FREE-IPS"), "header missing:\n{s}");
        assert!(s.contains("dev-a-crowded"), "row missing:\n{s}");
    }

    #[test]
    fn renders_resource_detail() {
        let mut m = resource_model(ResourceKind::Eips);
        m.res_detail = m.res_filtered[0].clone();
        m.mode = crate::tui::Mode::Detail;
        let s = render(&m, 120, 40);
        assert!(s.contains("PUBLIC-IP"), "detail key missing:\n{s}");
        assert!(s.contains("203.0.113"), "detail value missing:\n{s}");
    }
}
