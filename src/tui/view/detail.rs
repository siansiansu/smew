//! The describe page and the help page, sharing the list page's chrome:
//! top panel · framed main panel · crumbs bar.
//!
//! The describe page is a dashboard of bordered sub-panels named after the
//! AWS console's section tabs (Details / Networking / Security / Storage /
//! Monitoring / Tags), packed into 1–3 columns to keep everything on one
//! screen instead of a single scrolling column. Panels are laid out into a
//! full-content-height buffer and blitted through overlay_scroll — the same
//! wide-buffer pattern the tables use, turned vertical.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

use super::{StateClass, classify_state};
use crate::theme;
use crate::tui::{Model, age_label};

/// One dashboard sub-panel: an AWS-official section title and its lines.
struct Panel {
    title: String,
    lines: Vec<Line<'static>>,
}

/// Minimum sub-panel column width; the column count follows from it.
const MIN_COL_W: u16 = 55;

fn ncols(inner_w: u16) -> usize {
    ((inner_w / MIN_COL_W) as usize).clamp(1, 3)
}

/// key/value rows → panel lines. Long values wrap onto continuation lines
/// (blank key) so panel heights stay deterministic and nothing is clipped.
fn kv_lines(rows: &[(String, String)], inner_w: usize) -> Vec<Line<'static>> {
    let th = theme::current();
    let key_w = rows
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 24);
    let val_w = inner_w.saturating_sub(key_w + 3).max(8);
    let mut lines = Vec::new();
    for (k, v) in rows {
        let v = if v.is_empty() { "-" } else { v.as_str() };
        let chars: Vec<char> = v.chars().collect();
        let mut start = 0;
        let mut first = true;
        while first || start < chars.len() {
            let end = (start + val_w).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            let key = if first { k.as_str() } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!(" {key:<key_w$} "), Style::new().fg(th.gray)),
                Span::styled(chunk, Style::new().fg(th.value)),
            ]));
            first = false;
            start = end.max(start + 1);
        }
    }
    lines
}

/// The panels of a non-instance resource: straight from the row's grouped
/// detail record.
fn resource_panels(m: &Model, inner_w: usize) -> Vec<Panel> {
    m.res_detail
        .detail
        .iter()
        .map(|(title, rows)| Panel {
            title: title.clone(),
            lines: kv_lines(rows, inner_w),
        })
        .collect()
}

