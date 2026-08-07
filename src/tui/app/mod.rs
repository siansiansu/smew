//! The root model and the message loop: mode dispatch, async command
//! spawns, and shared small helpers.

mod list;
mod mouse;
mod overlays;
mod run;

pub use run::run;

pub(crate) use list::max_name_width;

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crossterm::event::{KeyEvent, MouseEvent};

use crate::inventory::{Instance, Inventory, ListResult};
use crate::session::Pane;
use crate::session::PluginDriver;
use crate::version;

use super::input::Input;
use super::keymap;

/// Messages driving the UI loop.
pub enum Msg {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    Loaded(ListResult),
    Version(String),
    ActionDone {
        verb: String,
        name: String,
        err: Option<String>,
    },
    DelayedRefresh,
    Tick,
    PaneOutput,
}

/// Constructs the inventory client and session driver for a profile,
/// returning the resolved AWS region. An empty profile means "use the
/// default credential chain".
pub type BuildFn = Box<dyn Fn(&str) -> Result<(Inventory, PluginDriver, String), String>>;

/// The current screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    List,
    Detail,
    Profiles,
    Help,
    Confirm,
    Session,
}

/// What the confirmation dialog is asking about.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConfirmKind {
    Reboot,
    CloseSession,
}

/// How session panes are tiled.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    Columns, // side-by-side (horizontal split)
    Rows,    // stacked top-to-bottom (vertical split, full-width)
    Grid,    // roughly-square grid
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Columns => "columns",
            Layout::Rows => "rows",
            Layout::Grid => "grid",
        }
    }
    pub(super) fn next(self) -> Layout {
        match self {
            Layout::Columns => Layout::Rows,
            Layout::Rows => Layout::Grid,
            Layout::Grid => Layout::Columns,
        }
    }
}

/// The active sort column.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortKey {
    Name,
    State,
    Type,
    Launch,
    Ip,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::State => "state",
            SortKey::Type => "type",
            SortKey::Launch => "age",
            SortKey::Ip => "ip",
        }
    }
}

pub struct Options {
    pub build: BuildFn,
    pub profiles: Vec<String>,
    pub initial_profile: String,
    pub refresh: Duration,
    pub leader: String,
    pub version_param: String,
    pub mouse: bool,
    pub rt: tokio::runtime::Handle,
}

/// The root model.
pub struct Model {
    rt: tokio::runtime::Handle,
    tx: Sender<Msg>,
    build: BuildFn,

    pub(crate) inventory: Option<Inventory>,
    pub(crate) driver: Option<PluginDriver>,
    pub(crate) profile: String,
    pub(crate) region: String,
    pub(crate) last_sync: Option<chrono::DateTime<chrono::Local>>,
    pub(crate) all: Vec<Instance>,
    pub(crate) filtered: Vec<Instance>,

    // instance table state
    pub(crate) cursor: usize,
    pub(crate) row_offset: usize,

    pub(crate) filter: Input,
    pub(crate) filtering: bool,
    pub(crate) filter_stack: Vec<String>,

    // profile picker
    pub(crate) profiles: Vec<String>,
    pub(crate) picker_cursor: usize,
    pub(crate) picker_input: Input,
    pub(crate) picker_typing: bool,
    pub(crate) picker_query: String, // committed filter

    pub(crate) mode: Mode,
    pub(crate) overlay_scroll: usize, // detail/help vertical scroll offset
    pub(crate) detail: Instance,
    pub(crate) confirm: Instance,
    pub(crate) confirm_action: ConfirmKind,
    pub(crate) refresh: Duration,

    // update check
    pub(crate) version_param: String,
    pub(crate) latest_version: String,
    pub(crate) update_available: bool,

    // multi-pane session state
    pub(crate) marked: HashSet<String>,
    pub(crate) panes: Vec<Arc<Pane>>,
    pub(crate) focus: usize,
    pub(crate) broadcast_group: HashSet<usize>, // Arc pointer identity of group members
    pub(crate) pane_dirty: Arc<AtomicBool>,
    pub(crate) leader: String,
    pub(crate) leader_pending: bool,
    pub(crate) focus_nav: bool,
    pub(crate) adding_pane: bool,
    pub(crate) zoomed: bool,
    pub(crate) scrolling: bool,
    pub(crate) scroll_offset: usize,
    pub(crate) layout: Layout,

    pub(crate) sort_by: SortKey,
    pub(crate) sort_asc: bool,

    pub(crate) count_buf: String, // vim-style numeric prefix (e.g. "10" then gg)
    pub(crate) g_pending: bool,   // first 'g' of a gg motion was pressed

    pub(crate) name_width: usize, // NAME column width = longest name
    pub(crate) h_offset: usize,   // horizontal scroll offset in cells

    pub(crate) status: String,
    pub(crate) loading: bool,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) mouse: bool,
    // last left-click (for double-click detection): when, screen, row index
    pub(crate) last_click: Option<(std::time::Instant, Mode, usize)>,
    quit: bool,
}

