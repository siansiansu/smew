//! The run-command flow (`x`): a multi-line script sent to the marked
//! hosts via ssm:SendCommand (AWS-RunShellScript), with per-host results
//! polled through ssm:GetCommandInvocation.

use std::time::Duration;

use crossterm::event::KeyEvent;

use super::{Mode, Model, Msg};

/// How often each host's invocation is polled, and for how long before the
/// poller gives up (Run Command itself keeps running server-side).
const POLL_EVERY: Duration = Duration::from_secs(2);
const POLL_TRIES: u32 = 150; // × 2s = 5 minutes

/// One host's row on the results screen.
#[derive(Debug, Clone)]
pub(crate) struct CmdResult {
    pub(crate) instance_id: String,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) output: String,
}

impl CmdResult {
    /// Whether the invocation reached a terminal state.
    pub(crate) fn is_done(&self) -> bool {
        !matches!(self.status.as_str(), "Pending" | "InProgress" | "Delayed")
    }
}

impl Model {
    /// Opens the run-command editor for the marked hosts (else the host
    /// under the cursor). The previous script is kept for re-runs.
    pub(crate) fn open_run_cmd(&mut self) {
        let targets = self.command_targets();
        if targets.is_empty() {
            self.status = "no SSM-online host selected — mark with space or move the cursor".into();
            return;
        }
        self.cmd_targets = targets;
        self.mode = Mode::RunCmd;
        self.status.clear();
    }

    /// Keys in the run-command editor: ctrl+s sends, esc cancels, the rest
    /// edits the script.
    pub(crate) fn update_run_cmd(&mut self, k: &KeyEvent, s: &str) {
        match s {
            "ctrl+c" => self.quit = true,
            "esc" => self.mode = Mode::List,
            "ctrl+s" => self.dispatch_run_cmd(),
            _ => {
                self.cmd_editor.handle(k);
            }
        }
    }

