//! Establishes connections to SSM targets.
//!
//! Session panes run `smew ssm-session …` (see ssm.rs): the SDK makes the
//! ssm:StartSession call, then the process execs session-manager-plugin —
//! the one external binary smew needs. The driver builds those argvs; the
//! design leaves room for a native SSM WebSocket driver later, without
//! touching the frontend.

mod driver;
mod pane;
pub mod ssm;

pub use driver::{PluginDriver, SshOptions};

pub(crate) use pane::{Notifier, Pane};
