//! Rendering for the modal screens: the profile picker, the detail view,
//! the help overlay, and the confirmation dialog.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};

use super::{StateClass, classify_state, hints_line, pad1, refresh_label, state_color, state_mark};
use crate::theme;
use crate::tui::{ConfirmKind, Model, age_label, leader_label};
use crate::version;

// ---- profile picker ----

pub(super) fn draw_profiles(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < 4 {
        return;
    }
    let th = theme::current();
    let items = m.picker_filtered();

    let mut lines: Vec<Line> = vec![pad1(Line::from(Span::styled(
        " Select AWS profile ",
        Style::new()
            .fg(th.title_fg)
            .bg(th.title_bg)
            .add_modifier(Modifier::BOLD),
    )))];

    if m.picker_typing {
        let (before, cur, after) = m.picker_input.render_parts();
        lines.push(pad1(Line::from(vec![
            Span::raw("/"),
            Span::raw(before.to_string()),
            Span::styled(cur, Style::new().add_modifier(Modifier::REVERSED)),
            Span::raw(after.to_string()),
        ])));
    } else if !m.picker_query.is_empty() {
        lines.push(pad1(Line::from(Span::styled(
            format!("“{}” {} item(s)", m.picker_query, items.len()),
            Style::new().dim(),
        ))));
    } else {
        lines.push(pad1(Line::from(Span::styled(
            format!("{} item(s)", items.len()),
            Style::new().dim(),
        ))));
    }

    let vis = (area.height as usize).saturating_sub(3).max(1);
    let offset = (m.picker_cursor + 1).saturating_sub(vis);
    for (i, p) in items.iter().enumerate().skip(offset).take(vis) {
        let num = Span::styled(format!("{:3} ", i + 1), Style::new().dim());
        let line = if i == m.picker_cursor {
            Line::from(vec![
                num,
                Span::styled(
                    format!("▶ {p}"),
                    Style::new().fg(th.pink).add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from(vec![num, Span::raw(format!("  {p}"))])
        };
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), area);

    let hints = hints_line(&[
        ("↑↓", "move"),
        ("/", "filter"),
        ("enter", "select"),
        ("esc", "cancel"),
        ("q", "quit"),
    ]);
    f.render_widget(
        Paragraph::new(pad1(hints)),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
}

// ---- detail view ----

/// The full record of the selected instance, scrollable.
pub(super) fn draw_detail(m: &Model, f: &mut Frame) {
    scrolled_screen(
        f,
        m.detail_lines(),
        m.overlay_scroll,
        "s connect · ↑/↓ scroll · esc/d back · q quit",
    );
}

/// Renders content lines scrolled by `off`, with a fixed hint bar (plus a
/// line-position indicator when the content overflows) on the bottom row.
fn scrolled_screen(f: &mut Frame, lines: Vec<Line<'static>>, off: usize, hint: &str) {
    let area = f.area();
    if area.height < 2 {
        return;
    }
    let body = Rect::new(area.x, area.y, area.width, area.height - 1);
    let total = lines.len();
    let vis = body.height as usize;
    f.render_widget(Paragraph::new(lines).scroll((off as u16, 0)), body);

    let mut spans = vec![Span::styled(format!(" {hint}"), Style::new().dim())];
    if total > vis {
        spans.push(Span::styled(
            format!("   ({}–{}/{})", off + 1, (off + vis).min(total), total),
            Style::new().dim(),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
}

impl Model {
    /// The detail screen's content lines (hint bar excluded).
    pub(crate) fn detail_lines(&self) -> Vec<Line<'static>> {
        let th = theme::current();
        let inst = &self.detail;
        let label = Style::new().fg(th.gray);
        let value = Style::new().fg(th.value);
        let accent = Style::new().fg(th.accent);
        let dash = |s: &str| {
            if s.is_empty() {
                "-".to_string()
            } else {
                s.to_string()
            }
        };

        let mut lines: Vec<Line> = Vec::new();
        // header: host name + colored state chip
        let chip_bg = match classify_state(&inst.state) {
            StateClass::Running => th.chip_running_bg,
            StateClass::Down => th.chip_down_bg,
            StateClass::Other => th.chip_other_bg,
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", inst.name),
                Style::new()
                    .fg(th.chip_fg)
                    .bg(th.sel_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!(" {} ", inst.state),
                Style::new()
                    .fg(th.chip_fg)
                    .bg(chip_bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        let sec = |lines: &mut Vec<Line>, title: &str| {
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!("▍ {title}"),
                Style::new().fg(th.cyan).add_modifier(Modifier::BOLD),
            )));
        };
        let kv = |lines: &mut Vec<Line>, k: &str, v: Vec<Span<'static>>| {
            let mut spans = vec![Span::styled(format!("  {k:<11} "), label)];
            spans.extend(v);
            lines.push(Line::from(spans));
        };

        sec(&mut lines, "Overview");
        kv(
            &mut lines,
            "Instance",
            vec![Span::styled(inst.instance_id.clone(), accent)],
        );
        kv(
            &mut lines,
            "State",
            vec![
                Span::raw(format!("{} ", state_mark(&inst.state))),
                Span::styled(
                    inst.state.clone(),
                    Style::new().fg(state_color(&inst.state)),
                ),
            ],
        );
        kv(
            &mut lines,
            "Type",
            vec![Span::styled(dash(&inst.instance_type), value)],
        );
        kv(
            &mut lines,
            "Platform",
            vec![Span::styled(dash(&inst.platform), value)],
        );
        let launched: Vec<Span<'static>> = match inst.launch_time {
            None => vec![Span::raw("-")],
            // UTC, matching the --dry-run output.
            Some(t) => vec![
                Span::styled(t.format("%Y-%m-%d %H:%M").to_string(), value),
                Span::styled(format!("  ({} ago)", age_label(inst.launch_time)), label),
            ],
        };
        kv(&mut lines, "Launched", launched);

        sec(&mut lines, "Network");
        kv(
            &mut lines,
            "VPC",
            vec![Span::styled(dash(&inst.vpc_id), accent)],
        );
        kv(
            &mut lines,
            "Subnet",
            vec![Span::styled(dash(&inst.subnet_id), accent)],
        );
        kv(&mut lines, "AZ", vec![Span::styled(dash(&inst.az), value)]);
        kv(
            &mut lines,
            "Private IP",
            vec![Span::styled(dash(&inst.private_ip), value)],
        );
        kv(
            &mut lines,
            "Public IP",
            vec![Span::styled(dash(&inst.public_ip), value)],
        );

        sec(&mut lines, "SSM");
        match &inst.ssm {
            Some(ssm) => {
                let reach = if inst.is_connectable() {
                    Span::styled("reachable 🟢", Style::new().fg(th.green))
                } else {
                    Span::styled("not reachable 🔴", Style::new().fg(th.red))
                };
                kv(&mut lines, "Status", vec![reach]);
                kv(
                    &mut lines,
                    "Agent",
                    vec![Span::styled(dash(&ssm.agent_version), value)],
                );
                kv(
                    &mut lines,
                    "Ping",
                    vec![Span::styled(dash(&ssm.ping_status), value)],
                );
            }
            None => kv(
                &mut lines,
                "Status",
                vec![Span::styled("no SSM info 🔴", Style::new().fg(th.red))],
            ),
        }

        sec(&mut lines, "Security Groups");
        if inst.security_groups.is_empty() {
            lines.push(Line::from(Span::styled("  -", label)));
        }
        for sg in &inst.security_groups {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<22} ", sg.id), accent),
                Span::styled(dash(&sg.name), value),
            ]));
        }

        sec(&mut lines, "Tags");
        if inst.tags.is_empty() {
            lines.push(Line::from(Span::styled("  -", label)));
        }
        for (k, v) in &inst.tags {
            lines.push(Line::from(vec![
                Span::styled(format!("  {k:<24} "), label),
                Span::styled(dash(v), value),
            ]));
        }

        sec(&mut lines, "SSH / scp (via SSM)");
        lines.push(Line::from(Span::styled(
            format!("  ssh <user>@{}", inst.instance_id),
            accent,
        )));
        lines.push(Line::from(Span::styled(
            format!("  scp ./file <user>@{}:/tmp/", inst.instance_id),
            accent,
        )));
        lines.push(Line::from(Span::styled(
            "  run `skua --ssh-config` once · user = ec2-user / ubuntu / …",
            Style::new().dim(),
        )));

        lines
    }
}

