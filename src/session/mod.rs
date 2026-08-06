//! Establishes connections to SSM targets.
//!
//! The interactive shell is driven by shelling out to the aws CLI (which in
//! turn invokes session-manager-plugin). The driver design leaves room for
//! port-forward / SSH-over-SSM and a native SSM WebSocket driver later,
//! without touching the frontend.

mod driver;
mod pane;

pub use driver::{PluginDriver, SshOptions};
pub use pane::{Notifier, Pane};