    /// Sends the script to the targets and switches to the results screen.
    fn dispatch_run_cmd(&mut self) {
        if self.cmd_editor.is_blank() {
            self.status = "nothing to run — type a command first".to_string();
            return;
        }
        let Some(inv) = self.inventory.clone() else {
            return;
        };
        self.cmd_gen += 1;
        let generation = self.cmd_gen;
        self.cmd_results = self
            .cmd_targets
            .iter()
            .map(|t| CmdResult {
                instance_id: t.instance_id.clone(),
                name: t.name.clone(),
                status: "Pending".to_string(),
                output: String::new(),
            })
            .collect();
        self.overlay_scroll = 0;
        self.mode = Mode::CmdResults;
        self.status.clear();

        let ids: Vec<String> = self
            .cmd_targets
            .iter()
            .map(|t| t.instance_id.clone())
            .collect();
        let lines: Vec<String> = self.cmd_editor.lines().to_vec();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let command_id = match inv.run_command(&ids, &lines).await {
                Ok(id) => id,
                Err(err) => {
                    let _ = tx.send(Msg::CmdError { generation, err });
                    return;
                }
            };
            // One poller per host: report every status change, stop on a
            // terminal state (or after the poll budget).
            for id in ids {
                let inv = inv.clone();
                let tx = tx.clone();
                let command_id = command_id.clone();
                tokio::spawn(async move {
                    let mut last = String::new();
                    for _ in 0..POLL_TRIES {
                        match inv.command_invocation(&command_id, &id).await {
                            Ok((status, output)) => {
                                let done = !matches!(
                                    status.as_str(),
                                    "Pending" | "InProgress" | "Delayed"
                                );
                                if done || status != last {
                                    last = status.clone();
                                    let _ = tx.send(Msg::CmdInvocation {
                                        generation,
                                        instance_id: id.clone(),
                                        status,
                                        output,
                                    });
                                }
                                if done {
                                    return;
                                }
                            }
                            Err(err) => {
                                let _ = tx.send(Msg::CmdInvocation {
                                    generation,
                                    instance_id: id.clone(),
                                    status: "Error".to_string(),
                                    output: err,
                                });
                                return;
                            }
                        }
                        tokio::time::sleep(POLL_EVERY).await;
                    }
                    let _ = tx.send(Msg::CmdInvocation {
                        generation,
                        instance_id: id.clone(),
                        status: "Error".to_string(),
                        output: "gave up polling after 5 minutes (the command may still be running — see the AWS console)".to_string(),
                    });
                });
            }
        });
    }

    /// Records one host's polled status/output (Msg::CmdInvocation).
    pub(crate) fn apply_cmd_invocation(
        &mut self,
        generation: u64,
        instance_id: &str,
        status: String,
        output: String,
    ) {
        if generation != self.cmd_gen {
            return; // a poller of an older dispatch
        }
        if let Some(r) = self
            .cmd_results
            .iter_mut()
            .find(|r| r.instance_id == instance_id)
        {
            r.status = status;
            r.output = output;
        }
    }

    /// All invocations reached a terminal state.
    pub(crate) fn cmd_all_done(&self) -> bool {
        self.cmd_results.iter().all(CmdResult::is_done)
    }

    /// Keys on the results screen: esc/q back to the editor's list, x
    /// reopens the editor (same script, same targets), the rest scrolls.
    pub(crate) fn update_cmd_results(&mut self, s: &str) {
        match s {
            "ctrl+c" => self.quit = true,
            "esc" | "q" | "enter" => self.mode = Mode::List,
            "x" => self.mode = Mode::RunCmd,
            _ => self.overlay_scroll_key(s, self.cmd_results_height(), self.page_rows()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Instance;
    use crate::tui::test_model;
    use crossterm::event::{KeyCode, KeyModifiers};

    fn online(name: &str) -> Instance {
        Instance {
            instance_id: format!("i-{name}"),
            name: name.to_string(),
            state: "running".to_string(),
            ssm: Some(crate::inventory::SsmStatus {
                online: true,
                agent_version: "3.3".into(),
                ping_status: "Online".into(),
            }),
            ..Default::default()
        }
    }

    fn key(m: &mut crate::tui::Model, code: KeyCode, mods: KeyModifiers) {
        let k = KeyEvent::new(code, mods);
        let s = crate::tui::keymap::key_name(&k);
        match m.mode {
            Mode::RunCmd => m.update_run_cmd(&k, &s),
            _ => panic!("unexpected mode"),
        }
    }

    #[test]
    fn open_edit_and_dispatch() {
        let mut m = test_model();
        m.inventory = Some(crate::inventory::Inventory::mock());
        m.all = vec![online("web"), online("db")];
        m.apply_filter();
        m.marked.insert("i-web".to_string());
        m.marked.insert("i-db".to_string());

        m.open_run_cmd();
        assert_eq!(m.mode, Mode::RunCmd);
        assert_eq!(m.cmd_targets.len(), 2);

        // type a two-line script and send it
        for c in "echo hi".chars() {
            key(&mut m, KeyCode::Char(c), KeyModifiers::NONE);
        }
        key(&mut m, KeyCode::Enter, KeyModifiers::NONE);
        for c in "uptime".chars() {
            key(&mut m, KeyCode::Char(c), KeyModifiers::NONE);
        }
        key(&mut m, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(m.mode, Mode::CmdResults);
        assert_eq!(m.cmd_results.len(), 2);
        assert!(!m.cmd_all_done());

        // results arrive per host; a stale generation is dropped
        m.apply_cmd_invocation(m.cmd_gen, "i-web", "Success".into(), "hi\n".into());
        m.apply_cmd_invocation(0, "i-db", "Failed".into(), String::new());
        assert_eq!(m.cmd_results[0].status, "Success");
        assert_eq!(m.cmd_results[1].status, "Pending");
        m.apply_cmd_invocation(m.cmd_gen, "i-db", "Success".into(), String::new());
        assert!(m.cmd_all_done());

        // esc returns to the list; x reopens the editor with the script kept
        m.update_cmd_results("esc");
        assert_eq!(m.mode, Mode::List);
        m.open_run_cmd();
        assert_eq!(m.cmd_editor.lines(), ["echo hi", "uptime"]);
    }

    #[test]
    fn blank_script_and_no_targets_report() {
        let mut m = test_model();
        m.inventory = Some(crate::inventory::Inventory::mock());
        m.open_run_cmd();
        assert_eq!(m.mode, Mode::List, "no target — the editor must not open");
        assert!(m.status.contains("no SSM-online host"), "{}", m.status);

        m.all = vec![online("web")];
        m.apply_filter();
        m.open_run_cmd();
        assert_eq!(m.mode, Mode::RunCmd, "the cursor host is the target");
        assert_eq!(m.cmd_targets.len(), 1);
        key(&mut m, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(m.mode, Mode::RunCmd, "a blank script must not dispatch");
        assert!(m.status.contains("nothing to run"), "{}", m.status);
    }
}
