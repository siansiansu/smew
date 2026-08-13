//! PluginDriver builds the argv a session pane runs: `smew ssm-session …`
//! (which calls ssm:StartSession itself and execs session-manager-plugin).
//! No aws CLI involved.

/// Guards values that end up inside a `--parameters` JSON element or a
/// generated `sh -c` string: only ASCII alphanumerics plus the given extra
/// characters pass. The port-forward form applies the same host rule
/// (src/tui/app/forward.rs), but the driver re-checks so the guarantee does
/// not depend on distant call sites.
fn check_charset(what: &str, value: &str, extra: &str) -> Result<(), String> {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
    {
        Ok(())
    } else {
        Err(format!(
            "{what}: letters, digits, or one of \"{extra}\" only"
        ))
    }
}

/// Extra characters allowed in profile names, regions, and key paths, all of
/// which are interpolated into an `sh -c "…"` ProxyCommand string. Notably
/// absent: quotes, whitespace, `$`, backticks, and backslashes.
const SHELL_SAFE_EXTRA: &str = "._:/@~+-";

/// The path of the running smew binary — session panes re-invoke it for
/// the internal `ssm-session` runner. Falls back to "smew" on PATH.
fn smew_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "smew".to_string())
}

/// Picks the first existing ~/.ssh/id_*.pub, else `id_ed25519.pub` — the
/// public key the EC2 Instance Connect flows push by default.
pub fn default_pub_key() -> String {
    let Some(home) = dirs::home_dir() else {
        return "~/.ssh/id_ed25519.pub".to_string();
    };
    for f in ["id_ed25519.pub", "id_rsa.pub", "id_ecdsa.pub"] {
        let p = home.join(".ssh").join(f);
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }
    home.join(".ssh")
        .join("id_ed25519.pub")
        .to_string_lossy()
        .into_owned()
}

/// Configures SSH-over-SSM config generation.
#[derive(Debug, Clone)]
pub struct SshOptions {
    /// Pushes a 60s temporary public key per connection via EC2 Instance
    /// Connect (no permanent authorized_keys entry). Default mode.
    pub ephemeral: bool,
    /// Local public key path used in ephemeral mode.
    pub public_key: String,
}

/// Builds session argvs (smew's own ssm-session / ssh-proxy runners).
#[derive(Debug, Clone)]
pub struct PluginDriver {
    profile: String,
    region: String,
    /// Developer mode: session/forward argv runs locally instead of aws.
    dev: bool,
}

impl PluginDriver {
    /// Returns a driver bound to a profile/region (either may be empty to
    /// defer to the SDK's own resolution).
    pub fn new(profile: &str, region: &str) -> Self {
        Self {
            profile: profile.to_string(),
            region: region.to_string(),
            dev: false,
        }
    }

    /// A developer-mode driver: "sessions" run a local shell and
    /// port-forwards run a placeholder loop — no AWS calls, no network. The
    /// pane is a real PTY, so input forwarding and reaping behave exactly
    /// as in production.
    pub fn dev() -> Self {
        Self {
            profile: String::new(),
            region: String::new(),
            dev: true,
        }
    }

    /// The `--region` / `--profile` pairs to append to every runner argv
    /// (a flag is omitted when its value is empty, deferring to the SDK).
    fn flag_pairs(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("--region", self.region.as_str()),
            ("--profile", self.profile.as_str()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
    }

    /// Builds the argv for
    /// `smew ssm-session --target <id> [--region ..] [--profile ..]`.
    /// In developer mode it runs the user's shell locally instead, after a
    /// banner naming the would-be target.
    pub fn shell_command(&self, target: &str) -> Vec<String> {
        if self.dev {
            // The target rides in as $0 — a positional argument, never
            // spliced into the script — so no name can inject shell syntax.
            return vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo \"[smew dev] $0 — local shell, not an SSM session\"; exec \"${SHELL:-/bin/sh}\"".to_string(),
                target.to_string(),
            ];
        }
        [smew_exe().as_str(), "ssm-session", "--target", target]
            .into_iter()
            .map(String::from)
            .chain(
                self.flag_pairs()
                    .flat_map(|(flag, value)| [flag.to_string(), value.to_string()]),
            )
            .collect()
    }