/// The panels of an EC2 instance, named after the AWS console's instance
/// tabs: Details, Networking, Security, Storage, Monitoring, Connect, Tags.
fn instance_panels(m: &Model, inner_w: usize) -> Vec<Panel> {
    let th = theme::current();
    let inst = &m.detail;
    let kv = |title: &str, rows: Vec<(&str, String)>| Panel {
        title: title.to_string(),
        lines: kv_lines(
            &rows
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<Vec<_>>(),
            inner_w,
        ),
    };

    let launched = match inst.launch_time {
        None => "-".to_string(),
        Some(t) => format!(
            "{}  ({} ago)",
            t.format("%Y-%m-%d %H:%M"),
            age_label(inst.launch_time)
        ),
    };
    let mut panels = vec![
        kv(
            "Details",
            vec![
                ("Instance", inst.instance_id.clone()),
                ("State", inst.state.clone()),
                ("Type", inst.instance_type.clone()),
                ("Platform", inst.platform.clone()),
                ("AMI", inst.image_id.clone()),
                ("Launched", launched),
            ],
        ),
        kv(
            "Networking",
            vec![
                ("VPC", inst.vpc_id.clone()),
                ("Subnet", inst.subnet_id.clone()),
                ("AZ", inst.az.clone()),
                ("Private IP", inst.private_ip.clone()),
                ("Public IP", inst.public_ip.clone()),
            ],
        ),
    ];

    // Security: one row per security group (id → name).
    let sg_rows: Vec<(String, String)> = if inst.security_groups.is_empty() {
        vec![("Groups".to_string(), "-".to_string())]
    } else {
        inst.security_groups
            .iter()
            .map(|sg| (sg.id.clone(), sg.name.clone()))
            .collect()
    };
    panels.push(Panel {
        title: "Security".to_string(),
        lines: kv_lines(&sg_rows, inner_w),
    });

    // Storage: root device + every attached volume.
    let mut st_rows = vec![("Root device".to_string(), inst.root_device.clone())];
    st_rows.extend(inst.volumes.iter().cloned());
    panels.push(Panel {
        title: "Storage".to_string(),
        lines: kv_lines(&st_rows, inner_w),
    });

    // Monitoring: only when the %CPU/%MEM columns are on at all.
    if m.metrics_enabled {
        let u = m.util.get(&inst.instance_id).copied().unwrap_or_default();
        let pct = |v: Option<f64>| match v {
            Some(v) => format!("{:.0} %", v.clamp(0.0, 100.0)),
            None => "n/a".to_string(),
        };
        panels.push(kv(
            "Monitoring",
            vec![("CPU", pct(u.cpu)), ("Memory", pct(u.mem))],
        ));
    }

    // Connect: SSM reachability + the ssh/scp recipes.
    let mut connect = match &inst.ssm {
        Some(ssm) => {
            let (label, color) = if inst.is_connectable() {
                ("reachable", th.green)
            } else {
                ("not reachable", th.red)
            };
            let mut l = vec![Line::from(vec![
                Span::styled(" SSM       ", Style::new().fg(th.gray)),
                Span::styled(label, Style::new().fg(color).add_modifier(Modifier::BOLD)),
            ])];
            l.extend(kv_lines(
                &[
                    ("Agent".to_string(), ssm.agent_version.clone()),
                    ("Ping".to_string(), ssm.ping_status.clone()),
                ],
                inner_w,
            ));
            l
        }
        None => vec![Line::from(vec![
            Span::styled(" SSM       ", Style::new().fg(th.gray)),
            Span::styled("no SSM info", Style::new().fg(th.red)),
        ])],
    };
    connect.push(Line::from(Span::styled(
        format!(" ssh <user>@{}", inst.instance_id),
        Style::new().fg(th.accent),
    )));
    connect.push(Line::from(Span::styled(
        format!(" scp ./file <user>@{}:/tmp/", inst.instance_id),
        Style::new().fg(th.accent),
    )));
    connect.push(Line::from(Span::styled(
        " `smew --ssh-config` once · user = ec2-user / ubuntu / …",
        Style::new().dim(),
    )));
    panels.push(Panel {
        title: "Connect".to_string(),
        lines: connect,
    });

    if !inst.tags.is_empty() {
        let rows: Vec<(String, String)> = inst
            .tags
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        panels.push(Panel {
            title: "Tags".to_string(),
            lines: kv_lines(&rows, inner_w),
        });
    }
    panels
}

/// Packs panels into `cols` columns (greedy: next panel goes to the
/// currently shortest column). Returns (x, y, h) per panel + total height.
fn pack(heights: &[u16], cols: usize, col_w: u16) -> (Vec<(u16, u16)>, u16) {
    let mut col_h = vec![0u16; cols.max(1)];
    let mut pos = Vec::with_capacity(heights.len());
    for &h in heights {
        let c = (0..col_h.len()).min_by_key(|&c| col_h[c]).unwrap_or(0);
        pos.push((c as u16 * col_w, col_h[c]));
        col_h[c] += h;
    }
    (pos, col_h.into_iter().max().unwrap_or(0))
}

fn build_panels(m: &Model, inner_w: u16) -> (Vec<Panel>, u16, usize) {
    let cols = ncols(inner_w);
    let col_w = inner_w / cols as u16;
    let content_w = col_w.saturating_sub(2) as usize; // panel borders
    let panels = if m.view == crate::resources::ResourceKind::Instances {
        instance_panels(m, content_w)
    } else {
        resource_panels(m, content_w)
    };
    (panels, col_w, cols)
}

impl Model {
    /// Total packed height of the describe dashboard (scroll clamping).
    pub(crate) fn detail_content_height(&self) -> usize {
        let inner_w = self.width.saturating_sub(2);
        if inner_w == 0 {
            return 0;
        }
        let (panels, col_w, cols) = build_panels(self, inner_w);
        let heights: Vec<u16> = panels.iter().map(|p| p.lines.len() as u16 + 2).collect();
        pack(&heights, cols, col_w).1 as usize
    }

