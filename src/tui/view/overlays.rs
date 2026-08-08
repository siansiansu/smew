//! Rendering for the modal screens: the profile picker, the detail view,
//! the help overlay, and the confirmation dialog.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use super::{hints_line, pad1, refresh_label};
use crate::theme;
use crate::tui::{ConfirmKind, FwdField, Model, leader_label};
use crate::version;

// ---- profile picker ----

pub(super) fn draw_profiles(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < 4 {
        return;
    }
    let th = theme::current();
    let items = m.picker_ranked();

    let mut lines: Vec<Line> = vec![pad1(Line::from(Span::styled(
        " Select AWS profile ",
        Style::new()
            .fg(th.title_fg)
            .bg(th.title_bg)
            .add_modifier(Modifier::BOLD),
    )))];

    // fzf-style prompt: always live — typing filters and re-ranks at once.
    let (before, cur, after) = m.picker_input.render_parts();
    lines.push(pad1(Line::from(vec![
        Span::styled("> ", Style::new().fg(th.pink).add_modifier(Modifier::BOLD)),
        Span::raw(before.to_string()),
        Span::styled(cur, Style::new().add_modifier(Modifier::REVERSED)),
        Span::raw(after.to_string()),
        Span::styled(
            format!("   {}/{} item(s)", items.len(), m.profiles.len()),
            Style::new().dim(),
        ),
    ])));

    let vis = (area.height as usize).saturating_sub(3).max(1);
    let offset = (m.picker_cursor + 1).saturating_sub(vis);
    let matched = Style::new().fg(th.orange).add_modifier(Modifier::BOLD);
    for (i, (p, positions)) in items.iter().enumerate().skip(offset).take(vis) {
        let selected = i == m.picker_cursor;
        let base = if selected {
            Style::new().fg(th.pink).add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        };
        let mut spans = vec![
            Span::styled(format!("{:3} ", i + 1), Style::new().dim()),
            Span::styled(if selected { "▶ " } else { "  " }, base),
        ];
        // fzf-style highlight: matched query chars pop in orange.
        for (ci, ch) in p.chars().enumerate() {
            let style = if positions.contains(&ci) {
                matched
            } else {
                base
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), area);

    let hints = hints_line(&[
        ("type", "filter"),
        ("↑↓", "move"),
        ("enter", "select"),
        ("esc", "clear / cancel"),
        ("ctrl+c", "quit"),
    ]);
    f.render_widget(
        Paragraph::new(pad1(hints)),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
}

// ---- help content ----

impl Model {
    /// The help screen's content lines (hint bar excluded).
    pub(crate) fn help_lines(&self) -> Vec<Line<'static>> {
        let th = theme::current();
        let lead = leader_label(&self.leader);
        let mut lines = vec![pad1(Line::from(Span::styled(
            format!("smew {} — keys", version::VERSION),
            Style::new().add_modifier(Modifier::BOLD),
        )))];
        let sec = |lines: &mut Vec<Line>, title: &str| {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("▍ {title}"),
                Style::new().fg(th.cyan).add_modifier(Modifier::BOLD),
            )));
        };
        let row = |lines: &mut Vec<Line>, k: String, v: String| {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {k:<18} "),
                    Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
                ),
                Span::raw(v),
            ]));
        };
        let r = |lines: &mut Vec<Line>, k: &str, v: &str| row(lines, k.into(), v.into());

        sec(&mut lines, "Navigate");
        r(&mut lines, "↑ / k, ↓ / j", "move up / down");
        r(&mut lines, "← / →", "scroll the table horizontally");
        r(&mut lines, "gg / G", "jump to top / bottom");
        r(
            &mut lines,
            "N gg / N G",
            "jump to row N (e.g. 10gg → row 10; see the # column)",
        );
        r(&mut lines, "ctrl+f / ctrl+b", "page down / up");
        r(&mut lines, "ctrl+d / ctrl+u", "half page down / up");

        sec(&mut lines, "Commands (:)");
        r(
            &mut lines,
            ":",
            "open the command prompt (tab completes · ↑ recalls the last command)",
        );
        r(
            &mut lines,
            ":<view>",
            "switch resource view — every view below (aws aliases work: ebs, fn, lb, sub, …)",
        );
        r(
            &mut lines,
            "enter (vpc/subnet/sg)",
            "drill into the instances of that container · d = raw details",
        );
        r(
            &mut lines,
            ":profile [name] / :ctx",
            "switch AWS profile (fuzzy; bare = open the picker)",
        );
        r(&mut lines, ":help / :q", "this help / quit");

        sec(&mut lines, "Resource views — by AWS category");
        {
            use crate::resources::{KINDS, ResourceKind};
            // group the registry by category, registry order preserved
            let mut cats: Vec<(&str, Vec<&str>)> = Vec::new();
            for k in std::iter::once(ResourceKind::Instances).chain(KINDS) {
                let c = k.category();
                match cats.iter_mut().find(|(cc, _)| *cc == c) {
                    Some((_, v)) => v.push(k.title()),
                    None => cats.push((c, vec![k.title()])),
                }
            }
            let key_w = cats
                .iter()
                .map(|(c, _)| c.chars().count())
                .max()
                .unwrap_or(0);
            for (c, kinds) in cats {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {c:<key_w$} "), Style::new().fg(th.gray)),
                    Span::styled(
                        format!(":{}", kinds.join("  :")),
                        Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }
        }

        sec(&mut lines, "Filter & sort");
        r(
            &mut lines,
            "/ or f",
            "filter: name / id / ip / type / az / vpc / tag (! excludes)",
        );
        r(&mut lines, "enter", "apply the filter and close the input");
        r(&mut lines, "esc", "clear the filter");
        r(
            &mut lines,
            "N / S / T / C / M / A / P",
            "sort by name / state / type / cpu / mem / age / ip (press again to reverse)",
        );

        sec(&mut lines, "Actions");
        r(&mut lines, "enter / d", "detail view of the selected host");
        r(&mut lines, "space", "mark / unmark host for multi-open");
        r(
            &mut lines,
            "s",
            "connect over SSM (marked hosts as split panes, else the selected one)",
        );
        r(
            &mut lines,
            "i",
            "SSH login via EC2 Instance Connect (pushes a 60s key, then ssh user@ip)",
        );
        r(
            &mut lines,
            "R",
            "reboot selected host (running only, confirmation required)",
        );
        r(
            &mut lines,
            "F",
            "port forward: local port → instance (or a remote host via it)",
        );
        r(&mut lines, "c", "switch AWS profile");
        r(&mut lines, "r / ctrl+r", "refresh inventory now");
        r(&mut lines, "?", "toggle this help");
        r(&mut lines, "q / ctrl+c", "quit");

        sec(
            &mut lines,
            &format!("Session — press the {lead} prefix, then"),
        );
        row(
            &mut lines,
            format!("{lead} h / l / ↑ / ↓"),
            "focus pane (↑/↓ move by grid row), then arrows keep moving".into(),
        );
        row(
            &mut lines,
            format!("{lead} space"),
            "add/remove focused pane from broadcast group — ≥2 auto-broadcasts (🔊)".into(),
        );
        row(
            &mut lines,
            format!("{lead} b"),
            "select all / clear the broadcast group".into(),
        );
        row(
            &mut lines,
            format!("{lead} v"),
            "cycle layout: columns → rows → grid".into(),
        );
        row(
            &mut lines,
            format!("{lead} z"),
            "zoom — toggle the focused pane full-screen".into(),
        );
        row(
            &mut lines,
            format!("{lead} ["),
            "scroll the pane's history (shell output only — less/vim scroll inside the app)".into(),
        );
        row(
            &mut lines,
            format!("{lead} a"),
            "add a pane (pick another host from the list)".into(),
        );
        row(
            &mut lines,
            format!("{lead} x"),
            "close the focused pane (disabled while broadcast is on)".into(),
        );
        row(
            &mut lines,
            format!("{lead} d"),
            "close the whole session — ends all SSM sessions (confirms)".into(),
        );
        row(
            &mut lines,
            format!("{lead} {lead}"),
            format!("send a literal {lead} to the shell"),
        );
        row(
            &mut lines,
            "exit (in shell)".into(),
            "ends that pane's SSM session and closes the pane; last pane returns to the list"
                .into(),
        );

        lines.push(Line::raw(""));
        if self.refresh > std::time::Duration::ZERO {
            lines.push(Line::raw(format!(
                "  auto-refresh: every {}",
                refresh_label(self.refresh)
            )));
        } else {
            lines.push(Line::raw(
                "  auto-refresh: off (set refresh_interval in config.yaml)",
            ));
        }
        lines
    }
}