    /// Builds the argv for an SSH login pane via EC2 Instance Connect:
    /// `smew eic-ssh` pushes a 60-second public key through the SDK, then
    /// execs `ssh <user>@<ip>` over the network (not through SSM) — the
    /// host's port 22 must be reachable from here. An empty public key
    /// skips the push (the key is already in authorized_keys).
    /// In developer mode it runs a local shell after a banner.
    pub fn ssh_shell_command(
        &self,
        target: &str,
        ip: &str,
        user: &str,
        public_key: &str,
    ) -> Result<Vec<String>, String> {
        check_charset("ssh user", user, "._-")?;
        check_charset("host ip", ip, "._-:")?;
        if self.dev {
            return Ok(vec![
                "sh".to_string(),
                "-c".to_string(),
                "echo \"[smew dev] ssh $0 — local shell, not an SSH session\"; exec \"${SHELL:-/bin/sh}\"".to_string(),
                format!("{user}@{ip}"),
            ]);
        }
        let mut argv: Vec<String> = [
            smew_exe().as_str(),
            "eic-ssh",
            "--target",
            target,
            "--ip",
            ip,
            "--user",
            user,
        ]
        .into_iter()
        .map(String::from)
        .collect();
        if !public_key.is_empty() {
            check_charset("ssh public key path", public_key, SHELL_SAFE_EXTRA)?;
            argv.push("--public-key".to_string());
            argv.push(public_key.to_string());
        }
        argv.extend(
            self.flag_pairs()
                .flat_map(|(flag, value)| [flag.to_string(), value.to_string()]),
        );
        Ok(argv)
    }

