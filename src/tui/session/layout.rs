//! Tiling math for the session view: pane rectangles per layout, and
//! resizing every pane's emulator + PTY to match.

use crate::tui::{Layout, Model};

/// One pane's outer box geometry within the session view.
pub(crate) struct PaneRect {
    pub outer_w: u16,
    pub outer_h: u16,
    pub x: u16,
    pub y: u16,
}

/// Converts an outer box size to the emulator content size, reserving 2
/// cells for the border and 1 line for the pane title.
fn inner_dims(outer_w: u16, outer_h: u16) -> (u16, u16) {
    (
        (outer_w as i32 - 2).max(1) as u16,
        (outer_h as i32 - 3).max(1) as u16,
    )
}

impl Model {
    /// A single pane fills the screen (zoomed, or the only pane) — rendered
    /// borderless so text selection/copy isn't broken.
    pub(crate) fn is_fullscreen(&self) -> bool {
        self.zoomed || self.panes.len() == 1
    }

    /// How many columns per row and how many rows the current layout uses.
    pub(super) fn grid_dims(&self) -> (usize, usize) {
        let n = self.panes.len();
        if n == 0 {
            return (1, 1);
        }
        match self.layout {
            Layout::Rows => (1, n),
            Layout::Grid => {
                let c = (n as f64).sqrt().ceil() as usize;
                (c, n.div_ceil(c))
            }
            Layout::Columns => (n, 1),
        }
    }

    /// Computes each pane's outer box (1 line reserved for the header),
    /// tiled per the current layout.
    pub(crate) fn pane_rects(&self) -> Vec<PaneRect> {
        let n = self.panes.len();
        let mut rects = Vec::with_capacity(n);
        let avail_h = (self.height as i32 - 1).max(4) as usize;
        let (ncols, nrows) = self.grid_dims();
        let base_h = avail_h / nrows;
        for i in 0..n {
            let row = i / ncols;
            let col = i % ncols;
            let row_start = row * ncols;
            let row_end = (row_start + ncols).min(n);
            let k = row_end - row_start; // panes in this row
            let base_w = self.width as usize / k.max(1);
            let mut w = base_w;
            if col == k - 1 {
                w += self.width as usize - base_w * k; // last column gets the remainder
            }
            let mut h = base_h;
            if row + 1 == nrows {
                h += avail_h - base_h * nrows; // last row gets the remainder
            }
            rects.push(PaneRect {
                outer_w: w.max(8) as u16,
                outer_h: h.max(4) as u16,
                x: (base_w * col) as u16,
                y: (1 + base_h * row) as u16,
            });
        }
        rects
    }

    /// Resizes every pane's emulator + PTY to the current layout.
    pub(crate) fn relayout_session(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        if self.is_fullscreen() {
            // a single visible pane: borderless full-width, reserving 1 line
            // for the header and 1 for the pane title.
            if let Some(p) = self.panes.get(self.focus) {
                p.resize(self.width.max(1), (self.height as i32 - 2).max(1) as u16);
            }
            return;
        }
        let rects = self.pane_rects();
        for (p, r) in self.panes.iter().zip(&rects) {
            let (cols, rows) = inner_dims(r.outer_w, r.outer_h);
            p.resize(cols, rows);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::live_pane;
    use super::*;
    use crate::tui::test_model;

    #[test]
    fn grid_and_rects_cover_screen() {
        let mut m = test_model();
        m.width = 100;
        m.height = 30;
        m.panes = vec![live_pane(), live_pane(), live_pane()];
        for (layout, dims) in [
            (Layout::Columns, (3, 1)),
            (Layout::Rows, (1, 3)),
            (Layout::Grid, (2, 2)),
        ] {
            m.layout = layout;
            assert_eq!(m.grid_dims(), dims, "{layout:?}");
            let rects = m.pane_rects();
            assert_eq!(rects.len(), 3);
            // widths of each row sum to the screen width
            let (ncols, _) = m.grid_dims();
            let mut row_w = std::collections::HashMap::<usize, u32>::new();
            for (i, r) in rects.iter().enumerate() {
                *row_w.entry(i / ncols).or_default() += r.outer_w as u32;
            }
            for (row, w) in &row_w {
                let full =
                    row_w.len() == 1 || *row < row_w.len() - 1 || rects.len().is_multiple_of(ncols);
                if full {
                    assert_eq!(*w, 100, "{layout:?} row {row} width");
                }
            }
        }
        for p in &m.panes {
            p.close();
        }
    }
}
