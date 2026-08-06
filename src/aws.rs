//! Wraps AWS SDK config loading and shared-profile enumeration.
//!
//! Relies on the standard credential chain (explicit profile > env vars >
//! shared default profile).

use std::path::{Path, PathBuf};

use aws_config::{BehaviorVersion, Region, SdkConfig};

/// Builds an SdkConfig. An empty profile or region falls back to the SDK
/// defaults (env vars / shared config). A named profile that doesn't exist
/// in the shared config/credentials files fails here, up front (the SDK
/// resolves credentials lazily and would otherwise surface the error only on
/// the first API call).
pub async fn load(profile: &str, region: &str) -> Result<SdkConfig, String> {
    if !profile.is_empty() && !profiles().iter().any(|p| p == profile) {
        return Err(format!("failed to get shared config profile, {profile}"));
    }
    let mut b = aws_config::defaults(BehaviorVersion::latest());
    if !profile.is_empty() {
        b = b.profile_name(profile);
    }
    if !region.is_empty() {
        b = b.region(Region::new(region.to_string()));
    }
    Ok(b.load().await)
}

/// The resolved region of a loaded config ("" when none).
pub fn region_of(cfg: &SdkConfig) -> String {
    cfg.region().map(|r| r.to_string()).unwrap_or_default()
}

/// Returns the caller's account and ARN via sts:GetCallerIdentity — a quick
/// way to confirm the resolved credentials actually work.
pub async fn identity(cfg: &SdkConfig) -> Result<String, String> {
    let out = aws_sdk_sts::Client::new(cfg)
        .get_caller_identity()
        .send()
        .await
        .map_err(|e| format!("{}", aws_sdk_sts::error::DisplayErrorContext(&e)))?;
    Ok(format!(
        "account={} arn={}",
        out.account().unwrap_or_default(),
        out.arn().unwrap_or_default()
    ))
}

/// Enumerates the AWS profiles available on this machine by parsing the
/// shared config and credentials files directly — the same profiles the aws
/// CLI would see. It never makes a network call or uses credentials.
///
///   - ~/.aws/config       sections: [profile foo], [default]
///   - ~/.aws/credentials  sections: [foo]
///
/// Honors AWS_CONFIG_FILE / AWS_SHARED_CREDENTIALS_FILE overrides.
pub fn profiles() -> Vec<String> {
    profiles_from(&config_path(), &credentials_path())
}

fn profiles_from(config: &Path, credentials: &Path) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut add = |name: &str| {
        let name = name.trim();
        if name.is_empty() || !seen.insert(name.to_string()) {
            return;
        }
        out.push(name.to_string());
    };

    parse_sections(config, |section| {
        if section == "default" {
            add("default");
        } else if let Some(rest) = section.strip_prefix("profile ") {
            add(rest);
        }
    });
    parse_sections(credentials, add);

    out.sort();
    out
}

fn config_path() -> PathBuf {
    if let Ok(v) = std::env::var("AWS_CONFIG_FILE")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    home_dir().join(".aws").join("config")
}

fn credentials_path() -> PathBuf {
    if let Ok(v) = std::env::var("AWS_SHARED_CREDENTIALS_FILE")
        && !v.is_empty()
    {
        return PathBuf::from(v);
    }
    home_dir().join(".aws").join("credentials")
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// Calls f with the inner text of every `[section]` line.
fn parse_sections(path: &Path, mut f: impl FnMut(&str)) {
    let Ok(data) = std::fs::read_to_string(path) else {
        return;
    };
    for line in data.lines() {
        let line = line.trim();
        if let Some(inner) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            f(inner.trim());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_profiles_from_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config");
        let creds = dir.path().join("credentials");
        let mut f = std::fs::File::create(&cfg).unwrap();
        writeln!(
            f,
            "[default]\nregion = us-east-1\n\n[profile prod]\nregion = ap-northeast-1\n\n[profile staging]\n"
        )
        .unwrap();
        let mut f = std::fs::File::create(&creds).unwrap();
        writeln!(f, "[prod]\naws_access_key_id = X\n\n[legacy]\n").unwrap();

        let got = profiles_from(&cfg, &creds);
        assert_eq!(got, vec!["default", "legacy", "prod", "staging"]);
    }

    #[test]
    fn missing_files_yield_empty() {
        let dir = tempfile::tempdir().unwrap();
        let got = profiles_from(&dir.path().join("nope"), &dir.path().join("nada"));
        assert!(got.is_empty());
    }
}
