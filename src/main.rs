//! Binary entry point: CLI parsing and the non-TUI modes (--version,
//! --ssh-config, --dry-run); everything else lives in the smew library.

use clap::Parser;

use smew::inventory::Instance;
use smew::session::{PluginDriver, SshOptions};
use smew::{aws, config, inventory, theme, tui, version};

/// Local AWS SSM connection tool: interactive inventory browser + multiplexing.
#[derive(Parser, Debug)]
#[command(name = "smew", disable_version_flag = true)]
struct Cli {
    /// AWS profile (default: config / env / shared default profile)
    #[arg(long)]
    profile: Option<String>,
    /// AWS region (default: config / profile / env region)
    #[arg(long)]
    region: Option<String>,
    /// Print resolved identity + inventory as a table and exit (no TUI)
    #[arg(long)]
    dry_run: bool,
    /// Developer mode: mock inventory + local-shell sessions, no AWS access
    /// (for developing/demoing the TUI offline)
    #[arg(long)]
    dev: bool,
    /// Print an ~/.ssh/config block for ssh/scp over SSM and exit
    #[arg(long)]
    ssh_config: bool,
    /// With --ssh-config: use a static `authorized_keys` key instead of
    /// ephemeral EC2 Instance Connect
    #[arg(long)]
    ssh_static: bool,
    /// With --ssh-config (ephemeral): public key to push (default:
    /// auto-detect ~/.ssh/id_*.pub)
    #[arg(long)]
    ssh_key: Option<String>,
    /// Print version (with git commit + build time) and exit
    #[arg(long)]
    version: bool,
}

fn main() {
    let cli = Cli::parse();

    if cli.version {
        println!("{}", version::details());
        return;
    }

    // Load config (optional file); CLI flags take precedence over it.
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to read config: {e}");
            std::process::exit(1);
        }
    };
    let profile = cli.profile.unwrap_or_else(|| cfg.default_profile.clone());
    let region = cli.region.unwrap_or_else(|| cfg.default_region.clone());

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    if cli.ssh_config {
        rt.block_on(print_ssh_config(
            &profile,
            &region,
            !cli.ssh_static,
            cli.ssh_key,
        ));
        return;
    }

    if cli.dry_run {
        rt.block_on(run_dry_run(&profile, &region, cli.dev));
        return;
    }

    // A misconfigured skin fails fast with the resolution hint rather than
    // silently falling back — the TUI would hide any warning.
    match theme::load(&cfg.skin) {
        Ok(t) => theme::init(t),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }

    // build resolves a profile to a fresh inventory client + session driver,
    // so the TUI can switch AWS profiles at runtime. Developer mode resolves
    // every profile to the mock backend — switching always succeeds offline.
    let build: tui::BuildFn = if cli.dev {
        Box::new(|_prof: &str| {
            Ok((
                inventory::Inventory::mock(),
                PluginDriver::dev(),
                "local".to_string(),
            ))
        })
    } else {
        let handle = rt.handle().clone();
        let region = region.clone();
        Box::new(move |prof: &str| {
            let cfg = handle.block_on(aws::load(prof, &region))?;
            let resolved = aws::region_of(&cfg);
            Ok((
                inventory::Inventory::new(&cfg),
                PluginDriver::new(prof, &resolved),
                resolved,
            ))
        })
    };

    // Fake profiles in dev mode so the picker/switching flows are testable.
    let (profiles, initial_profile) = if cli.dev {
        (
            ["dev", "dev-staging", "dev-prod"].map(String::from).into(),
            "dev".to_string(),
        )
    } else {
        (aws::profiles(), profile)
    };

    let opts = tui::Options {
        build,
        profiles,
        initial_profile,
        refresh: cfg.refresh_interval(),
        leader: cfg.leader().to_string(),
        version_param: cfg.version_param_name().to_string(),
        mouse: cfg.mouse_enabled(),
        // Dev mode always shows the metrics columns — the mock data is free.
        metrics: cfg.metrics || cli.dev,
        rt: rt.handle().clone(),
    };
    if let Err(e) = tui::run(opts) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Prints an ~/.ssh/config block (to stdout) that makes ssh/scp/rsync/sftp to
