//! Rendering for the list / profile / detail / help / confirm screens.
//! Colors come from the active theme (crate::theme); the defaults are fixed
//! 256-color indexes.

mod list;
mod overlays;
mod resource;
mod session;

pub(crate) use list::{HEADER_H, PROMPT_H};

use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{ConfirmKind, Mode, Model};
use crate::theme;
use crate::version;

pub(crate) fn draw(m: &Model, f: &mut Frame) {
    match m.mode {
        Mode::Profiles => overlays::draw_profiles(m, f),
        Mode::Session => session::draw_session(m, f),
        _ if m.loading => draw_loading(f),
        Mode::Help => overlays::draw_help(m, f),
        Mode::Confirm => {
            // The dialog floats centered over the screen it interrupts.
            match m.confirm_action {
                ConfirmKind::CloseSession => session::draw_session(m, f),
                ConfirmKind::Reboot => list::draw_list(m, f),
            }
            overlays::draw_confirm(m, f);
        }
        Mode::Forward => {
            list::draw_list(m, f);
            overlays::draw_forward(m, f);
        }
        Mode::Detail if m.view != crate::resources::ResourceKind::Instances => {
            overlays::draw_res_detail(m, f)
        }
        Mode::Detail => overlays::draw_detail(m, f),
        Mode::List => list::draw_list(m, f),
    }
}

pub(super) fn pad1(line: Line) -> Line {
    // One cell of horizontal padding: a leading space.
    Line::from_iter(std::iter::once(Span::raw(" ")).chain(line.spans))
}

fn draw_loading(f: &mut Frame) {
    let lines = vec![
        pad1(Line::from(Span::styled(
            format!("smew {}", version::VERSION),
            Style::new().add_modifier(Modifier::BOLD),
        ))),
        pad1(Line::from(Span::styled(
            "Loading instances…",
            Style::new().dim(),
        ))),
    ];
    f.render_widget(Paragraph::new(lines), f.area());
}

/// EC2 lifecycle states bucketed for display (color, chip, counts).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum StateClass {
    Running,
    Down,
    Other,
}

pub(super) fn classify_state(state: &str) -> StateClass {
    match state.to_lowercase().as_str() {
        "running" => StateClass::Running,
        "stopped" | "stopping" | "terminated" | "shutting-down" => StateClass::Down,
        _ => StateClass::Other,
    }
}

/// The text color for a lifecycle state.
pub(super) fn state_color(state: &str) -> Color {
    let th = theme::current();
    match classify_state(state) {
        StateClass::Running => th.green,
        StateClass::Down => th.red,
        StateClass::Other => th.orange,
    }
}

/// The colored key:action hint bar.
pub(super) fn hints_line(pairs: &[(&'static str, &'static str)]) -> Line<'static> {
    let th = theme::current();
    let mut spans = Vec::new();
    for (i, (key, act)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            *key,
            Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(":{act}"), Style::new().fg(th.gray)));
    }
    Line::from(spans)
}

/// The refresh interval as compact humantime text, e.g. "30s" or "1h30m".
pub(super) fn refresh_label(d: std::time::Duration) -> String {
    humantime::format_duration(d).to_string().replace(' ', "")
}

#[cfg(test)]
mod test_util {
    use crate::inventory::{Instance, SsmStatus};
    use crate::tui::{max_name_width, test_model};
    use ratatui::{Terminal, backend::TestBackend};

    pub(crate) fn inst(name: &str, id: &str, state: &str, online: bool) -> Instance {
        Instance {
            instance_id: id.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            instance_type: "t3.large".to_string(),
            az: "ap-northeast-1a".to_string(),
            private_ip: "10.0.1.23".to_string(),
            vpc_id: "vpc-0abc".to_string(),
            launch_time: Some(chrono::Utc::now() - chrono::Duration::hours(5)),
            ssm: online.then(|| SsmStatus {
                online: true,
                agent_version: "3.3".into(),
                ping_status: "Online".into(),
            }),
            ..Default::default()
        }
    }

    pub(crate) fn render(m: &crate::tui::Model, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::draw(m, f)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    pub(crate) fn listed_model() -> crate::tui::Model {
        let mut m = test_model();
        m.loading = false;
        m.all = vec![
            inst("web-prod-01", "i-0aaa1111", "running", true),
            inst("db-bastion", "i-0bbb2222", "stopped", false),
        ];
        m.name_width = max_name_width(&m.all);
        m.region = "ap-northeast-1".to_string();
        m.apply_filter();
        m
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::render;
    use crate::tui::{Mode, test_model};

    #[test]
    fn renders_session_panes() {
        let mut m = test_model();
        m.mode = Mode::Session;
        let argv: Vec<String> = ["sh", "-c", "echo pane-marker; sleep 30"]
            .map(String::from)
            .into();
        let notify = std::sync::Arc::new(|| {});
        let p1 = crate::session::Pane::start("host-a", &argv, 40, 10, notify.clone()).unwrap();
        let p2 = crate::session::Pane::start("host-b", &argv, 40, 10, notify).unwrap();
        m.panes = vec![p1, p2];
        m.relayout_session();
        // give the shells a moment to print
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline
            && !m.panes[0].contents_text().contains("pane-marker")
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = render(&m, 100, 30);
        assert!(s.contains("▶ host-a"), "focused pane title missing:\n{s}");
        assert!(s.contains("host-b"), "second pane title missing:\n{s}");
        assert!(s.contains("pane-marker"), "pane content missing:\n{s}");
        assert!(s.contains("2 pane(s)"), "session header missing:\n{s}");
        assert!(s.contains("╭"), "pane borders missing:\n{s}");

        // zoomed: borderless, only the focused pane
        m.zoomed = true;
        m.relayout_session();
        let s = render(&m, 100, 30);
        assert!(!s.contains("╭"), "zoom must be borderless:\n{s}");
        assert!(s.contains("▶ host-a"), "zoom title missing:\n{s}");

        // which-key popup while the leader prefix is pending (also over zoom)
        m.leader_pending = true;
        let s = render(&m, 100, 30);
        assert!(
            s.contains("^b — command"),
            "leader menu title missing:\n{s}"
        );
        assert!(s.contains("end session"), "leader menu rows missing:\n{s}");
        assert!(s.contains("scrollback"), "leader menu rows missing:\n{s}");
        for p in &m.panes {
            p.close();
        }
    }
}