    /// Rows visible inside the framed main panel of the describe/help pages
    /// (header, frame borders and the crumbs bar subtracted).
    pub(crate) fn page_rows(&self) -> usize {
        (self.height as usize)
            .saturating_sub(super::HEADER_H as usize + 3)
            .max(1)
    }
}

/// The framed title of the describe page: `view/name` + a state chip for
/// instances (same embedded-title style as the list panel).
fn detail_title(m: &Model) -> Vec<Span<'static>> {
    let th = theme::current();
    let name = if m.view == crate::resources::ResourceKind::Instances {
        m.detail.name.clone()
    } else {
        m.res_detail.cells.first().cloned().unwrap_or_default()
    };
    let mut title = vec![
        Span::raw(" "),
        Span::styled(
            format!("{}/", m.view.title()),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            name,
            Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
        ),
    ];
    if m.view == crate::resources::ResourceKind::Instances {
        let chip_bg = match classify_state(&m.detail.state) {
            StateClass::Running => th.chip_running_bg,
            StateClass::Down => th.chip_down_bg,
            StateClass::Other => th.chip_other_bg,
        };
        title.push(Span::raw(" "));
        title.push(Span::styled(
            format!(" {} ", m.detail.state),
            Style::new().fg(th.chip_fg).bg(chip_bg),
        ));
    }
    title.push(Span::raw(" "));
    title
}

/// The describe page: header · framed dashboard of grouped panels · crumbs.
pub(super) fn draw_detail_page(m: &Model, f: &mut Frame) {
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
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.accent).add_modifier(Modifier::BOLD))
        .title(Line::from(detail_title(m)).centered());
    let inner = block.inner(panel);
    f.render_widget(block, panel);

    // Lay the panels out in a full-height buffer, then blit the visible
    // window (vertical scroll).
    let (panels, col_w, cols) = build_panels(m, inner.width);
    let heights: Vec<u16> = panels.iter().map(|p| p.lines.len() as u16 + 2).collect();
    let (pos, total_h) = pack(&heights, cols, col_w);
    if total_h == 0 {
        return;
    }
    let mut wide = Buffer::empty(Rect::new(0, 0, inner.width, total_h));
    for ((p, &h), &(x, y)) in panels.iter().zip(&heights).zip(&pos) {
        let b = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(th.border_unfocused))
            .title(Span::styled(
                format!(" {} ", p.title),
                Style::new().fg(th.cyan).add_modifier(Modifier::BOLD),
            ));
        let r = Rect::new(x, y, col_w, h);
        let ib = b.inner(r);
        (&b).render(r, &mut wide);
        Paragraph::new(p.lines.clone()).render(ib, &mut wide);
    }
    let off = (m.overlay_scroll as u16).min(total_h.saturating_sub(inner.height));
    for y in 0..inner.height.min(total_h) {
        for x in 0..inner.width {
            if let Some(src) = wide.cell((x, y + off)) {
                f.buffer_mut()[(inner.x + x, inner.y + y)] = src.clone();
            }
        }
    }

    let hint = if m.view == crate::resources::ResourceKind::Instances {
        "s connect · ↑/↓ scroll · esc/d back · q quit"
    } else {
        "↑/↓ scroll · esc/d back · q quit"
    };
    draw_crumbs(
        m,
        f,
        format!("<{}/describe>", m.view.title()),
        hint,
        (off as usize, inner.height as usize, total_h as usize),
    );
}

/// The help page in the same chrome: header · framed content · crumbs.
pub(super) fn draw_help_page(m: &Model, f: &mut Frame) {
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
    let lines = m.help_lines();
    let total = lines.len();
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.accent).add_modifier(Modifier::BOLD))
        .title(
            Line::from(Span::styled(
                " help ",
                Style::new().add_modifier(Modifier::BOLD),
            ))
            .centered(),
        );
    let inner = block.inner(panel);
    f.render_widget(block, panel);
    f.render_widget(
        Paragraph::new(lines).scroll((m.overlay_scroll as u16, 0)),
        inner,
    );

    draw_crumbs(
        m,
        f,
        "<help>".to_string(),
        "esc / ? back · ↑/↓ scroll · q quit",
        (m.overlay_scroll, inner.height as usize, total),
    );
}

