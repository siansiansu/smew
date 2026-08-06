// Embeds git commit / commit time / rustc version into the binary (used by
// --version).
use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    // TZ=UTC0 so git date formatting below yields UTC.
    let out = Command::new(cmd)
        .args(args)
        .env("TZ", "UTC0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");

    let mut commit = run("git", &["rev-parse", "HEAD"]).unwrap_or_default();
    if commit.len() > 12 {
        commit.truncate(12);
    }
    if !commit.is_empty() && run("git", &["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) {
        commit.push_str("-dirty");
    }
    // Commit time in UTC.
    let when = run(
        "git",
        &[
            "show",
            "-s",
            "--format=%cd",
            "--date=format-local:%Y-%m-%dT%H:%M:%SZ",
            "HEAD",
        ],
    )
    .unwrap_or_default();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let rustv = run(&rustc, &["--version"])
        .map(|v| v.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    println!("cargo:rustc-env=SKUA_COMMIT={commit}");
    println!("cargo:rustc-env=SKUA_COMMIT_TIME={when}");
    println!("cargo:rustc-env=SKUA_RUSTC={rustv}");
}