// ---- confirm dialog ----

/// The confirmation dialog for the pending action, centered over the
/// underlying screen (drawn by the caller).
pub(super) fn draw_confirm(m: &Model, f: &mut Frame) {
    let body = match m.confirm_action {
        ConfirmKind::CloseSession => format!(
            "Close session?\n\nThis ends {} SSM session(s) and kills any\nrunning commands in them.\n\n[y] close    [n / esc] cancel",
            m.panes.len()
        ),
        ConfirmKind::Reboot => {
            let inst = &m.confirm;
            format!(
                "Reboot this instance?\n\n  {}\n  {}\n  {} / {}\n\n[y] confirm    [n / esc] cancel",
                inst.name, inst.instance_id, inst.state, inst.private_ip
            )
        }
    };
    let inner_w = body
        .lines()
        .map(unicode_width::UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let w = (inner_w + 8).min(f.area().width as usize) as u16; // padding 3+3, borders 2
    let h = (body.lines().count() + 4).min(f.area().height.saturating_sub(1) as usize) as u16;
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::current().red))
        .padding(Padding::new(3, 3, 1, 1));
    let fa = f.area();
    let area = Rect::new(
        fa.x + fa.width.saturating_sub(w) / 2,
        fa.y + fa.height.saturating_sub(h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(body).block(block), area);
}

