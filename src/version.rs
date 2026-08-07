//! The smew version shown in the UI title bar and by the --version flag.

/// Current smew version (from Cargo.toml; bump there when releasing).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn or_unknown(s: &str) -> &str {
    if s.is_empty() { "unknown" } else { s }
}

/// Version plus the git commit and commit time embedded at build time, so a
/// distributed binary is identifiable. Used by --version.
pub fn details() -> String {
    let commit = or_unknown(env!("SMEW_COMMIT"));
    let when = or_unknown(env!("SMEW_COMMIT_TIME"));
    let rustv = or_unknown(env!("SMEW_RUSTC"));
    format!(
        "smew {VERSION}\n  commit: {commit}\n  built:  {when}\n  rust:   {rustv} {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}
