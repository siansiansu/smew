//! Rendering for the run-command flow: the multi-line script editor and
//! the per-host results page.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::theme;
use crate::tui::Model;

/// The editor page (`x`): a framed multi-line script box over the header,
/// listing the target hosts in the title.
pub(super) fn draw_run_cmd_page(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < super::HEADER_H + 4 || area.width < 24 {
        return;
    }
    super::list::draw_header(m, f, Rect::new(area.x, area.y, area.width, super::HEADER_H));

    let th = theme::current();
    let panel = Rect::new(
        area.x,
        area.y + super::HEADER_H,
        area.width,
        area.height - super::HEADER_H - 1,
    );
    let names: Vec<&str> = m.cmd_targets.iter().map(|t| t.name.as_str()).collect();
    let mut title = format!(
        " run command on {} host(s): {} ",
        names.len(),
        names.join(", ")
    );
    if title.chars().count() + 2 > panel.width as usize {
        title = format!(" run command on {} host(s) ", names.len());
    }
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.accent).add_modifier(Modifier::BOLD))
        .title(Line::from(Span::styled(
            title,
            Style::new().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(
            Line::from(Span::styled(
                " enter = new line · ctrl+s = run · esc = cancel ",
                Style::new().dim(),
            ))
            .right_aligned(),
        );
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let (crow, ccol) = m.cmd_editor.cursor();
    let mut lines: Vec<Line> = Vec::with_capacity(m.cmd_editor.lines().len());
    for (i, l) in m.cmd_editor.lines().iter().enumerate() {
        if i != crow {
            lines.push(Line::raw(format!(" {l}")));
            continue;
        }
        // the cursor line: split at the cursor and reverse one cell
        let chars: Vec<char> = l.chars().collect();
        let before: String = chars[..ccol].iter().collect();
        let cur = chars.get(ccol).map(|c| c.to_string()).unwrap_or(" ".into());
        let after: String = chars
            .get(ccol + 1..)
            .map(|s| s.iter().collect())
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::raw(format!(" {before}")),
            Span::styled(cur, Style::new().add_modifier(Modifier::REVERSED)),
            Span::raw(after),
        ]));
    }
    // keep the cursor line visible when the script outgrows the box
    let skip = (crow + 1).saturating_sub(inner.height as usize);
    f.render_widget(
        Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()),
        inner,
    );
}

impl Model {
    /// The results page's content lines (per-host header + indented output).
    pub(crate) fn cmd_results_lines(&self) -> Vec<Line<'static>> {
        let th = theme::current();
        let mut lines = Vec::new();
        for r in &self.cmd_results {
            let color = match r.status.as_str() {
                "Success" => th.green,
                "Pending" | "InProgress" | "Delayed" => th.orange,
                _ => th.red,
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ({}) ", r.name, r.instance_id),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {} ", r.status),
                    Style::new()
                        .bg(color)
                        .fg(th.badge_fg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            for l in r.output.lines() {
                lines.push(Line::raw(format!("   {l}")));
            }
            lines.push(Line::raw(""));
        }
        lines
    }

    /// Total height of the results content (scroll clamping).
    pub(crate) fn cmd_results_height(&self) -> usize {
        self.cmd_results_lines().len()
    }
}

/// The results page: one section per host with a status chip and the
/// command output, scrollable.
pub(super) fn draw_cmd_results_page(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < super::HEADER_H + 4 || area.width < 24 {
        return;
    }
    super::list::draw_header(m, f, Rect::new(area.x, area.y, area.width, super::HEADER_H));

    let th = theme::current();
    let panel = Rect::new(
        area.x,
        area.y + super::HEADER_H,
        area.width,
        area.height - super::HEADER_H - 1,
    );
    let done = m.cmd_results.iter().filter(|r| r.is_done()).count();
    let state = if m.cmd_all_done() {
        "done".to_string()
    } else {
        format!("{done}/{} done — polling…", m.cmd_results.len())
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.accent).add_modifier(Modifier::BOLD))
        .title(Line::from(Span::styled(
            format!(" command results — {state} "),
            Style::new().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(
            Line::from(Span::styled(
                " ↑↓ scroll · x = edit & re-run · esc = back ",
                Style::new().dim(),
            ))
            .right_aligned(),
        );
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    let lines = m.cmd_results_lines();
    let visible: Vec<Line> = lines
        .into_iter()
        .skip(m.overlay_scroll)
        .take(inner.height as usize)
        .collect();
    f.render_widget(Paragraph::new(visible), inner);
}
