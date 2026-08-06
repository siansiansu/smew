//! Rendering for the multi-pane session view: tiled bordered panes (or one
//! borderless full-screen pane), the badge/help header, and the emulator
//! screen contents with a cursor overlay.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::{GRAY, ORANGE, PINK, RED};
use crate::session::Pane;
use crate::tui::session::pane_key;
use crate::tui::{Model, leader_label};

pub(super) fn draw_session(m: &Model, f: &mut Frame) {
    if m.panes.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" no panes", Style::new().dim()))),
            f.area(),
        );
        return;
    }
    let area = f.area();
    f.render_widget(
        Paragraph::new(session_header(m)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height < 3 || area.width < 4 {
        return;
    }

    if m.is_fullscreen() {
        draw_zoom(m, f);
    } else {
        let rects = m.pane_rects();
        for (i, r) in rects.iter().enumerate() {
            let rect = Rect::new(r.x, r.y, r.outer_w, r.outer_h).intersection(area);
            if rect.width < 3 || rect.height < 3 {
                continue;
            }
            draw_pane_box(m, i, rect, f.buffer_mut());
        }
    }
    if m.leader_pending {
        draw_leader_menu(m, f);
    }
}

/// The which-key popup: while the leader prefix is pending, an anchored
/// panel lists every command so the multiplexer is discoverable without
/// memorizing the cheat sheet. Any non-command key still just cancels.
fn draw_leader_menu(m: &Model, f: &mut Frame) {
    let lead = leader_label(&m.leader);
    let entries: Vec<(String, String)> = vec![
        ("h/l · ↑/↓".into(), "focus pane (grid: directional)".into()),
        ("space".into(), "± broadcast group".into()),
        ("b".into(), "group all / none".into()),
        ("v".into(), format!("layout ({})", m.layout.name())),
        ("z".into(), "zoom pane".into()),
        ("[".into(), "scrollback".into()),
        ("a".into(), "add pane".into()),
        ("x".into(), "close pane".into()),
        ("d".into(), "end session".into()),
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
    // bottom-right, clear of the focused pane's top-left content
    let rect = Rect::new(
        area.x + area.width.saturating_sub(w + 1),
        area.y + area.height.saturating_sub(h + 1),
        w,
        h,
    );
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(GRAY))
        .title(format!(" {lead} — command "));
    let lines: Vec<Line> = entries
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(
                    format!(" {k:<kw$}  "),
                    Style::new().fg(ORANGE).add_modifier(Modifier::BOLD),
                ),
                Span::raw(v.clone()),
            ])
        })
        .collect();
    f.render_widget(Clear, rect);
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

/// The focused pane full-screen with NO border, so text selection / copy
/// isn't broken by side frame lines. One line each for the header and the
/// pane title.
fn draw_zoom(m: &Model, f: &mut Frame) {
    let area = f.area();
    let p = &m.panes[m.focus.min(m.panes.len() - 1)];
    let title = pane_title(m, p, "▶ ", m.scrolling);
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
}

/// One pane (bordered, tiled).
fn draw_pane_box(m: &Model, i: usize, rect: Rect, buf: &mut Buffer) {
    let p = &m.panes[i];
    let focused = i == m.focus;
    let scrolling_here = focused && m.scrolling;
    let in_group = m.broadcast_group.contains(&pane_key(p));
    let active = m.broadcasting();
    let receiving = (active && in_group) || (!active && focused); // gets input right now

    let bc = if scrolling_here {
        Color::Indexed(220) // scroll mode: yellow
    } else if active && in_group {
        RED // broadcasting to this pane
    } else if in_group {
        ORANGE // selected into a pending group
    } else if focused {
        PINK
    } else {
        Color::Indexed(240)
    };

    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(bc));
    let inner = block.inner(rect);
    ratatui::widgets::Widget::render(block, rect, buf);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let marker = if focused { "▶ " } else { "  " };
    // Broadcast-group marker: 🔊 = in group, 🔇 = not. Shown while a group is
    // forming (any member) or while selecting via focus-nav.
    let group = if m.broadcast_count() > 0 || m.focus_nav {
        if in_group { "🔊 " } else { "🔇 " }
    } else {
        ""
    };
    let prefix = format!("{marker}{group}");
    let title = pane_title(m, p, &prefix, scrolling_here);
    let tstyle = if receiving {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    buf.set_stringn(inner.x, inner.y, &title, inner.width as usize, tstyle);

    let content = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    let off = if scrolling_here { m.scroll_offset } else { 0 };
    render_screen(p, off, buf, content);
    // Draw the cursor in panes receiving input, but not while scrolling.
    if receiving && !p.is_done() && !scrolling_here {
        overlay_cursor(p, buf, content);
    }
}

