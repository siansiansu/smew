//! Rendering for the instance list: the k9s-style top panel (info block,
//! keymap menu, logo), the scrollable table, and the status bar.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::{StateClass, classify_state, pad1, refresh_label, state_color};
use crate::theme;
use crate::tui::{LIST_ROW_H, Model, SortKey, age_label};
use crate::version;

const MARK_ONLINE: &str = "Connected"; // reachable via SSM
const MARK_OFFLINE: &str = "-"; // not reachable
// k9s marks rows with color, not emoji — double-width glyphs misalign in
// some terminals.
const MARK_SELECTED: &str = "*"; // space-marked for multi-open
const MARK_NONE: &str = " ";

const NAME_COL: usize = 3; // index of the NAME column
const CPU_COL: usize = 7; // base index of %CPU
const MEM_COL: usize = 8; // base index of %MEM
const PCT_W: usize = 6; // %CPU/%MEM column width (fits "100" + sort arrow)

pub(crate) struct Column {
    pub title: String,
    pub width: usize,
}

const BASE_COLUMNS: [(&str, usize); 13] = [
    ("#", 5),   // row number (serial)
    ("", 3),    // multi-open mark (✅)
    ("SSM", 9), // fits "Connected"
    ("NAME", 34),
    ("INSTANCE-ID", 20),
    ("STATE", 14),
    ("TYPE", 11),
    ("%CPU", PCT_W),
    ("%MEM", PCT_W),
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
        // With metrics off the %CPU/%MEM columns disappear entirely
        // (k9s-style: no metrics source → no columns, not a dead n/a pair).
        let mut cols: Vec<Column> = BASE_COLUMNS
            .iter()
            .enumerate()
            .filter(|(i, _)| self.metrics_enabled || (*i != CPU_COL && *i != MEM_COL))
            .map(|(_, (t, w))| Column {
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
        // Signed on purpose: fill goes negative on narrow terminals, and the
        // comparison below must see that rather than a saturated zero.
        let fill = self.width as i64 - fixed as i64 - 2 * cols.len() as i64;
        if fill > name_w as i64 {
            name_w = fill as usize;
        }
        cols[NAME_COL].width = name_w;

        let base = match self.sort_by {
            SortKey::Name => 3,
            SortKey::State => 5,
            SortKey::Type => 6,
            SortKey::Cpu => CPU_COL,
            SortKey::Mem => MEM_COL,
            SortKey::Launch => 9,
            SortKey::Ip => 11,
        };
        // Positions past the hidden %CPU/%MEM shift left by two (sorting by
        // cpu/mem itself is unreachable with metrics off — keys are guarded).
        let idx = if self.metrics_enabled || base < CPU_COL {
            base
        } else {
            base - 2
        };
        let arrow = if self.sort_asc { " ↑" } else { " ↓" };
        cols[idx].title.push_str(arrow);
        cols
    }
}

/// Rows of the k9s-style top panel: info block · keymap menu · logo.
pub(crate) const HEADER_H: u16 = 7;

/// Keymap rows per menu column in the top panel.
const MENU_ROWS: usize = 6;

// The smew itself (a floating duck), not lettering.
const LOGO: [&str; 4] = [
    r"    __     ",
    r"  <(o )___ ",
    r"   ( ._> / ",
    r"~~~`---'~~~",
];
const LOGO_W: u16 = 11;

const LIST_MENU: [(&str, &str); 12] = [
    ("↑↓", "move"),
    (":", "command"),
    ("/", "filter"),
    ("space", "mark"),
    ("s", "connect"),
    ("enter", "details"),
    ("F", "port-forward"),
    ("c", "profile"),
    ("R", "reboot"),
    ("r", "refresh"),
    ("?", "help"),
    ("q", "quit"),
];

/// The reduced menu of the non-instance resource views.
const RES_MENU: [(&str, &str); 10] = [
    ("↑↓", "move"),
    (":", "command"),
    ("/", "filter"),
    ("enter", "drill / details"),
    ("d", "details"),
    ("c", "profile"),
    ("r", "refresh"),
    ("esc", "back to ec2"),
    ("?", "help"),
    ("q", "quit"),
];

/// The describe page's menu (instances also get `s`, harmless elsewhere).
const DETAIL_MENU: [(&str, &str); 5] = [
    ("↑↓", "scroll"),
    ("s", "connect"),
    ("esc / d", "back"),
    ("?", "help"),
    ("q", "quit"),
];

/// The help page's menu.
const HELP_MENU: [(&str, &str); 3] = [("↑↓", "scroll"), ("esc / ?", "back"), ("q", "quit")];

pub(super) fn draw_list(m: &Model, f: &mut Frame) {
    let area = f.area();
    if area.height < 5 || area.width < 10 {
        return;
    }

    draw_header(m, f, Rect::new(area.x, area.y, area.width, HEADER_H));

    // k9s-style prompt box: inserted between header and panel only while
    // active; the table reclaims the rows when it closes.
    let mut y = HEADER_H.min(area.height);
    if (m.filtering || m.commanding) && area.height > y + PROMPT_H {
        draw_prompt(m, f, Rect::new(area.x, area.y + y, area.width, PROMPT_H));
        y += PROMPT_H;
    }

    // The framed main panel fills everything down to the bottom bar.
    let panel_h = area.height.saturating_sub(y + 1);
    if panel_h >= 3 {
        draw_panel(m, f, Rect::new(area.x, area.y + y, area.width, panel_h));
    }

    let bar_y = area.y + area.height - 1;
    f.render_widget(
        Paragraph::new(pad1(bottom_bar(m))),
        Rect::new(area.x, bar_y, area.width, 1),
    );
}

/// Height of the command/filter prompt box (border + 1 content line).
pub(crate) const PROMPT_H: u16 = 3;

/// The command/filter prompt, k9s-style: one bordered one-line box serving
/// both modes, told apart by prefix char and border color (command = `> `
/// accent, filter = `/ ` pink). Appears only while typing; an applied
/// filter then lives in the panel title chip.
fn draw_prompt(m: &Model, f: &mut Frame, area: Rect) {
    let th = theme::current();
    let (prefix, color, input) = if m.commanding {
        (" > ", th.accent, &m.cmd)
    } else {
        (" / ", th.pink, &m.filter)
    };
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(color).add_modifier(Modifier::BOLD));
    let mut spans = vec![Span::styled(
        prefix,
        Style::new().fg(color).add_modifier(Modifier::BOLD),
    )];
    let (before, cur, after) = input.render_parts();
    if input.value().is_empty() {
        spans.push(Span::styled(
            cur,
            Style::new().add_modifier(Modifier::REVERSED),
        ));
        let hint = if m.commanding {
            "command (tab completes · ↑ recalls): ec2 vol sg vpc s3 lambda rds ddb sqs ecs … (? lists all) · profile [name] · help · quit"
        } else {
            "filter (substring · space = AND · ! = exclude): name / id / ip / type / az / vpc / tag"
        };
        spans.push(Span::styled(hint, Style::new().dim()));
    } else {
        spans.push(Span::raw(before.to_string()));
        spans.push(Span::styled(
            cur,
            Style::new().add_modifier(Modifier::REVERSED),
        ));
        spans.push(Span::raw(after.to_string()));
        // inline fish-style ghost: the remainder of the best suggestion
        if m.commanding && after.is_empty() {
            let sug = m.cmd_suggestion();
            if !sug.is_empty() {
                spans.push(Span::styled(sug, Style::new().fg(th.gray).dim()));
            }
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

/// The framed main panel: rounded bold border, k9s-style embedded title
/// (` </query> ec2(profile)[count] `), the table blitted into the interior.
fn draw_panel(m: &Model, f: &mut Frame, area: Rect) {
    let th = theme::current();
    let border = if m.mode == crate::tui::Mode::List {
        Style::new().fg(th.accent).add_modifier(Modifier::BOLD)
    } else {
        // an overlay (confirm / forward form) has focus
        Style::new().fg(th.border_unfocused)
    };

    let mut title: Vec<Span> = vec![Span::raw(" ")];
    if !m.filter.value().is_empty() && !m.filtering {
        title.push(Span::styled(
            format!("</{}> ", m.filter.value()),
            Style::new().fg(th.pink).add_modifier(Modifier::BOLD),
        ));
    }
    title.push(Span::styled(
        m.view.title(),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    title.push(Span::raw("("));
    title.push(Span::styled(
        if m.profile.is_empty() {
            "default".to_string()
        } else {
            m.profile.clone()
        },
        Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
    ));
    title.push(Span::raw(")["));
    title.push(Span::styled(
        format!("{}", m.row_count()),
        Style::new().fg(th.accent).add_modifier(Modifier::BOLD),
    ));
    title.push(Span::raw("] "));

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Line::from(title).centered());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if m.view == crate::resources::ResourceKind::Instances {
        draw_table(m, f.buffer_mut(), inner);
    } else {
        super::resource::draw_res_table(m, f.buffer_mut(), inner);
    }
}

/// The k9s-style top panel: a `Label: value` info block on the left, the
/// keymap menu in the middle, and the logo on the right. The menu and logo
/// drop out on narrow terminals; the info block always renders (clipped).
/// Shared by the list, describe and help pages (the menu follows the mode).
pub(super) fn draw_header(m: &Model, f: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let info = info_lines(m);
    let info_w = info.iter().map(Line::width).max().unwrap_or(0) as u16 + 1; // +1 for pad1
    let menu = match m.mode {
        crate::tui::Mode::Detail => menu_lines(&DETAIL_MENU),
        crate::tui::Mode::Help => menu_lines(&HELP_MENU),
        _ if m.view == crate::resources::ResourceKind::Instances => menu_lines(&LIST_MENU),
        _ => menu_lines(&RES_MENU),
    };
    let menu_w = menu.iter().map(Line::width).max().unwrap_or(0) as u16;

    let info_lines: Vec<Line> = info.into_iter().map(pad1).collect();
    f.render_widget(
        Paragraph::new(info_lines),
        Rect::new(area.x, area.y, info_w.min(area.width), area.height),
    );

    let logo_fits = area.width >= info_w + 2 + menu_w + 3 + LOGO_W;
    if logo_fits {
        let th = theme::current();
        let lines: Vec<Line> = LOGO
            .iter()
            .map(|s| Line::from(Span::styled(*s, Style::new().fg(th.orange))))
            .collect();
        f.render_widget(
            Paragraph::new(lines),
            Rect::new(
                area.x + area.width - LOGO_W - 1,
                area.y + 1,
                LOGO_W,
                (LOGO.len() as u16).min(area.height.saturating_sub(1)),
            ),
        );
    }

    if area.width > info_w + 4 {
        let mx = area.x + info_w + 2;
        let mw = if logo_fits {
            area.width - info_w - 2 - LOGO_W - 2
        } else {
            area.width - info_w - 2
        };
        f.render_widget(Paragraph::new(menu), Rect::new(mx, area.y, mw, area.height));
    }
}

/// The `Label: value` rows of the top panel's info block (always HEADER_H
/// rows so the block lines up with the menu and logo).
fn info_lines(m: &Model) -> Vec<Line<'static>> {
    let th = theme::current();
    let label = |s: &str| Span::styled(format!("{s:<10} "), Style::new().fg(th.gray));
    let (total, running, stopped, other) = counts(m);

    let mut inst_spans = vec![
        label("Instances:"),
        Span::styled(
            format!("{total}"),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ("),
        Span::styled(format!("{running} running"), Style::new().fg(th.green)),
        Span::styled(" · ", Style::new().fg(th.gray)),
        Span::styled(format!("{stopped} stopped"), Style::new().fg(th.red)),
    ];
    if other > 0 {
        inst_spans.push(Span::styled(" · ", Style::new().fg(th.gray)));
        inst_spans.push(Span::styled(
            format!("{other} other"),
            Style::new().fg(th.orange),
        ));
    }
    inst_spans.push(Span::raw(")"));

    let mut version_spans = vec![
        label("Version:"),
        Span::styled(version::VERSION.to_string(), Style::new().fg(th.accent)),
    ];
    if m.update_available {
        version_spans.push(Span::raw(" "));
        version_spans.push(Span::styled(
            format!(" ⬆ {} ", m.latest_version),
            Style::new()
                .bg(th.orange)
                .fg(th.notice_fg)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Identity rows: placeholders until sts:GetCallerIdentity resolves.
    let (account, user) = match &m.identity {
        Some(id) => (id.account.clone(), id.user_label()),
        None => ("…".to_string(), "…".to_string()),
    };

    vec![
        Line::from(vec![
            label("Profile:"),
            Span::styled(
                if m.profile.is_empty() {
                    "default".to_string()
                } else {
                    m.profile.clone()
                },
                Style::new().fg(th.cyan),
            ),
        ]),
        Line::from(vec![
            label("Account:"),
            Span::styled(account, Style::new().fg(th.accent)),
        ]),
        Line::from(vec![
            label("User:"),
            Span::styled(user, Style::new().fg(th.accent)),
        ]),
        Line::from(vec![
            label("Region:"),
            Span::styled(
                if m.region.is_empty() {
                    "(default)".to_string()
                } else {
                    m.region.clone()
                },
                Style::new().fg(th.cyan),
            ),
        ]),
        Line::from(inst_spans),
        Line::from(vec![
            label("Synced:"),
            Span::styled(
                m.last_sync
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "—".into()),
                Style::new().fg(th.green),
            ),
            Span::styled(" · ", Style::new().fg(th.gray)),
            Span::styled(auto_label(m), Style::new().fg(th.orange)),
        ]),
        Line::from(version_spans),
    ]
}

/// Lays hints out k9s-style: column-major, MENU_ROWS per column, each
/// column's keys padded to a shared width so descriptions align.
fn menu_lines(hints: &[(&'static str, &'static str)]) -> Vec<Line<'static>> {
    let th = theme::current();
    let key_style = Style::new().fg(th.orange).add_modifier(Modifier::BOLD);
    let desc_style = Style::new().fg(th.gray);
    let mut lines: Vec<Vec<Span>> = vec![Vec::new(); MENU_ROWS];
    for col in hints.chunks(MENU_ROWS) {
        let key_w = col
            .iter()
            .map(|(k, _)| k.chars().count() + 2)
            .max()
            .unwrap_or(0);
        let desc_w = col
            .iter()
            .map(|(_, d)| d.chars().count())
            .max()
            .unwrap_or(0);
        for (r, spans) in lines.iter_mut().enumerate() {
            if let Some((k, d)) = col.get(r) {
                spans.push(Span::styled(
                    format!("{:<key_w$}", format!("<{k}>")),
                    key_style,
                ));
                spans.push(Span::styled(format!(" {d:<desc_w$}"), desc_style));
            } else {
                spans.push(Span::raw(" ".repeat(key_w + 1 + desc_w)));
            }
            spans.push(Span::raw("   "));
        }
    }
    lines.into_iter().map(Line::from).collect()
}

/// Tallies instances by lifecycle state for the info block.
fn counts(m: &Model) -> (usize, usize, usize, usize) {
    let [running, stopped, other] = m.all.iter().fold([0usize; 3], |mut acc, inst| {
        let slot = match classify_state(&inst.state) {
            StateClass::Running => 0,
            StateClass::Down => 1,
            StateClass::Other => 2,
        };
        acc[slot] += 1;
        acc
    });
    (m.all.len(), running, stopped, other)
}

/// The bottom bar, k9s crumbs-style: the view chip on the left, then
/// transient mode indicators (adding pane, horizontal scroll, goto) and the
/// last status/flash message.
fn bottom_bar(m: &Model) -> Line<'static> {
    let th = theme::current();
    let label = Style::new().fg(th.gray);
    let mut spans = vec![Span::styled(
        format!(" <{}> ", m.view.title()),
        Style::new()
            .fg(th.title_fg)
            .bg(th.title_bg)
            .add_modifier(Modifier::BOLD),
    )];
    let sep = Span::raw("  ");
    if m.filtering {
        spans.push(sep);
        spans.push(Span::styled(
            "enter = apply · esc = clear",
            Style::new().dim(),
        ));
        return Line::from(spans);
    }
    if m.adding_pane {
        spans.push(sep.clone());
        spans.push(Span::styled(
            "adding pane — s to add · esc to cancel",
            Style::new().fg(th.orange),
        ));
    }
    if m.h_offset > 0 {
        spans.push(sep.clone());
        spans.push(Span::styled(format!("(scrolled →{})", m.h_offset), label));
    }
    if !m.count_buf.is_empty() {
        spans.push(sep.clone());
        spans.push(Span::styled(
            format!("goto:{}", m.count_buf),
            Style::new().fg(th.orange),
        ));
    }
    // Status goes last: on overflow it truncates first — the chip and the
    // mode indicators are never sacrificed to a long message.
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

/// k9s-style percent cell: a bare floored integer (no % sign), right-aligned
/// under the header; "n/a" when the metric is unavailable.
fn pct_label(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{:>4}", v.clamp(0.0, 100.0).floor() as i64),
        None => format!("{:>4}", "n/a"),
    }
}

/// k9s threshold colors (warn 70 / critical 90): hot cells go orange/red
/// bold; below the warn level the cell inherits the row style.
fn pct_style(v: Option<f64>, base: Style) -> Style {
    let th = theme::current();
    match v {
        Some(v) if v >= 90.0 => base.fg(th.red).add_modifier(Modifier::BOLD),
        Some(v) if v >= 70.0 => base.fg(th.orange).add_modifier(Modifier::BOLD),
        _ => base,
    }
}

/// Renders the instance table into a full-content-width buffer and blits the
/// horizontally-scrolled window into the frame.
fn draw_table(m: &Model, dst: &mut Buffer, area: Rect) {
    if area.height < 3 {
        return;
    }
    let cols = m.columns();
    let row_h = LIST_ROW_H as u16;
    // content_width() is capped internally, so the buffer, the scroll bound
    // (max_h_offset) and this blit all agree on the same width.
    let content_w = m.content_width() as u16;
    let data_rows = ((area.height - 2) / row_h).min(m.visible_data_rows() as u16);
    let mut wide = Buffer::empty(Rect::new(0, 0, content_w, 2 + data_rows * row_h));

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
        // Each row slot is LIST_ROW_H lines: blank spacer(s) above, content on
        // the last line — so the header rule gets breathing room too.
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
        let u = m.util.get(&inst.instance_id).copied().unwrap_or_default();
        // Cells carry their BASE_COLUMNS index so they stay paired with the
        // visible columns whether or not %CPU/%MEM are shown.
        let mut cells: Vec<(usize, String)> = vec![
            (0, format!("{}", idx + 1)),
            (
                1,
                if m.marked.contains(&inst.instance_id) {
                    MARK_SELECTED
                } else {
                    MARK_NONE
                }
                .into(),
            ),
            (
                2,
                if inst.is_connectable() {
                    MARK_ONLINE
                } else {
                    MARK_OFFLINE
                }
                .into(),
            ),
            (3, inst.name.clone()),
            (4, inst.instance_id.clone()),
            (5, inst.state.clone()),
            (6, inst.instance_type.clone()),
        ];
        if m.metrics_enabled {
            cells.push((CPU_COL, pct_label(u.cpu)));
            cells.push((MEM_COL, pct_label(u.mem)));
        }
        cells.extend([
            (9, age_label(inst.launch_time)),
            (10, inst.az.clone()),
            (11, inst.private_ip.clone()),
            (12, inst.vpc_id.clone()),
        ]);
        let mut x = 0u16;
        for ((ci, cell), c) in cells.iter().zip(cols.iter()) {
            // SSM/STATE keep their status color on unselected rows; %CPU/%MEM
            // highlight k9s-style when hot even on the selected row.
            let cell_style = match *ci {
                CPU_COL => pct_style(u.cpu, row_style),
                MEM_COL => pct_style(u.mem, row_style),
                // the multi-open mark keeps its color on the selected row
                1 => row_style.fg(th.cyan).add_modifier(Modifier::BOLD),
                _ if selected => row_style,
                2 if inst.is_connectable() => Style::new().fg(th.green),
                2 => Style::new().fg(th.gray),
                5 => Style::new().fg(state_color(&inst.state)),
                _ => row_style,
            };
            wide.set_stringn(x + 1, y, cell, c.width, cell_style);
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
        assert!(s.contains("NAME ↑"), "sorted NAME header missing:\n{s}");
        assert!(s.contains("INSTANCE-ID"), "header missing:\n{s}");
        assert!(s.contains("web-prod-01"), "row missing:\n{s}");
        assert!(s.contains("i-0bbb2222"), "row missing:\n{s}");
        // framed panel: k9s-style title + rounded border + crumbs chip
        assert!(s.contains("ec2(default)["), "panel title missing:\n{s}");
        assert!(s.contains("╭"), "panel border missing:\n{s}");
        assert!(s.contains("<ec2>"), "crumbs chip missing:\n{s}");
        // top panel: info block, keymap menu, logo
        assert!(s.contains("Region:"), "info label missing:\n{s}");
        assert!(s.contains("ap-northeast-1"), "region value missing:\n{s}");
        assert!(s.contains("1 running"), "state counts missing:\n{s}");
        assert!(s.contains("1 stopped"), "state counts missing:\n{s}");
        assert!(s.contains("<s>"), "keymap menu missing:\n{s}");
        assert!(s.contains("connect"), "keymap menu missing:\n{s}");
        assert!(s.contains("port-forward"), "keymap menu missing:\n{s}");
        assert!(s.contains("<(o )"), "bird logo missing:\n{s}");
    }

    #[test]
    fn renders_identity_rows() {
        let mut m = listed_model();
        // placeholder before STS resolves
        let s = render(&m, 120, 30);
        assert!(s.contains("Account:"), "account label missing:\n{s}");
        m.identity = Some(crate::inventory::CallerIdentity {
            account: "111122223333".to_string(),
            arn: "arn:aws:sts::111122223333:assumed-role/Admin/alex".to_string(),
        });
        let s = render(&m, 120, 30);
        assert!(s.contains("111122223333"), "account id missing:\n{s}");
        assert!(s.contains("Admin/alex"), "user label missing:\n{s}");
    }

    #[test]
    fn narrow_terminal_drops_logo_keeps_info() {
        let m = listed_model();
        let s = render(&m, 60, 30);
        assert!(!s.contains("<(o )"), "logo must drop when narrow:\n{s}");
        assert!(s.contains("Region:"), "info block must survive:\n{s}");
    }

    #[test]
    fn renders_utilization_cells() {
        let mut m = listed_model();
        m.util.insert(
            "i-0aaa1111".to_string(),
            crate::inventory::Utilization {
                cpu: Some(97.4),
                mem: Some(42.0),
            },
        );
        let s = render(&m, 120, 30);
        assert!(s.contains("%CPU"), "%CPU header missing:\n{s}");
        assert!(s.contains("%MEM"), "%MEM header missing:\n{s}");
        assert!(s.contains("97"), "cpu value missing (floored):\n{s}");
        assert!(s.contains("42"), "mem value missing:\n{s}");
        // the host without metrics shows the placeholder
        assert!(s.contains("n/a"), "n/a placeholder missing:\n{s}");
    }

    #[test]
    fn metrics_off_hides_columns() {
        let mut m = listed_model();
        m.metrics_enabled = false;
        let s = render(&m, 120, 30);
        assert!(!s.contains("%CPU"), "%CPU must be hidden:\n{s}");
        assert!(!s.contains("n/a"), "no placeholder cells either:\n{s}");
        assert!(s.contains("NAME ↑"), "sort arrow must survive:\n{s}");
        assert!(s.contains("AGE"), "later columns must survive:\n{s}");
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
    fn renders_filter_chip_and_marks() {
        let mut m = listed_model();
        m.filter.insert_str("prod");
        m.marked.insert("i-0aaa1111".to_string());
        m.apply_filter();
        let s = render(&m, 120, 30);
        // applied filter lives in the panel title chip with the count
        assert!(s.contains("</prod>"), "filter chip missing:\n{s}");
        assert!(
            s.contains("ec2(default)[1]"),
            "filtered count missing:\n{s}"
        );
        assert!(s.contains('*'), "mark missing:\n{s}");
    }

    #[test]
    fn renders_prompt_box_while_filtering() {
        let mut m = listed_model();
        m.filtering = true;
        m.filter.insert_str("web");
        m.apply_filter();
        let s = render(&m, 120, 30);
        assert!(s.contains("/ web"), "prompt content missing:\n{s}");
        // while typing the title shows no chip yet
        assert!(!s.contains("</web>"), "chip must wait for apply:\n{s}");
    }
}
