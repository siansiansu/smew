//! The terminal frontend: a filterable, sortable instance table
//! with a profile picker, detail view, and a multi-pane session view.
//!
//! Architecture: a single Msg channel drained by the UI thread (`app::run`).
//! Input, ticks, AWS calls and pane output all land here as messages;
//! rendering happens after each batch.

mod app;
mod input;
mod keymap;
mod session;
mod view;

pub use app::{BuildFn, Options, run};

pub(crate) use app::{
    ConfirmKind, FwdField, LIST_ROW_H, Mode, Model, SortKey, age_label, leader_label,
};

#[cfg(test)]
pub(crate) use app::{CmdResult, max_name_width, test_model};