    /// Builds the argv for an SSM port-forwarding session. With a remote
    /// host it forwards through the instance to that host
    /// (AWS-StartPortForwardingSessionToRemoteHost); otherwise to a port on
    /// the instance itself (AWS-StartPortForwardingSession). The parameters
    /// are a single JSON argv element — no shell is involved — and
    /// `remote_host` is charset-checked here so it cannot break out of the
    /// JSON string (the form applies the same rule for early feedback).
    pub fn port_forward_command(
        &self,
        target: &str,
        local_port: u16,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<Vec<String>, String> {
        check_charset("remote host", remote_host, "._-:")?;
        if self.dev {
            let dest = if remote_host.is_empty() {
                target
            } else {
                remote_host
            };
            // dest is $0 (positional, not interpolated); the ports are u16.
            return Ok(vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "echo \"[smew dev] pretending to forward localhost:{local_port} -> $0:{remote_port}\"; while :; do sleep 3600; done"
                ),
                dest.to_string(),
            ]);
        }
        let mut argv = vec![
            smew_exe(),
            "ssm-session".to_string(),
            "--target".to_string(),
            target.to_string(),
            "--doc".to_string(),
            if remote_host.is_empty() {
                "AWS-StartPortForwardingSession"
            } else {
                "AWS-StartPortForwardingSessionToRemoteHost"
            }
            .to_string(),
        ];
        if !remote_host.is_empty() {
            argv.push("--param".to_string());
            argv.push(format!("host={remote_host}"));
        }
        argv.push("--param".to_string());
        argv.push(format!("portNumber={remote_port}"));
        argv.push("--param".to_string());
        argv.push(format!("localPortNumber={local_port}"));
        argv.extend(
            self.flag_pairs()
                .flat_map(|(flag, value)| [flag.to_string(), value.to_string()]),
        );
        Ok(argv)
    }

    /// Builds the ssh ProxyCommand that tunnels SSH over SSM:
    /// `smew ssh-proxy` calls StartSession itself (pushing a 60-second EC2
    /// Instance Connect key first in ephemeral mode) and streams SSH over
    /// the plugin on stdio. ssh substitutes %h (instance id), %p (port),
    /// and %r (login user). No inbound port is opened, no aws CLI involved.
    ///
    /// ssh runs the ProxyCommand through `sh`, so the interpolated values
    /// are charset-checked first; one that would need quoting is rejected
    /// rather than emitted broken.
    pub fn ssh_proxy_command(&self, opt: &SshOptions) -> Result<String, String> {
        check_charset("profile", &self.profile, SHELL_SAFE_EXTRA)?;
        check_charset("region", &self.region, SHELL_SAFE_EXTRA)?;
        if opt.ephemeral {
            check_charset("ssh public key path", &opt.public_key, SHELL_SAFE_EXTRA)?;
        }
        // "smew" from PATH on purpose: the block lands in ~/.ssh/config and
        // must outlive whatever build generated it.
        let mut cmd = String::from("smew ssh-proxy --target %h --port %p --user %r");
        if opt.ephemeral {
            cmd.push_str(" --public-key ");
            cmd.push_str(&opt.public_key);
        }
        for (flag, value) in self.flag_pairs() {
            cmd.push(' ');
            cmd.push_str(flag);
            cmd.push(' ');
            cmd.push_str(value);
        }
        Ok(cmd)
    }

    /// Returns a ~/.ssh/config block matching instance ids so that
    /// ssh/scp/rsync/sftp/VSCode-Remote work through SSM (no open port 22).
    pub fn ssh_config_block(&self, opt: &SshOptions) -> Result<String, String> {
        let mut tag = String::from("smew");
        if opt.ephemeral {
            tag.push_str(" · ephemeral (EC2 Instance Connect)");
        } else {
            tag.push_str(" · static key");
        }
        if !self.profile.is_empty() {
            tag.push_str(" · profile ");
            tag.push_str(&self.profile);
        }
        if !self.region.is_empty() {
            tag.push_str(" · region ");
            tag.push_str(&self.region);
        }
        Ok(format!(
            "# {tag}\nHost i-* mi-*\n  ProxyCommand {}\n",
            self.ssh_proxy_command(opt)?
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_args() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let argv = d.shell_command("i-0abc");
        // argv[0] is the running smew binary (varies under test)
        assert_eq!(
            &argv[1..],
            [
                "ssm-session",
                "--target",
                "i-0abc",
                "--region",
                "ap-northeast-1",
                "--profile",
                "prod"
            ]
        );
        let d = PluginDriver::new("", "");
        assert_eq!(
            &d.shell_command("i-1")[1..],
            ["ssm-session", "--target", "i-1"]
        );
    }

    #[test]
    fn ssh_shell_command_args() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let argv = d
            .ssh_shell_command(
                "i-0abc",
                "10.0.1.5",
                "ec2-user",
                "/home/u/.ssh/id_ed25519.pub",
            )
            .unwrap();
        assert_eq!(
            &argv[1..],
            [
                "eic-ssh",
                "--target",
                "i-0abc",
                "--ip",
                "10.0.1.5",
                "--user",
                "ec2-user",
                "--public-key",
                "/home/u/.ssh/id_ed25519.pub",
                "--region",
                "ap-northeast-1",
                "--profile",
                "prod"
            ]
        );
        // an empty key skips the push flag (key already in authorized_keys)
        let argv = d
            .ssh_shell_command("i-0abc", "10.0.1.5", "ubuntu", "")
            .unwrap();
        assert!(!argv.contains(&"--public-key".to_string()));
        // dev mode runs a local shell with a banner, no runner involved
        let sh = PluginDriver::dev()
            .ssh_shell_command("i-1", "1.2.3.4", "u", "")
            .unwrap();
        assert_eq!(&sh[..2], ["sh", "-c"]);
        assert_eq!(sh[3], "u@1.2.3.4");
        // hostile values are rejected before any argv is built
        assert!(d.ssh_shell_command("i-1", "1.2.3.4", "u$(x)", "").is_err());
        assert!(d.ssh_shell_command("i-1", "1.2.3.4;x", "u", "").is_err());
    }

    #[test]
    fn port_forward_command_args() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let argv = d
            .port_forward_command("i-0abc", 15432, "db.internal", 5432)
            .unwrap();
        assert_eq!(
            &argv[1..],
            [
                "ssm-session",
                "--target",
                "i-0abc",
                "--doc",
                "AWS-StartPortForwardingSessionToRemoteHost",
                "--param",
                "host=db.internal",
                "--param",
                "portNumber=5432",
                "--param",
                "localPortNumber=15432",
                "--region",
                "ap-northeast-1",
                "--profile",
                "prod"
            ]
        );
        // No remote host → forward to the instance itself.
        let d = PluginDriver::new("", "");
        assert_eq!(
            &d.port_forward_command("i-1", 8080, "", 80).unwrap()[1..],
            [
                "ssm-session",
                "--target",
                "i-1",
                "--doc",
                "AWS-StartPortForwardingSession",
                "--param",
                "portNumber=80",
                "--param",
                "localPortNumber=8080",
            ]
        );
    }

    // Developer mode must never invoke the aws CLI: sessions run a local
    // shell and forwards run a placeholder loop, both with a banner naming
    // the would-be target/ports.
    #[test]
    fn dev_driver_runs_locally() {
        let d = PluginDriver::dev();

        // The target/dest is a positional $0 argument, not part of the
        // script, so hostile names cannot inject shell syntax.
        let sh = d.shell_command("i-0abc");
        assert_eq!(&sh[..2], ["sh", "-c"]);
        assert!(sh[2].contains("$0"), "{}", sh[2]);
        assert_eq!(sh[3], "i-0abc");
        assert!(!sh[2].contains("ssm-session"), "{}", sh[2]);

        let fwd = d
            .port_forward_command("i-0abc", 15432, "db.internal", 5432)
            .unwrap();
        assert_eq!(&fwd[..2], ["sh", "-c"]);
        assert!(fwd[2].contains("localhost:15432"), "{}", fwd[2]);
        assert!(fwd[2].contains("$0:5432"), "{}", fwd[2]);
        assert_eq!(fwd[3], "db.internal");
        assert!(!fwd[2].contains("ssm-session"), "{}", fwd[2]);

        // Without a remote host, the forward targets the instance itself.
        let fwd = d.port_forward_command("i-1", 8080, "", 80).unwrap();
        assert_eq!(fwd[3], "i-1");
        assert!(fwd[2].contains(":80"), "{}", fwd[2]);
    }

    // Defense in depth: the driver itself rejects values that could break
    // out of the --parameters JSON or a generated sh -c string, independent
    // of the form-side validation.
    #[test]
    fn hostile_inputs_are_rejected() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let err = d
            .port_forward_command("i-1", 1, "bad\"host", 2)
            .unwrap_err();
        assert!(err.contains("remote host"), "{err}");
        // Same rule in dev mode.
        assert!(
            PluginDriver::dev()
                .port_forward_command("i-1", 1, "$(reboot)", 2)
                .is_err()
        );

        let opt = SshOptions {
            ephemeral: true,
            public_key: "/home/u/.ssh/id_ed25519.pub".into(),
        };
        let err = PluginDriver::new("pr\"of", "")
            .ssh_proxy_command(&opt)
            .unwrap_err();
        assert!(err.contains("profile"), "{err}");
        let err = PluginDriver::new("", "ap$(x)")
            .ssh_proxy_command(&opt)
            .unwrap_err();
        assert!(err.contains("region"), "{err}");
        let err = PluginDriver::new("", "")
            .ssh_proxy_command(&SshOptions {
                ephemeral: true,
                public_key: "/tmp/k ey.pub".into(),
            })
            .unwrap_err();
        assert!(err.contains("key path"), "{err}");
        // Static mode never embeds the key, so its path is not checked.
        assert!(
            PluginDriver::new("", "")
                .ssh_config_block(&SshOptions {
                    ephemeral: false,
                    public_key: "/tmp/k ey.pub".into(),
                })
                .is_ok()
        );
    }

    #[test]
    fn ssh_config_block_static() {
        let d = PluginDriver::new("prod", "ap-northeast-1");
        let block = d
            .ssh_config_block(&SshOptions {
                ephemeral: false,
                public_key: String::new(),
            })
            .unwrap();
        assert_eq!(
            block,
            "# smew · static key · profile prod · region ap-northeast-1\n\
             Host i-* mi-*\n\
             \x20 ProxyCommand smew ssh-proxy --target %h --port %p --user %r --region ap-northeast-1 --profile prod\n"
        );
    }

    #[test]
    fn ssh_config_block_ephemeral() {
        let d = PluginDriver::new("", "");
        let block = d
            .ssh_config_block(&SshOptions {
                ephemeral: true,
                public_key: "/home/u/.ssh/id_ed25519.pub".into(),
            })
            .unwrap();
        assert!(
            block.contains(
                "ProxyCommand smew ssh-proxy --target %h --port %p --user %r --public-key /home/u/.ssh/id_ed25519.pub"
            ),
            "{block}"
        );
        assert!(block.starts_with("# smew · ephemeral (EC2 Instance Connect)\n"));
    }
}
