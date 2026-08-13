//! The root model and the message loop: mode dispatch, async command
//! spawns, and shared small helpers.

mod command;
mod forward;
mod list;
mod mouse;
mod overlays;
mod resource;
mod run;
mod runcmd;

pub use run::run;

pub(crate) use list::{LIST_ROW_H, max_name_width};
#[cfg(test)]
pub(crate) use runcmd::CmdResult;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crossterm::event::{KeyEvent, MouseEvent};

use crate::inventory::{CallerIdentity, Instance, Inventory, ListResult, Utilization};
use crate::resources::{ResourceKind, ResourceList, ResourceRow};
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
    /// Instance list result, stamped with the load generation it answers —
    /// stale responses (older profile/view) are dropped.
    Loaded {
        generation: u64,
        res: ListResult,
    },
    /// A non-instance resource view result (same staleness contract).
    ResourceLoaded {
        generation: u64,
        res: ResourceList,
    },
    Identity(CallerIdentity),
    Utilization(HashMap<String, Utilization>),
    Version(String),
    ActionDone {
        verb: &'static str,
        name: String,
        err: Option<String>,
    },
    DelayedRefresh,
    Tick,
    PaneOutput,
    /// One host's polled status/output for a dispatched run-command,
    /// stamped with the dispatch generation (stale pollers are dropped).
    CmdInvocation {
        generation: u64,
        instance_id: String,
        status: String,
        output: String,
    },
    /// The dispatch itself (ssm:SendCommand) failed.
    CmdError {
        generation: u64,
        err: String,
    },
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
    Forward,
    Session,
    /// The run-command editor (`x`): a multi-line script for the marked hosts.
    RunCmd,
    /// Per-host status/output of a dispatched run-command.
    CmdResults,
}

/// The port-forward form state (Mode::Forward). Field order: remote host,
/// remote port, local port.
#[derive(Default)]
pub(crate) struct ForwardForm {
    pub(crate) target: Instance,
    pub(crate) host: Input,
    pub(crate) port: Input,
    pub(crate) local: Input,
    pub(crate) field: FwdField,
    pub(crate) error: String,
}

/// Which port-forward form field has focus.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum FwdField {
    #[default]
    Host,
    Port,
    Local,
}

impl FwdField {
    pub(crate) fn next(self) -> FwdField {
        match self {
            FwdField::Host => FwdField::Port,
            FwdField::Port => FwdField::Local,
            FwdField::Local => FwdField::Host,
        }
    }
    pub(crate) fn prev(self) -> FwdField {
        match self {
            FwdField::Host => FwdField::Local,
            FwdField::Port => FwdField::Host,
            FwdField::Local => FwdField::Port,
        }
    }
}

/// What the confirmation dialog is asking about.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConfirmKind {
    Reboot,
    CloseSession,
}