// ---- help view ----

/// The full keybinding overlay (opened with ?), grouped and scrollable.
pub(super) fn draw_help(m: &Model, f: &mut Frame) {
    scrolled_screen(
        f,
        m.help_lines(),
        m.overlay_scroll,
        "esc / ? back · ↑/↓ scroll · q quit",
    );
}

impl Model {
    /// The help screen's content lines (hint bar excluded).
    pub(crate) fn help_lines(&self) -> Vec<Line<'static>> {
        let th = theme::current();
        let lead = leader_label(&self.leader);
        let mut lines = vec![pad1(Line::from(Span::styled(
            format!("skua {} — keys", version::VERSION),
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

        sec(&mut lines, "Filter & sort");
        r(
            &mut lines,
            "/ or f",
            "filter: name / id / ip / type / az / vpc / tag (! excludes)",
        );
        r(
            &mut lines,
            "enter",
            "add a nested filter level (narrows within results)",
        );
        r(&mut lines, "esc", "pop the last filter level");
        r(
            &mut lines,
            "N / S / T / A / P",
            "sort by name / state / type / age / ip (press again to reverse)",
        );

        sec(&mut lines, "Actions");
        r(&mut lines, "enter / d", "detail view of the selected host");
        r(&mut lines, "space", "mark / unmark host for multi-open");
        r(
            &mut lines,
            "s",
            "connect (marked hosts as split panes, else the selected one)",
        );
        r(
            &mut lines,
            "R",
            "reboot selected host (running only, confirmation required)",
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

#[cfg(test)]
mod tests {
    use super::super::test_util::{listed_model, render};
    use crate::tui::{ConfirmKind, Mode, test_model};

    #[test]
    fn renders_detail_help_confirm() {
        let mut m = listed_model();
        m.detail = m.all[0].clone();
        m.mode = Mode::Detail;
        let s = render(&m, 100, 40);
        assert!(s.contains("▍ Overview"), "detail sections missing:\n{s}");
        assert!(s.contains("reachable 🟢"), "ssm status missing:\n{s}");
        assert!(
            s.contains("ssh <user>@i-0aaa1111"),
            "ssh hint missing:\n{s}"
        );

        m.mode = Mode::Help;
        let s = render(&m, 100, 45);
        assert!(s.contains("— keys"), "help title missing:\n{s}");
        assert!(s.contains("^b h / l"), "leader rows missing:\n{s}");

        m.mode = Mode::Confirm;
        m.confirm_action = ConfirmKind::Reboot;
        m.confirm = m.all[0].clone();
        let s = render(&m, 100, 30);
        assert!(s.contains("Reboot this instance?"), "confirm missing:\n{s}");
        assert!(s.contains("[y] confirm"), "confirm keys missing:\n{s}");
    }

    #[test]
    fn detail_and_help_scroll() {
        let mut m = listed_model();
        m.detail = m.all[0].clone();
        m.mode = Mode::Detail;
        m.overlay_scroll = 5;
        let s = render(&m, 100, 12);
        assert!(
            !s.contains("▍ Overview"),
            "scrolled-off section shown:\n{s}"
        );
        assert!(s.contains("s connect"), "hint bar must stay fixed:\n{s}");
        assert!(s.contains("(6–"), "scroll position indicator missing:\n{s}");

        m.mode = Mode::Help;
        m.overlay_scroll = 4;
        let s = render(&m, 100, 20);
        assert!(
            !s.contains("▍ Navigate"),
            "scrolled-off section shown:\n{s}"
        );
        assert!(
            s.contains("▍ Filter & sort"),
            "section header missing:\n{s}"
        );
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
        assert!(s.contains("SSM instances"), "underlying list missing:\n{s}");
    }

    #[test]
    fn renders_profile_picker() {
        let mut m = test_model();
        m.mode = Mode::Profiles;
        m.profiles = vec!["Cloud.dev".into(), "Cloud.prod".into(), "personal".into()];
        let s = render(&m, 80, 20);
        assert!(s.contains("Select AWS profile"), "title missing:\n{s}");
        assert!(s.contains("▶ Cloud.dev"), "selection missing:\n{s}");
        assert!(s.contains("3 item(s)"), "count missing:\n{s}");
    }
}