impl Model {
    /// Constructs the model. When initial_profile is empty and profiles are
    /// available, it opens on the profile picker; otherwise it loads
    /// immediately.
    pub fn new(opts: Options, tx: Sender<Msg>) -> Model {
        // Terminals report ctrl+space as NUL, traditionally spelled "ctrl+@";
        // we name it "ctrl+ " (matching config.example.yaml). Accept both
        // spellings. (Config::leader() already guarantees a non-empty default.)
        let leader = if opts.leader == "ctrl+@" {
            "ctrl+ ".to_string()
        } else {
            opts.leader
        };
        let mut m = Model {
            rt: opts.rt,
            tx,
            build: opts.build,
            inventory: None,
            driver: None,
            profile: String::new(),
            region: String::new(),
            last_sync: None,
            all: Vec::new(),
            filtered: Vec::new(),
            cursor: 0,
            row_offset: 0,
            filter: Input::default(),
            filtering: false,
            filter_stack: Vec::new(),
            profiles: opts.profiles,
            picker_cursor: 0,
            picker_input: Input::default(),
            picker_typing: false,
            picker_query: String::new(),
            mode: Mode::List,
            overlay_scroll: 0,
            detail: Instance::default(),
            confirm: Instance::default(),
            confirm_action: ConfirmKind::Reboot,
            refresh: opts.refresh,
            version_param: opts.version_param,
            latest_version: String::new(),
            update_available: false,
            marked: HashSet::new(),
            panes: Vec::new(),
            focus: 0,
            broadcast_group: HashSet::new(),
            pane_dirty: Arc::new(AtomicBool::new(false)),
            leader,
            leader_pending: false,
            focus_nav: false,
            adding_pane: false,
            zoomed: false,
            scrolling: false,
            scroll_offset: 0,
            layout: Layout::Columns,
            sort_by: SortKey::Name,
            sort_asc: true,
            count_buf: String::new(),
            g_pending: false,
            name_width: 0,
            h_offset: 0,
            status: String::new(),
            loading: false,
            width: 0,
            height: 0,
            mouse: opts.mouse,
            last_click: None,
            quit: false,
        };

        if opts.initial_profile.is_empty() && !m.profiles.is_empty() {
            m.mode = Mode::Profiles;
            m.status = "select a profile".to_string();
            return m;
        }

        m.profile = opts.initial_profile;
        m.loading = true;
        m.status = "loading…".to_string();
        match (m.build)(&m.profile) {
            Err(e) => {
                m.status = format!("profile load error: {e}");
                m.loading = false;
            }
            Ok((inv, drv, region)) => {
                m.inventory = Some(inv);
                m.driver = Some(drv);
                m.region = region;
            }
        }
        m
    }

    /// Kicks off the initial load + version check.
    pub fn init(&self) {
        if self.inventory.is_some() && self.mode != Mode::Profiles {
            self.spawn_load();
            self.spawn_version_check();
        }
    }

    // ---- async command spawns ----