// ---- port-forward form ----

/// The port-forward form, centered over the list (drawn by the caller).
/// Three fields; the focused one shows a cursor.
pub(super) fn draw_forward(m: &Model, f: &mut Frame) {
    let th = theme::current();
    let fwd = &m.fwd;

    let field = |label: &str,
                 input: &crate::tui::input::Input,
                 focused: bool,
                 note: &str|
     -> Line<'static> {
        let mut spans = vec![
            Span::styled(
                format!("{} ", if focused { "▶" } else { " " }),
                Style::new().fg(th.pink),
            ),
            Span::styled(format!("{label:<12}"), Style::new().fg(th.gray)),
        ];
        if focused {
            let (before, cur, after) = input.render_parts();
            spans.push(Span::raw(before.to_string()));
            spans.push(Span::styled(
                cur,
                Style::new().add_modifier(Modifier::REVERSED),
            ));
            spans.push(Span::raw(after.to_string()));
        } else if input.value().is_empty() {
            spans.push(Span::styled(note.to_string(), Style::new().dim()));
            return Line::from(spans);
        } else {
            spans.push(Span::raw(input.value().to_string()));
        }
        if !note.is_empty() {
            spans.push(Span::styled(format!("   {note}"), Style::new().dim()));
        }
        Line::from(spans)
    };

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "Port forward — {} ({})",
                fwd.target.name, fwd.target.instance_id
            ),
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        field(
            "Remote host",
            &fwd.host,
            fwd.field == FwdField::Host,
            "empty = the instance itself",
        ),
        field(
            "Remote port",
            &fwd.port,
            fwd.field == FwdField::Port,
            "e.g. 5432",
        ),
        field(
            "Local port",
            &fwd.local,
            fwd.field == FwdField::Local,
            "empty = same as remote",
        ),
        Line::raw(""),
    ];
    if fwd.error.is_empty() {
        lines.push(Line::from(Span::styled(
            "[enter] start    [tab/↑↓] field    [esc] cancel",
            Style::new().dim(),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("⚠ {}", fwd.error),
            Style::new().fg(th.red),
        )));
    }

    let inner_w = lines.iter().map(Line::width).max().unwrap_or(0).max(52);
    let w = (inner_w + 8).min(f.area().width as usize) as u16; // padding 3+3, borders 2
    let h = (lines.len() + 4).min(f.area().height.saturating_sub(1) as usize) as u16;
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.cyan))
        .padding(Padding::new(3, 3, 1, 1));
    let fa = f.area();
    let area = Rect::new(
        fa.x + fa.width.saturating_sub(w) / 2,
        fa.y + fa.height.saturating_sub(h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{listed_model, render};
    use crate::tui::{ConfirmKind, Mode, test_model};

    #[test]
    fn renders_help_and_confirm() {
        let mut m = listed_model();
        m.mode = Mode::Help;
        let s = render(&m, 100, 60);
        assert!(s.contains("— keys"), "help title missing:\n{s}");
        assert!(s.contains("^b h / l"), "leader rows missing:\n{s}");
        // resource views are grouped by AWS official category
        assert!(
            s.contains("Application Integration"),
            "categories missing:\n{s}"
        );
        assert!(s.contains(":lambda"), "new views missing:\n{s}");

        m.mode = Mode::Confirm;
        m.confirm_action = ConfirmKind::Reboot;
        m.confirm = m.all[0].clone();
        let s = render(&m, 100, 30);
        assert!(s.contains("Reboot this instance?"), "confirm missing:\n{s}");
        assert!(s.contains("[y] confirm"), "confirm keys missing:\n{s}");
    }

    #[test]
    fn help_scrolls_inside_the_frame() {
        let mut m = listed_model();
        m.mode = Mode::Help;
        m.overlay_scroll = 4;
        let s = render(&m, 100, 22);
        assert!(
            !s.contains("▍ Navigate"),
            "scrolled-off section shown:\n{s}"
        );
        assert!(s.contains("▍ Commands"), "section header missing:\n{s}");
    }

    #[test]
    fn confirm_dialog_is_centered() {
        let mut m = listed_model();
        m.mode = Mode::Confirm;
        m.confirm_action = ConfirmKind::Reboot;
        m.confirm = m.all[0].clone();
        let s = render(&m, 100, 30);
        let row = s
            .lines()
            .position(|l| l.contains("Reboot this instance?"))
            .expect("dialog body missing");
        assert!(
            (10..20).contains(&row),
            "dialog not vertically centered: row {row}\n{s}"
        );
        assert!(s.contains("INSTANCE-ID"), "underlying list missing:\n{s}");
    }

    #[test]
    fn renders_forward_form() {
        let mut m = listed_model();
        let inst = m.all[0].clone();
        m.open_forward_form(inst);
        let s = render(&m, 100, 30);
        assert!(
            s.contains("Port forward — web-prod-01 (i-0aaa1111)"),
            "title missing:\n{s}"
        );
        assert!(s.contains("▶ Remote host"), "focused field missing:\n{s}");
        assert!(s.contains("Remote port"), "field missing:\n{s}");
        assert!(s.contains("[enter] start"), "hints missing:\n{s}");

        // an error replaces the hint line
        m.fwd.error = "remote port: required, 1–65535".to_string();
        let s = render(&m, 100, 30);
        assert!(s.contains("⚠ remote port"), "error missing:\n{s}");
    }

    #[test]
    fn renders_profile_picker() {
        let mut m = test_model();
        m.mode = Mode::Profiles;
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        let s = render(&m, 80, 20);
        assert!(s.contains("Select AWS profile"), "title missing:\n{s}");
        assert!(s.contains("▶ Cloud.dev"), "selection missing:\n{s}");
        assert!(s.contains("3/3 item(s)"), "count missing:\n{s}");
    }
}