/// instance ids work over SSM, plus usage notes (stderr).
async fn print_ssh_config(profile: &str, region: &str, ephemeral: bool, pub_key: Option<String>) {
    // Resolve the profile's region without a network call, for accuracy.
    let mut region = region.to_string();
    if region.is_empty()
        && let Ok(cfg) = aws::load(profile, "").await
    {
        region = aws::region_of(&cfg);
    }
    let mut pub_key = pub_key.unwrap_or_default();
    if ephemeral && pub_key.is_empty() {
        pub_key = default_pub_key();
    }
    let drv = PluginDriver::new(profile, &region);
    let opt = SshOptions {
        ephemeral,
        public_key: pub_key,
    };

    eprintln!("# Add this to ~/.ssh/config (or a file Include'd from it), e.g.:");
    eprintln!("#   smew --ssh-config >> ~/.ssh/config");
    eprintln!(
        "# Then: ssh <user>@i-0abc...   scp ./f <user>@i-0abc...:/tmp/   (user = ec2-user/ubuntu/…)"
    );
    if ephemeral {
        eprintln!("#");
        eprintln!("# Mode: EPHEMERAL (EC2 Instance Connect). Pushes a 60s temporary public key");
        eprintln!(
            "#   ({}) per connection — no permanent authorized_keys entry.",
            opt.public_key
        );
        eprintln!(
            "# Requires: sshd on the host; the ec2-instance-connect package (preinstalled on"
        );
        eprintln!(
            "#   AL2/AL2023/Ubuntu); IAM ec2-instance-connect:SendSSHPublicKey + ssm:StartSession"
        );
        eprintln!(
            "#   on AWS-StartSSHSession. Use --ssh-static for a static authorized_keys key instead."
        );
    } else {
        eprintln!("#");
        eprintln!(
            "# Mode: STATIC. Requires your public key already in the host's authorized_keys,"
        );
        eprintln!("#   sshd running, and IAM ssm:StartSession on AWS-StartSSHSession.");
    }
    eprintln!("# No inbound port 22 is opened in either mode.");
    eprintln!();
    match drv.ssh_config_block(&opt) {
        Ok(block) => print!("{block}"),
        Err(e) => {
            eprintln!("cannot generate ssh config: {e}");
            std::process::exit(1);
        }
    }
}

/// Picks the first existing ~/.ssh/id_*.pub, else `id_ed25519.pub`.
fn default_pub_key() -> String {
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

/// Prints caller identity and the full enriched inventory as an aligned table
/// (wide-table fields), so inventory logic can be verified without a TTY.
/// In developer mode the mock inventory is printed with no AWS calls.
async fn run_dry_run(profile: &str, region: &str, dev: bool) {
    let mut sso_expired = false;
    let inv = if dev {
        println!("caller identity: developer mode (mock inventory, no AWS calls)");
        println!("region:          local\n");
        inventory::Inventory::mock()
    } else {
        let cfg = match aws::load(profile, region).await {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("failed to load AWS config: {e}");
                std::process::exit(1);
            }
        };
        match aws::identity(&cfg).await {
            Ok(id) => println!("caller identity: {id}"),
            Err(e) => {
                sso_expired |= aws::is_sso_token_error(&e);
                println!("caller identity: ERROR: {e}");
            }
        }
        println!("region:          {}\n", aws::region_of(&cfg));
        inventory::Inventory::new(&cfg)
    };
    let res = inv.list().await;

    let mut rows: Vec<Vec<String>> = vec![
        [
            "SSM",
            "NAME",
            "INSTANCE-ID",
            "STATE",
            "TYPE",
            "LAUNCHED",
            "AZ",
            "PRIVATE-IP",
            "VPC",
            "SUBNET",
            "SG",
            "TAGS",
        ]
        .map(String::from)
        .into(),
    ];
    for inst in &res.instances {
        let launched = inst
            .launch_time
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .unwrap_or_default();
        rows.push(vec![
            if inst.is_connectable() { "yes" } else { "no" }.to_string(),
            sanitize(&inst.name),
            sanitize(&inst.instance_id),
            sanitize(&inst.state),
            sanitize(&inst.instance_type),
            launched,
            sanitize(&inst.az),
            sanitize(&inst.private_ip),
            sanitize(&inst.vpc_id),
            sanitize(&inst.subnet_id),
            sanitize(&sg_summary(inst)),
            sanitize(&tag_summary(inst)),
        ]);
    }
    print!("{}", align_columns(&rows));

    println!("\n{} instances", res.instances.len());
    for warn in &res.warnings {
        println!("WARNING {}: {}", warn.op, warn.err);
    }
    sso_expired |= res
        .warnings
        .iter()
        .any(|warn| aws::is_sso_token_error(&warn.err));
    if sso_expired {
        println!("{}", aws::sso_login_hint(profile));
    }
}

/// Left-aligns rows into columns padded to the widest cell + two spaces.
fn align_columns(rows: &[Vec<String>]) -> String {
    use std::fmt::Write;

    const PAD: usize = 2;
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut width = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            width[i] = width[i].max(cell.chars().count());
        }
    }
    let mut out = String::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i + 1 == row.len() {
                out.push_str(cell);
            } else {
                write!(out, "{:w$}", cell, w = width[i] + PAD)
                    .expect("writing to a String cannot fail");
            }
        }
        out.push('\n');
    }
    out
}

fn sg_summary(inst: &Instance) -> String {
    inst.security_groups
        .iter()
        .map(|sg| {
            if sg.name.is_empty() {
                sg.id.as_str()
            } else {
                sg.name.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn tag_summary(inst: &Instance) -> String {
    inst.tags
        .iter()
        .filter(|(k, _)| k.as_str() != "Name")
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Strips control characters from AWS-derived strings before they reach the
/// terminal. Anyone with `ec2:CreateTags` in the account controls tag and
/// name content, so --dry-run output must not pass raw ANSI/OSC sequences
/// through. (The TUI path is already safe: ratatui filters control chars.)
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn sanitize_strips_control_chars() {
        assert_eq!(
            sanitize("web-\x1b]0;pwned\x07\x1b[2J01\n"),
            "web-]0;pwned[2J01"
        );
        assert_eq!(sanitize("plain-name_1.2 ürlaub"), "plain-name_1.2 ürlaub");
        assert_eq!(sanitize(""), "");
    }
}