    fn spawn_load(&self) {
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let res = inv.list().await;
            let _ = tx.send(Msg::Loaded(res));
        });
    }

    /// Asynchronously reads the latest published version from SSM.
    /// Best-effort: failures (no param, no permission) are silently ignored.
    fn spawn_version_check(&self) {
        if self.version_param.is_empty() {
            return;
        }
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let param = self.version_param.clone();
        self.rt.spawn(async move {
            let v = inv.latest_version(&param).await.unwrap_or_default();
            let _ = tx.send(Msg::Version(v));
        });
    }

    fn spawn_reboot(&self, inst: Instance) {
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let err = inv.reboot(&inst.instance_id).await.err();
            let _ = tx.send(Msg::ActionDone {
                verb: "reboot".into(),
                name: inst.name,
                err,
            });
        });
    }

    fn spawn_delayed_refresh(&self) {
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let _ = tx.send(Msg::DelayedRefresh);
        });
    }

    /// The coalesced pane notifier: many PTY writes fold into one PaneOutput
    /// message.
    pub(crate) fn pane_notifier(&self) -> crate::session::Notifier {
        let dirty = Arc::clone(&self.pane_dirty);
        let tx = self.tx.clone();
        Arc::new(move || {
            if !dirty.swap(true, Ordering::SeqCst) {
                let _ = tx.send(Msg::PaneOutput);
            }
        })
    }

    // ---- message handling ----

    pub fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Resize(w, h) => {
                self.width = w;
                self.height = h;
                self.clamp_h_offset();
                if self.mode == Mode::Session {
                    self.relayout_session();
                }
            }

            Msg::Version(latest) => {
                self.update_available = !latest.is_empty() && latest != version::VERSION;
                self.latest_version = latest;
            }

            Msg::Loaded(res) => {
                self.loading = false;
                self.last_sync = Some(chrono::Local::now());
                self.all = res.instances;
                let warnings = res.warnings.len();
                self.name_width = max_name_width(&self.all);
                self.sort_all();
                self.clamp_h_offset();
                let sso_expired = res
                    .warnings
                    .iter()
                    .any(|w| crate::aws::is_sso_token_error(&w.err));
                self.status = if sso_expired {
                    crate::aws::sso_login_hint(&self.profile)
                } else if warnings > 0 {
                    format!(
                        "{} instances · {warnings} warning(s) — some detail unavailable (permissions)",
                        self.all.len()
                    )
                } else {
                    format!("{} instances", self.all.len())
                };
            }

            Msg::PaneOutput => {
                // A pane produced output or exited. Allow further coalesced
                // notifications, then reap panes whose process ended (returns
                // to the list when the last one goes).
                self.pane_dirty.store(false, Ordering::SeqCst);
                self.reap_exited_panes();
            }

            Msg::Tick => {
                if self.mode == Mode::List && !self.loading && self.inventory.is_some() {
                    self.spawn_load(); // silent refresh (no "Loading…")
                }
            }

            Msg::ActionDone { verb, name, err } => match err {
                Some(e) => self.status = format!("{verb} {name} failed: {e}"),
                None => {
                    self.status = format!("{verb} {name} requested · refreshing…");
                    // Refresh once shortly after, so the new state is reflected.
                    self.spawn_delayed_refresh();
                }
            },

            Msg::DelayedRefresh => {
                if self.inventory.is_some() {
                    self.spawn_load();
                }
            }

            Msg::Paste(s) => self.handle_paste(&s),

            Msg::Key(k) => self.handle_key(&k),

            Msg::Mouse(me) => self.handle_mouse(&me),
        }
    }

    fn handle_paste(&mut self, s: &str) {
        match self.mode {
            Mode::Session => {
                // Pasted text is swallowed while in
                // scroll (copy) mode — otherwise clipboard contents would run
                // in the live remote shell while the screen shows history —
                // and a paste right after the leader is consumed as the
                // leader's (unmatched) argument.
                if self.scrolling {
                    return;
                }
                if self.leader_pending {
                    self.leader_pending = false;
                    return;
                }
                if self.focus_nav {
                    self.focus_nav = false; // any other input exits focus-nav
                }
                self.session_paste(s);
            }
            Mode::Profiles if self.picker_typing => {
                self.picker_input.insert_str(s);
                self.picker_cursor = 0;
            }
            Mode::List if self.filtering => {
                self.filter.insert_str(s);
                self.apply_filter();
                self.table_to_top();
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, k: &KeyEvent) {
        let s = keymap::key_name(k);
        match self.mode {
            Mode::Profiles => self.update_profiles(k, &s),
            Mode::Detail => self.update_detail(&s),
            Mode::Help => self.update_help(&s),
            Mode::Confirm => self.update_confirm(&s),
            Mode::Session => self.update_session(k, &s),
            Mode::List => self.update_list(k, &s),
        }
    }

    pub(crate) fn should_quit(&self) -> bool {
        self.quit
    }
}

/// Compact relative age (e.g. 3d, 5h, 12m).
pub(crate) fn age_label(t: Option<chrono::DateTime<chrono::Utc>>) -> String {
    let Some(t) = t else {
        return String::new();
    };
    let d = chrono::Utc::now().signed_duration_since(t);
    let secs = d.num_seconds().max(0);
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}

/// Renders a key string like "ctrl+b" as "^b" for compact display.
pub(crate) fn leader_label(s: &str) -> String {
    if let Some(r) = s.strip_prefix("ctrl+") {
        return format!("^{r}");
    }
    s.to_string()
}

#[cfg(test)]
pub(crate) fn test_model() -> Model {
    let (tx, _rx) = std::sync::mpsc::channel::<Msg>();
    // Keep a runtime alive for the model's handle (never used by tests that
    // don't spawn).
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    let rt = RT.get_or_init(|| tokio::runtime::Runtime::new().unwrap());
    let mut m = Model::new(
        Options {
            build: Box::new(|_| Err("no aws in tests".to_string())),
            profiles: Vec::new(),
            initial_profile: String::new(),
            refresh: Duration::ZERO,
            leader: "ctrl+b".to_string(),
            version_param: String::new(),
            mouse: true,
            rt: rt.handle().clone(),
        },
        tx,
    );
    m.width = 100;
    m.height = 30;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_formats() {
        let now = chrono::Utc::now();
        assert_eq!(age_label(None), "");
        assert_eq!(age_label(Some(now - chrono::Duration::seconds(30))), "30s");
        assert_eq!(age_label(Some(now - chrono::Duration::minutes(5))), "5m");
        assert_eq!(age_label(Some(now - chrono::Duration::hours(3))), "3h");
        assert_eq!(age_label(Some(now - chrono::Duration::days(4))), "4d");
    }

    #[test]
    fn leader_labels() {
        assert_eq!(leader_label("ctrl+b"), "^b");
        assert_eq!(leader_label("f12"), "f12");
    }
}
