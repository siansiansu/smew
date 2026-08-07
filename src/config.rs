//! Loads smew settings from ~/.config/smew/config.yaml.
//!
//! The file is optional — a missing file yields zero-value defaults. CLI flags
//! take precedence over config values (resolved in main).

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

/// Auto-refresh interval when refresh_interval is unset.
const DEFAULT_REFRESH: Duration = Duration::from_secs(30);

/// Mirrors config.yaml. `refresh_interval` is a duration string such as
/// "30s" or "2m"; empty / "0" disables auto-refresh. Unknown keys are
/// rejected so typos fail loudly instead of silently keeping the default.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub default_profile: String,
    pub default_region: String,
    pub refresh_interval: String,
    /// Multiplexer prefix key inside a split session (key string, e.g.
    /// "ctrl+b", "ctrl+a", "ctrl+ "). Default ctrl+b.
    pub session_leader: String,
    /// SSM Parameter Store name holding the latest published version, for the
    /// update check. Default /smew/latest-version.
    pub version_param: String,
    /// Turns the update check off entirely.
    pub disable_version_check: bool,
    /// Mouse support (wheel scroll, click to select/focus). Default on;
    /// set false to keep the terminal's native click-drag text selection
    /// (with mouse on, hold Shift while dragging instead).
    pub mouse: Option<bool>,
    /// Color theme: a built-in name (default, dracula, gruvbox-dark, nord)
    /// or a file in ~/.config/smew/skins/<name>.yaml. Empty means default.
    pub skin: String,
    /// Shows %CPU/%MEM columns fed from CloudWatch (GetMetricStatistics /
    /// ListMetrics — free-tier-eligible APIs, polled every 5 minutes).
    /// Default false: the columns are hidden and smew never calls
    /// CloudWatch at all.
    pub metrics: bool,
}

impl Config {
    /// The SSM parameter to read for the update check, or "" when disabled.
    pub fn version_param_name(&self) -> &str {
        if self.disable_version_check {
            return "";
        }
        if self.version_param.is_empty() {
            return "/smew/latest-version";
        }
        &self.version_param
    }

    /// Whether mouse support is enabled (default true).
    pub fn mouse_enabled(&self) -> bool {
        self.mouse.unwrap_or(true)
    }

    /// The configured session leader key, defaulting to "ctrl+b".
    pub fn leader(&self) -> &str {
        if self.session_leader.is_empty() {
            "ctrl+b"
        } else {
            &self.session_leader
        }
    }

    /// Parses refresh_interval. Unset defaults to 30s (auto-refresh on); an
    /// explicit "0" (or a negative duration) disables it; an invalid value
    /// falls back to the default. Duration::ZERO means "off".
    pub fn refresh_interval(&self) -> Duration {
        let s = self.refresh_interval.trim();
        if s.is_empty() {
            return DEFAULT_REFRESH;
        }
        if s == "0" {
            return Duration::ZERO;
        }
        if let Some(rest) = s.strip_prefix('-') {
            // A parseable negative duration clamps to 0 (off).
            return if parse_duration(rest).is_some() {
                Duration::ZERO
            } else {
                DEFAULT_REFRESH
            };
        }
        parse_duration(s).unwrap_or(DEFAULT_REFRESH)
    }
}

/// Parses the duration syntax the config documents: a sequence of decimal
/// numbers (fractions allowed) with a unit suffix, e.g. "30s", "1h30m",
/// "1.5h". humantime rejects fractional units, which would silently turn a
/// configured "1.5h" into the 30s default.
fn parse_duration(s: &str) -> Option<Duration> {
    let mut total = 0f64;
    let mut rest = s.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let num_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let (num, r) = rest.split_at(num_end);
        let value: f64 = num.parse().ok()?;
        let unit_end = r
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(r.len());
        let (unit, r2) = r.split_at(unit_end);
        let mult = match unit {
            "ns" => 1e-9,
            "us" | "µs" | "μs" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            _ => return None,
        };
        total += value * mult;
        rest = r2;
    }
    if !total.is_finite() || total > 1e15 {
        return None;
    }
    Some(Duration::from_secs_f64(total))
}

/// The resolved config file location (honors XDG_CONFIG_HOME).
fn path() -> Option<PathBuf> {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        return Some(PathBuf::from(x).join("smew").join("config.yaml"));
    }
    let home = dirs::home_dir()?;
    Some(home.join(".config").join("smew").join("config.yaml"))
}

/// Reads the config file. A missing file is not an error.
pub fn load() -> Result<Config, String> {
    let Some(p) = path() else {
        return Ok(Config::default());
    };
    let data = match std::fs::read_to_string(&p) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    serde_yaml_ng::from_str(&data).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(yaml: &str) -> Config {
        serde_yaml_ng::from_str(yaml).unwrap()
    }

    #[test]
    fn interval_defaults_and_overrides() {
        assert_eq!(
            Config::default().refresh_interval(),
            Duration::from_secs(30)
        );
        assert_eq!(
            cfg("refresh_interval: \"45s\"").refresh_interval(),
            Duration::from_secs(45)
        );
        assert_eq!(
            cfg("refresh_interval: \"2m\"").refresh_interval(),
            Duration::from_secs(120)
        );
        assert_eq!(
            cfg("refresh_interval: \"0\"").refresh_interval(),
            Duration::ZERO
        );
        assert_eq!(
            cfg("refresh_interval: \"-5s\"").refresh_interval(),
            Duration::ZERO
        );
        assert_eq!(
            cfg("refresh_interval: \"bogus\"").refresh_interval(),
            Duration::from_secs(30)
        );
        // Go-style compound and fractional durations must be honored.
        assert_eq!(
            cfg("refresh_interval: \"1h30m\"").refresh_interval(),
            Duration::from_secs(5400)
        );
        assert_eq!(
            cfg("refresh_interval: \"1.5h\"").refresh_interval(),
            Duration::from_secs(5400)
        );
        assert_eq!(
            cfg("refresh_interval: \"0s\"").refresh_interval(),
            Duration::ZERO
        );
        assert_eq!(
            cfg("refresh_interval: \"500ms\"").refresh_interval(),
            Duration::from_millis(500)
        );
    }

    #[test]
    fn mouse_default_on() {
        assert!(Config::default().mouse_enabled());
        assert!(cfg("mouse: true").mouse_enabled());
        assert!(!cfg("mouse: false").mouse_enabled());
    }

    #[test]
    fn leader_default() {
        assert_eq!(Config::default().leader(), "ctrl+b");
        assert_eq!(cfg("session_leader: \"ctrl+a\"").leader(), "ctrl+a");
    }

    #[test]
    fn version_param_logic() {
        assert_eq!(
            Config::default().version_param_name(),
            "/smew/latest-version"
        );
        assert_eq!(cfg("version_param: \"/x/y\"").version_param_name(), "/x/y");
        assert_eq!(cfg("disable_version_check: true").version_param_name(), "");
    }

    #[test]
    fn parses_example_config() {
        // The shipped example must always match the struct; with
        // deny_unknown_fields this catches any drift between the two.
        let c = cfg(include_str!("../config.example.yaml"));
        assert_eq!(c.default_region, "ap-northeast-1");
        assert_eq!(c.refresh_interval(), Duration::from_secs(30));
    }

    #[test]
    fn unknown_key_is_an_error() {
        let err = serde_yaml_ng::from_str::<Config>("defaut_region: x")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("defaut_region"),
            "error should name the key: {err}"
        );
    }
}
