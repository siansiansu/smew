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
        return Err(format!(
            "profile `{profile}` not found in {} or {}",
            config_path().display(),
            credentials_path().display()
        ));
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

/// Whether an error message indicates an expired or missing SSO token.
/// aws-config's SsoTokenProviderError variants all render with the phrase
/// "SSO token" ("the SSO token has expired and cannot be refreshed",
/// "failed to load the cached SSO token", "… cached SSO token file"), and
/// DisplayErrorContext includes the full cause chain in our messages.
pub fn is_sso_token_error(msg: &str) -> bool {
    msg.to_ascii_lowercase().contains("sso token")
}

/// An actionable re-login hint for an SSO token error. An empty profile
/// defers to the CLI's default profile resolution.
pub fn sso_login_hint(profile: &str) -> String {
    if profile.is_empty() {
        "AWS SSO session expired or missing — run: aws sso login".to_string()
    } else {
        format!("AWS SSO session expired or missing — run: aws sso login --profile {profile}")
    }
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
    // A BTreeSet gives dedupe across both files plus sorted output.
    let mut names = std::collections::BTreeSet::new();
    let mut add = |name: &str| {
        let name = name.trim();
        if !name.is_empty() {
            names.insert(name.to_string());
        }
    };

    parse_sections(config, |section| {
        if section == "default" {
            add("default");
        } else if let Some(rest) = section.strip_prefix("profile ") {
            add(rest);
        }
    });
    parse_sections(credentials, &mut add);

    names.into_iter().collect()
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
    fn detects_sso_token_errors() {
        // The three messages aws-config 1.8 actually emits.
        assert!(is_sso_token_error(
            "the SSO token has expired and cannot be refreshed"
        ));
        assert!(is_sso_token_error("failed to load the cached SSO token"));
        assert!(is_sso_token_error("invalid JSON in cached SSO token file"));
        // Wrapped in a DisplayErrorContext-style cause chain.
        assert!(is_sso_token_error(
            "dispatch failure: other: failed to load the cached SSO token: token file does not exist"
        ));
        assert!(!is_sso_token_error(
            "AccessDenied: not authorized to perform ec2:DescribeInstances"
        ));
    }

    #[test]
    fn sso_hint_includes_profile() {
        assert_eq!(
            sso_login_hint(""),
            "AWS SSO session expired or missing — run: aws sso login"
        );
        assert_eq!(
            sso_login_hint("prod"),
            "AWS SSO session expired or missing — run: aws sso login --profile prod"
        );
    }

    #[test]
    fn missing_files_yield_empty() {
        let dir = tempfile::tempdir().unwrap();
        let got = profiles_from(&dir.path().join("nope"), &dir.path().join("nada"));
        assert!(got.is_empty());
    }
}