/// A pane's title line: prefix + name, plus [exited] / [SCROLL ↑n] markers.
fn pane_title(m: &Model, p: &Pane, prefix: &str, scrolling_here: bool) -> String {
    let mut title = format!("{prefix}{}", p.title);
    if p.is_done() {
        title.push_str(" [exited]");
    }
    if scrolling_here {
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

/// Draws a reverse-video block at the pane cursor so the focused pane shows
/// where input goes. At a line end (e.g. vim `$`) the emulator can report
/// the cursor at the phantom column x == w — clamp so the cursor stays
/// visible on the last cell.
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

/// The badge + help header line above the panes.
fn session_header(m: &Model) -> Line<'static> {
    let badge = |text: String, bg: u8| {
        Span::styled(
            format!(" {text} "),
            Style::new()
                .bg(Color::Indexed(bg))
                .fg(Color::Indexed(15))
                .add_modifier(Modifier::BOLD),
        )
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let n = m.broadcast_count();
    if m.broadcasting() {
        spans.push(badge(format!("🔊 BROADCAST {n}/{}", m.panes.len()), 203));
        spans.push(Span::raw(" "));
    } else if n == 1 {
        // one selected — need one more to auto-broadcast
        spans.push(badge(
            format!(
                "GROUP 1/{} — select 1 more (space) to broadcast",
                m.panes.len()
            ),
            62,
        ));
        spans.push(Span::raw(" "));
    }
    if m.zoomed {
        spans.push(badge("ZOOM".into(), 36));
        spans.push(Span::raw(" "));
    }
    if m.scrolling {
        spans.push(badge("SCROLL".into(), 220));
        spans.push(Span::raw(" "));
    }
    if m.leader_pending {
        spans.push(badge("PREFIX".into(), 57));
        spans.push(Span::raw(" "));
    }
    if m.focus_nav {
        spans.push(badge("←→↑↓ FOCUS · space ±bcast".into(), 62));
        spans.push(Span::raw(" "));
    }

    let lead = leader_label(&m.leader);
    let help = if m.scrolling {
        let sb = m
            .panes
            .get(m.focus)
            .map(|p| p.scrollback_len())
            .unwrap_or(0);
        if sb == 0 {
            // Full-screen apps (less, vim…) run on the alternate screen, which
            // has no scrollback — their content is scrolled inside the app.
            "SCROLL: no history for this pane — full-screen apps (less, vim…) scroll inside the app · esc/q exit".to_string()
        } else {
            format!(
                "SCROLL ({sb} lines): ↑/↓ line · PgUp/PgDn page · g/G top/bottom · esc/q exit · {lead} z/h/l still work"
            )
        }
    } else {
        format!(
            "{} pane(s) · {lead} then: h/l/↑/↓ focus, space ±group (≥2 broadcasts) · b all/none · v layout({}) · z zoom · [ scroll · a add · x close · d end",
            m.panes.len(),
            m.layout.name()
        )
    };
    spans.push(Span::styled(help, Style::new().dim()));

    if !m.status.is_empty() && !m.scrolling {
        // Render notices (e.g. "disable broadcast before zoom") prominently.
        spans.push(Span::styled(
            format!(" ⚠ {} ", m.status),
            Style::new()
                .fg(Color::Indexed(0))
                .bg(ORANGE)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}
