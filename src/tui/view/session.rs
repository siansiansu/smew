//! Rendering for the session view: one borderless full-screen pane (so
//! text selection / copy isn't broken by frame lines), a badge/help header,
//! and the emulator screen contents with a cursor overlay.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::session::Pane;
use crate::theme;
use crate::tui::{Model, leader_label};

pub(super) fn draw_session(m: &Model, f: &mut Frame) {
    let Some(p) = &m.pane else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" no session", Style::new().dim()))),
            f.area(),
        );
        return;
    };
    let area = f.area();
    f.render_widget(
        Paragraph::new(session_header(m)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height < 3 || area.width < 4 {
        return;
    }

    let title = pane_title(m, p);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title,
            Style::new().add_modifier(Modifier::BOLD),
        ))),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );

    let content = Rect::new(
        area.x,
        area.y + 2,
        area.width,
        area.height.saturating_sub(2),
    );
    let off = if m.scrolling { m.scroll_offset } else { 0 };
    render_screen(p, off, f.buffer_mut(), content);
    if !p.is_done() && !m.scrolling {
        overlay_cursor(p, f.buffer_mut(), content);
    }

    if m.leader_pending {
        draw_leader_menu(m, f);
    }
}

/// The which-key popup: while the leader prefix is pending, an anchored
/// panel lists every command so they're discoverable without memorizing
/// the cheat sheet. Any non-command key still just cancels.
fn draw_leader_menu(m: &Model, f: &mut Frame) {
    let lead = leader_label(&m.leader);
    let entries: Vec<(String, String)> = vec![
        ("[".into(), "scrollback".into()),
        ("x / d".into(), "end session".into()),
        ("?".into(), "help".into()),
        (lead.clone(), format!("send literal {lead}")),
        ("esc".into(), "cancel".into()),
    ];
    let kw = entries
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let inner_w = entries
        .iter()
        .map(|(_, v)| kw + 2 + v.chars().count())
        .max()
        .unwrap_or(0)
        .max(lead.chars().count() + 4);
    let area = f.area();
    let w = ((inner_w + 4) as u16).min(area.width);
    let h = ((entries.len() + 2) as u16).min(area.height.saturating_sub(1));
    // bottom-right, clear of the pane's top-left content
    let rect = Rect::new(
        area.x + area.width.saturating_sub(w + 1),
        area.y + area.height.saturating_sub(h + 1),
        w,
        h,
    );
    let th = theme::current();
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(th.gray))
        .title(format!(" {lead} — command "));
    let lines: Vec<Line> = entries
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(
                    format!(" {k:<kw$}  "),
                    Style::new().fg(th.orange).add_modifier(Modifier::BOLD),
                ),
                Span::raw(v.clone()),
            ])
        })
        .collect();
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

/// The pane's title line: name plus [exited] / [SCROLL ↑n] markers.
fn pane_title(m: &Model, p: &Pane) -> String {
    let mut title = format!("▶ {}", p.title);
    if p.is_done() {
        title.push_str(" [exited]");
    }
    if m.scrolling {
        title.push_str(&format!(" [SCROLL ↑{}]", m.scroll_offset));
    }
    title
}

/// Copies the emulator screen (optionally scrolled into history) into the
/// buffer region, mapping vt100 colors/attributes to ratatui styles.
fn render_screen(p: &Pane, off: usize, buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    p.with_screen(off, |screen| {
        let (rows, cols) = screen.size();
        for y in 0..rows.min(area.height) {
            for x in 0..cols.min(area.width) {
                let Some(cell) = screen.cell(y, x) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let dst = &mut buf[(area.x + x, area.y + y)];
                let contents = cell.contents();
                if contents.is_empty() {
                    dst.set_symbol(" ");
                } else {
                    dst.set_symbol(contents);
                }
                let mut style = Style::new()
                    .fg(to_ratatui_color(cell.fgcolor(), Color::Reset))
                    .bg(to_ratatui_color(cell.bgcolor(), Color::Reset));
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.dim() {
                    style = style.add_modifier(Modifier::DIM);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                dst.set_style(style);
            }
        }
    });
}

fn to_ratatui_color(c: vt100::Color, default: Color) -> Color {
    match c {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Draws a reverse-video block at the pane cursor so the pane shows where
/// input goes. At a line end (e.g. vim `$`) the emulator can report the
/// cursor at the phantom column x == w — clamp so the cursor stays visible
/// on the last cell.
fn overlay_cursor(p: &Pane, buf: &mut Buffer, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (cx, cy) = p.cursor_pos();
    let x = cx.min(area.width - 1);
    if cy >= area.height {
        return;
    }
    let cell = &mut buf[(area.x + x, area.y + cy)];
    let style = cell.style().add_modifier(Modifier::REVERSED);
    cell.set_style(style);
}

/// The badge + help header line above the pane.
fn session_header(m: &Model) -> Line<'static> {
    let th = theme::current();
    let badge = |text: String, bg: Color| {
        Span::styled(
            format!(" {text} "),
            Style::new()
                .bg(bg)
                .fg(th.badge_fg)
                .add_modifier(Modifier::BOLD),
        )
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    if m.scrolling {
        spans.push(badge("SCROLL".into(), th.border_scroll));
        spans.push(Span::raw(" "));
    }
    if m.leader_pending {
        spans.push(badge("PREFIX".into(), th.sel_bg));
        spans.push(Span::raw(" "));
    }

    let lead = leader_label(&m.leader);
    let help = if m.scrolling {
        let sb = m.pane.as_ref().map(|p| p.scrollback_len()).unwrap_or(0);
        if sb == 0 {
            // Full-screen apps (less, vim…) run on the alternate screen, which
            // has no scrollback — their content is scrolled inside the app.
            "SCROLL: no history for this pane — full-screen apps (less, vim…) scroll inside the app · esc/q exit".to_string()
        } else {
            format!("SCROLL ({sb} lines): ↑/↓ line · PgUp/PgDn page · g/G top/bottom · esc/q exit")
        }
    } else {
        format!("{lead} then: [ scroll · x/d end session · ? help")
    };
    spans.push(Span::styled(help, Style::new().dim()));

    if !m.status.is_empty() && !m.scrolling {
        spans.push(Span::styled(
            format!(" ⚠ {} ", m.status),
            Style::new()
                .fg(th.notice_fg)
                .bg(th.orange)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}