/// The crumbs bar of the describe/help pages: view chip, key hints, and a
/// line-position indicator when the content overflows.
fn draw_crumbs(m: &Model, f: &mut Frame, chip: String, hint: &str, scroll: (usize, usize, usize)) {
    let th = theme::current();
    let (off, vis, total) = scroll;
    let mut spans = vec![Span::styled(
        format!(" {chip} "),
        Style::new()
            .fg(th.title_fg)
            .bg(th.title_bg)
            .add_modifier(Modifier::BOLD),
    )];
    // position first: the hint truncates before the indicator does
    if total > vis {
        spans.push(Span::styled(
            format!("  ({}–{}/{})", off + 1, (off + vis).min(total), total),
            Style::new().dim(),
        ));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(hint.to_string(), Style::new().dim()));
    let area = f.area();
    let _ = m;
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{listed_model, render};
    use crate::resources::{ResourceKind, mock};
    use crate::tui::Mode;

    #[test]
    fn instance_describe_renders_dashboard_panels() {
        let mut m = listed_model();
        m.width = 140;
        m.height = 44;
        m.detail = m.all[0].clone();
        m.detail.image_id = "ami-0base1".into();
        m.detail.root_device = "/dev/xvda (ebs)".into();
        m.detail.volumes = vec![("/dev/xvda".into(), "vol-0aaa1".into())];
        m.detail.tags.insert("env".into(), "dev".into());
        m.mode = Mode::Detail;
        let s = render(&m, 140, 44);
        // console-tab panels, side by side (dashboard, not a single column)
        for p in [
            "Details",
            "Networking",
            "Security",
            "Storage",
            "Connect",
            "Tags",
        ] {
            assert!(s.contains(p), "panel {p} missing:\n{s}");
        }
        assert!(s.contains("vol-0aaa1"), "storage volume missing:\n{s}");
        assert!(
            s.contains("ssh <user>@i-0aaa1111"),
            "ssh hint missing:\n{s}"
        );
        // same chrome as the list: header info block + framed panel + crumbs
        assert!(s.contains("Region:"), "header missing:\n{s}");
        assert!(s.contains("╭"), "frame missing:\n{s}");
        assert!(s.contains("<ec2/describe>"), "crumbs chip missing:\n{s}");
        assert!(s.contains("ec2/web-prod-01"), "panel title missing:\n{s}");
        // two panels share a row → dashboard columns are in effect
        assert!(
            s.lines()
                .any(|l| l.contains("Details") && l.contains("Networking")),
            "panels must sit in columns:\n{s}"
        );
    }

    #[test]
    fn resource_describe_renders_grouped_panels() {
        let mut m = listed_model();
        m.width = 140;
        m.height = 40;
        m.view = ResourceKind::Volumes;
        m.res_all = mock(ResourceKind::Volumes);
        m.apply_filter();
        m.res_detail = m.res_filtered[0].clone();
        m.mode = Mode::Detail;
        let s = render(&m, 140, 40);
        assert!(s.contains("Details"), "details panel missing:\n{s}");
        assert!(s.contains("Tags"), "tags panel missing:\n{s}");
        assert!(s.contains("<vol/describe>"), "crumbs chip missing:\n{s}");
        assert!(s.contains("vol/"), "title missing:\n{s}");
    }

    #[test]
    fn help_page_shares_the_chrome() {
        let mut m = listed_model();
        m.mode = Mode::Help;
        let s = render(&m, 120, 50);
        assert!(s.contains("— keys"), "help content missing:\n{s}");
        assert!(s.contains("Region:"), "header missing:\n{s}");
        assert!(s.contains("╭"), "frame missing:\n{s}");
        assert!(s.contains("<help>"), "crumbs chip missing:\n{s}");
    }

    #[test]
    fn dashboard_scrolls_when_it_overflows() {
        let mut m = listed_model();
        m.width = 60; // one column → guaranteed overflow at height 16
        m.height = 16;
        m.detail = m.all[0].clone();
        m.mode = Mode::Detail;
        let total = m.detail_content_height();
        assert!(total > m.page_rows(), "fixture must overflow");
        m.overlay_scroll = 3;
        let s = render(&m, 60, 16);
        assert!(s.contains("(4–"), "scroll indicator missing:\n{s}");
    }
}