/// The active sort column.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortKey {
    Name,
    State,
    Type,
    Cpu,
    Mem,
    Launch,
    Ip,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::State => "state",
            SortKey::Type => "type",
            SortKey::Cpu => "cpu",
            SortKey::Mem => "mem",
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
    /// Enables the %CPU/%MEM columns (CloudWatch polling). Off = no
    /// CloudWatch calls at all and the columns are hidden.
    pub metrics: bool,
    /// Login user for the SSH connect action (config ssh_user).
    pub ssh_user: String,
    /// Public key pushed via EC2 Instance Connect for SSH connects.
    pub ssh_key: String,
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
    /// Who the credentials are (sts:GetCallerIdentity), fetched async per
    /// profile; None until it resolves (the panel shows a placeholder).
    pub(crate) identity: Option<CallerIdentity>,
    pub(crate) last_sync: Option<chrono::DateTime<chrono::Local>>,
    pub(crate) all: Vec<Instance>,
    pub(crate) filtered: Vec<Instance>,

    /// Which table the main panel shows (`:` commands switch it).
    pub(crate) view: ResourceKind,
    /// Rows of the active non-instance view (empty for Instances).
    pub(crate) res_all: Vec<ResourceRow>,
    pub(crate) res_filtered: Vec<ResourceRow>,
    /// Which kind res_all currently holds — lets a drill-back to the same
    /// view render the cached rows instantly instead of a blank reload.
    pub(crate) res_kind: ResourceKind,
    /// One-level back stack for drill-down: the (view, cursor) Enter was
    /// pressed in; esc in the drilled ec2 view pops back to it.
    pub(crate) drill_from: Option<(ResourceKind, usize)>,
    /// Bumped on profile/view switches; async loads answer with the
    /// generation they were spawned under so stale results are dropped.
    pub(crate) load_gen: u64,
    /// Whether the %CPU/%MEM columns (and their CloudWatch polling) are on.
    pub(crate) metrics_enabled: bool,
    /// CloudWatch CPU/MEM by instance id; hosts without data show n/a.
    pub(crate) util: HashMap<String, Utilization>,
    /// When utilization was last fetched (rate-limits the CloudWatch calls).
    pub(crate) last_util_fetch: Option<std::time::Instant>,

    // instance table state
    pub(crate) cursor: usize,
    pub(crate) row_offset: usize,

    pub(crate) filter: Input,
    pub(crate) filtering: bool,

    // `:` command mode (k9s-style prompt)
    pub(crate) cmd: Input,
    pub(crate) commanding: bool,
    pub(crate) cmd_sel: usize,   // selected suggestion (↑↓ cycles)
    pub(crate) cmd_last: String, // ↑ on an empty prompt recalls this

    // profile picker
    pub(crate) profiles: Vec<String>,
    pub(crate) picker_cursor: usize,
    /// fzf-style query: typing in the picker filters immediately.
    pub(crate) picker_input: Input,

    pub(crate) mode: Mode,
    pub(crate) overlay_scroll: usize, // detail/help vertical scroll offset
    pub(crate) detail: Instance,
    /// The row shown by the detail overlay in non-instance views.
    pub(crate) res_detail: ResourceRow,
    pub(crate) confirm: Instance,
    pub(crate) confirm_action: ConfirmKind,
    pub(crate) fwd: ForwardForm,
    pub(crate) refresh: Duration,

    // update check
    pub(crate) version_param: String,
    pub(crate) latest_version: String,
    pub(crate) update_available: bool,

    /// Hosts marked with `space` — the target set of the run-command
    /// action (`x`); empty means "the host under the cursor".
    pub(crate) marked: HashSet<String>,

    // run-command state (`x`)
    pub(crate) cmd_editor: super::input::MultiInput,
    pub(crate) cmd_targets: Vec<Instance>,
    pub(crate) cmd_results: Vec<runcmd::CmdResult>,
    /// Bumped per dispatch; pollers answer with the generation they were
    /// spawned under so results of an older run are dropped.
    pub(crate) cmd_gen: u64,

    // single-pane session state (k9s-style: one full-screen shell)
    pub(crate) pane: Option<Arc<Pane>>,
    pub(crate) pane_dirty: Arc<AtomicBool>,
    pub(crate) leader: String,
    pub(crate) leader_pending: bool,
    pub(crate) ssh_user: String,
    pub(crate) ssh_key: String,
    pub(crate) scrolling: bool,
    pub(crate) scroll_offset: usize,

    pub(crate) sort_by: SortKey,
    pub(crate) sort_asc: bool,

    pub(crate) count_buf: String, // vim-style numeric prefix (e.g. "10" then gg)
    pub(crate) g_pending: bool,   // first 'g' of a gg motion was pressed

    pub(crate) name_width: usize, // NAME column width = longest name
    pub(crate) h_offset: usize,   // horizontal scroll offset in cells

    pub(crate) status: String,
    pub(crate) loading: bool,
    /// A list refresh is in flight (header shows "syncing…").
    pub(crate) syncing: bool,
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
            identity: None,
            last_sync: None,
            all: Vec::new(),
            filtered: Vec::new(),
            view: ResourceKind::Instances,
            res_all: Vec::new(),
            res_filtered: Vec::new(),
            res_kind: ResourceKind::Instances,
            drill_from: None,
            load_gen: 0,
            metrics_enabled: opts.metrics,
            util: HashMap::new(),
            last_util_fetch: None,
            cursor: 0,
            row_offset: 0,
            filter: Input::default(),
            filtering: false,
            cmd: Input::default(),
            commanding: false,
            cmd_sel: 0,
            cmd_last: String::new(),
            profiles: opts.profiles,
            picker_cursor: 0,
            picker_input: Input::default(),
            mode: Mode::List,
            overlay_scroll: 0,
            detail: Instance::default(),
            res_detail: ResourceRow::default(),
            confirm: Instance::default(),
            confirm_action: ConfirmKind::Reboot,
            fwd: ForwardForm::default(),
            refresh: opts.refresh,
            version_param: opts.version_param,
            latest_version: String::new(),
            update_available: false,
            marked: HashSet::new(),
            cmd_editor: super::input::MultiInput::default(),
            cmd_targets: Vec::new(),
            cmd_results: Vec::new(),
            cmd_gen: 0,
            pane: None,
            pane_dirty: Arc::new(AtomicBool::new(false)),
            leader,
            leader_pending: false,
            ssh_user: opts.ssh_user,
            ssh_key: opts.ssh_key,
            scrolling: false,
            scroll_offset: 0,
            sort_by: SortKey::Name,
            sort_asc: true,
            count_buf: String::new(),
            g_pending: false,
            name_width: 0,
            h_offset: 0,
            status: String::new(),
            loading: false,
            syncing: false,
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
    pub fn init(&mut self) {
        if self.inventory.is_some() && self.mode != Mode::Profiles {
            self.spawn_load();
            self.spawn_identity();
            self.spawn_version_check();
        }
    }

    // ---- async command spawns ----

    fn spawn_load(&mut self) {
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        self.syncing = true;
        let tx = self.tx.clone();
        let generation = self.load_gen;
        self.rt.spawn(async move {
            let res = inv.list().await;
            let _ = tx.send(Msg::Loaded { generation, res });
        });
    }

    /// Loads the active non-instance resource view.
    pub(super) fn spawn_load_resources(&mut self) {
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        self.syncing = true;
        let tx = self.tx.clone();
        let kind = self.view;
        let generation = self.load_gen;
        self.rt.spawn(async move {
            let res = inv.list_resources(kind).await;
            let _ = tx.send(Msg::ResourceLoaded { generation, res });
        });
    }

    /// Refreshes whichever view is active.
    pub(super) fn spawn_load_active(&mut self) {
        if self.view == ResourceKind::Instances {
            self.spawn_load();
        } else {
            self.spawn_load_resources();
        }
    }

    /// Switches the main panel to another resource view: bumps the load
    /// generation (stale in-flight results get dropped), clears view state,
    /// and fetches. Instances keeps its cached rows while refreshing, and a
    /// return to the kind res_all still holds renders the cache instantly.
    pub(super) fn switch_view(&mut self, kind: ResourceKind) {
        if kind == self.view {
            self.status.clear();
            return;
        }
        self.load_gen += 1;
        self.view = kind;
        self.drill_from = None; // a lateral move invalidates the back path
        if kind != ResourceKind::Instances && kind != self.res_kind {
            self.res_all.clear();
            self.res_kind = kind;
        }
        self.filter.clear();
        self.filtering = false;
        self.apply_filter();
        self.table_to_top();
        self.h_offset = 0;
        if self.row_count() == 0 {
            self.status = format!("loading {}…", kind.title());
        } else {
            self.status.clear(); // cached rows are already on screen
        }
        self.spawn_load_active();
    }

    /// Fetches CPU/MEM utilization for the listed instances. Rate-limited to
    /// the metric's own 5-minute resolution: polling faster returns the same
    /// datapoint while spending free-tier API requests for nothing.
    fn spawn_utilization(&mut self) {
        const UTIL_MIN_INTERVAL: Duration = Duration::from_secs(300);
        if !self.metrics_enabled
            || self.all.is_empty()
            || self
                .last_util_fetch
                .is_some_and(|t| t.elapsed() < UTIL_MIN_INTERVAL)
        {
            return;
        }
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        self.last_util_fetch = Some(std::time::Instant::now());
        let ids: Vec<String> = self.all.iter().map(|i| i.instance_id.clone()).collect();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            // Warnings are dropped on purpose: metrics are an optional
            // enrichment (n/a in the table), and a permission error would
            // otherwise repeat in the status bar every fetch.
            let (util, _warnings) = inv.utilization(&ids).await;
            let _ = tx.send(Msg::Utilization(util));
        });
    }

    /// Resolves the caller identity for the top panel. Best-effort: on
    /// error the Account/User rows keep their placeholder (a credential
    /// problem already surfaces via the list warnings).
    pub(super) fn spawn_identity(&self) {
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            if let Ok(id) = inv.identity().await {
                let _ = tx.send(Msg::Identity(id));
            }
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
                verb: "reboot",
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

            Msg::Identity(id) => self.identity = Some(id),

            Msg::Version(latest) => {
                self.update_available = !latest.is_empty() && latest != version::VERSION;
                self.latest_version = latest;
            }

            Msg::Loaded { generation, res } => {
                if generation != self.load_gen {
                    return; // stale: answered an older profile/view
                }
                self.loading = false;
                self.syncing = false;
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
                self.spawn_utilization();
            }

            Msg::ResourceLoaded { generation, res } => {
                if generation != self.load_gen || res.kind != self.view {
                    return; // stale or answers a view we already left
                }
                self.loading = false;
                self.syncing = false;
                self.last_sync = Some(chrono::Local::now());
                self.res_kind = res.kind;
                self.res_all = res.rows;
                self.apply_filter();
                self.clamp_h_offset();
                let sso_expired = res
                    .warnings
                    .iter()
                    .any(|w| crate::aws::is_sso_token_error(&w.err));
                self.status = if sso_expired {
                    crate::aws::sso_login_hint(&self.profile)
                } else if let Some(w) = res.warnings.first() {
                    format!("{}: {}", w.op, w.err)
                } else {
                    format!("{} {}", self.res_all.len(), self.view.title())
                };
            }

            Msg::Utilization(util) => {
                self.util = util;
                if matches!(self.sort_by, SortKey::Cpu | SortKey::Mem) {
                    self.sort_all();
                }
            }

            Msg::CmdInvocation {
                generation,
                instance_id,
                status,
                output,
            } => self.apply_cmd_invocation(generation, &instance_id, status, output),

            Msg::CmdError { generation, err } => {
                if generation == self.cmd_gen {
                    for r in &mut self.cmd_results {
                        r.status = "Error".to_string();
                        r.output = err.clone();
                    }
                    self.status = format!("ssm:SendCommand failed: {err}");
                }
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
                    self.spawn_load_active(); // silent refresh (no "Loading…")
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
                    self.spawn_load_active();
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
                self.session_paste(s);
            }
            Mode::Profiles => {
                self.picker_input.insert_str(s);
                self.picker_cursor = 0;
            }
            Mode::List if self.commanding => {
                self.cmd.insert_str(s);
                self.cmd_sel = 0;
            }
            Mode::List if self.filtering => {
                self.filter.insert_str(s);
                self.apply_filter();
                self.table_to_top();
            }
            Mode::Forward => {
                self.forward_field_mut().insert_str(s);
                self.fwd.error.clear();
            }
            // Pasting a whole script into the run-command editor works —
            // embedded newlines split into lines.
            Mode::RunCmd => self.cmd_editor.insert_str(s),
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
            Mode::Forward => self.update_forward(k, &s),
            Mode::Session => self.update_session(k, &s),
            Mode::RunCmd => self.update_run_cmd(k, &s),
            Mode::CmdResults => self.update_cmd_results(&s),
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
            metrics: true, // view tests exercise the %CPU/%MEM columns
            ssh_user: "ec2-user".to_string(),
            ssh_key: String::new(),
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
