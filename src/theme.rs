//! User-selectable color themes (skins), following the k9s model: a skin is
//! a YAML file mapping named UI roles to colors, selected by name via the
//! `skin` key in config.yaml.
//!
//! Resolution order for a skin name: built-in (default, dracula,
//! gruvbox-dark, nord) → ~/.config/smew/skins/<name>.yaml. A skin file may
//! set any subset of roles; unset roles keep the default theme's value.
//! The embedded terminal content of session panes is never themed — it keeps
//! whatever colors the remote shell emits.

use std::path::PathBuf;
use std::sync::OnceLock;

use ratatui::style::Color;
use serde::Deserialize;

/// Every themable UI role. Field names double as the YAML keys in skin files.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Section headers, table header row, region value.
    pub cyan: Color,
    /// Running/healthy states, sync timestamp.
    pub green: Color,
    /// Down states, errors, broadcast borders/badge.
    pub red: Color,
    /// Transitional states, key hints, notices, update badge.
    pub orange: Color,
    /// Labels and dim/secondary text.
    pub gray: Color,
    /// Plain field values.
    pub value: Color,
    /// Identifiers (instance ids, VPCs, SSH snippets).
    pub accent: Color,
    /// The focused pane border and picker cursor.
    pub pink: Color,
    /// Selected table row text.
    pub sel_fg: Color,
    /// Selected table row background (also the detail name chip and the
    /// PREFIX badge background).
    pub sel_bg: Color,
    /// The profile-picker title text / background.
    pub title_fg: Color,
    pub title_bg: Color,
    /// Text on state chips (detail header).
    pub chip_fg: Color,
    /// State chip backgrounds by lifecycle class.
    pub chip_running_bg: Color,
    pub chip_down_bg: Color,
    pub chip_other_bg: Color,
    /// Pane border / badge while scrollback is active.
    pub border_scroll: Color,
    /// Unfocused pane borders.
    pub border_unfocused: Color,
    /// Light text on dark badge backgrounds (session header badges).
    pub badge_fg: Color,
    /// ZOOM and GROUP/FOCUS badge backgrounds.
    pub badge_zoom_bg: Color,
    pub badge_info_bg: Color,
    /// Dark text on bright backgrounds (orange notices, the update badge).
    pub notice_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            cyan: Color::Indexed(39),
            green: Color::Indexed(42),
            red: Color::Indexed(203),
            orange: Color::Indexed(214),
            gray: Color::Indexed(245),
            value: Color::Indexed(252),
            accent: Color::Indexed(180),
            pink: Color::Indexed(205),
            sel_fg: Color::Indexed(229),
            sel_bg: Color::Indexed(57),
            title_fg: Color::Indexed(230),
            title_bg: Color::Indexed(62),
            chip_fg: Color::Indexed(231),
            chip_running_bg: Color::Indexed(22),
            chip_down_bg: Color::Indexed(52),
            chip_other_bg: Color::Indexed(58),
            border_scroll: Color::Indexed(220),
            border_unfocused: Color::Indexed(240),
            badge_fg: Color::Indexed(15),
            badge_zoom_bg: Color::Indexed(36),
            badge_info_bg: Color::Indexed(62),
            notice_fg: Color::Indexed(0),
        }
    }
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// Installs the theme for this process. Later calls are ignored (the first
/// install wins), so tests that never call this get the default.
pub fn init(t: Theme) {
    let _ = THEME.set(t);
}

/// The active theme (default until init is called).
pub fn current() -> &'static Theme {
    THEME.get_or_init(Theme::default)
}

/// Mirrors a skin YAML file: every role optional, unknown keys rejected so
/// typos fail loudly instead of silently keeping the default color.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SkinFile {
    cyan: Option<ColorDef>,
    green: Option<ColorDef>,
    red: Option<ColorDef>,
    orange: Option<ColorDef>,
    gray: Option<ColorDef>,
    value: Option<ColorDef>,
    accent: Option<ColorDef>,
    pink: Option<ColorDef>,
    sel_fg: Option<ColorDef>,
    sel_bg: Option<ColorDef>,
    title_fg: Option<ColorDef>,
    title_bg: Option<ColorDef>,
    chip_fg: Option<ColorDef>,
    chip_running_bg: Option<ColorDef>,
    chip_down_bg: Option<ColorDef>,
    chip_other_bg: Option<ColorDef>,
    border_scroll: Option<ColorDef>,
    border_unfocused: Option<ColorDef>,
    badge_fg: Option<ColorDef>,
    badge_zoom_bg: Option<ColorDef>,
    badge_info_bg: Option<ColorDef>,
    notice_fg: Option<ColorDef>,
}

/// A color value as written in YAML: "#rrggbb", a 0–255 index (bare number
/// or string), or "default" for the terminal's own color.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ColorDef {
    Num(u8),
    Str(String),
}

impl TryFrom<&ColorDef> for Color {
    type Error = String;

    fn try_from(def: &ColorDef) -> Result<Color, String> {
        let s = match def {
            ColorDef::Num(i) => return Ok(Color::Indexed(*i)),
            ColorDef::Str(s) => s.trim(),
        };
        if s.eq_ignore_ascii_case("default") {
            return Ok(Color::Reset);
        }
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6
                && let Ok(v) = u32::from_str_radix(hex, 16)
            {
                return Ok(Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
            }
            return Err(format!("invalid hex color {s:?} (want #rrggbb)"));
        }
        if let Ok(i) = s.parse::<u8>() {
            return Ok(Color::Indexed(i));
        }
        Err(format!(
            "invalid color {s:?} (want \"#rrggbb\", 0–255, or \"default\")"
        ))
    }
}

impl SkinFile {
    /// Overlays the set roles onto `base`.
    fn apply(self, mut base: Theme) -> Result<Theme, String> {
        macro_rules! set {
            ($($f:ident),*) => {
                $(if let Some(c) = self.$f {
                    base.$f =
                        Color::try_from(&c).map_err(|e| format!("{}: {e}", stringify!($f)))?;
                })*
            };
        }
        set!(
            cyan,
            green,
            red,
            orange,
            gray,
            value,
            accent,
            pink,
            sel_fg,
            sel_bg,
            title_fg,
            title_bg,
            chip_fg,
            chip_running_bg,
            chip_down_bg,
            chip_other_bg,
            border_scroll,
            border_unfocused,
            badge_fg,
            badge_zoom_bg,
            badge_info_bg,
            notice_fg
        );
        Ok(base)
    }
}

/// The built-in skins, embedded from skins/ in the repo (which doubles as
/// the set of copyable examples for user skins).
const BUILTINS: [(&str, &str); 3] = [
    ("dracula", include_str!("../skins/dracula.yaml")),
    ("gruvbox-dark", include_str!("../skins/gruvbox-dark.yaml")),
    ("nord", include_str!("../skins/nord.yaml")),
];

fn parse(yaml: &str) -> Result<Theme, String> {
    let f: SkinFile = serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())?;
    f.apply(Theme::default())
}

/// The user skin directory: ~/.config/smew/skins (honors XDG_CONFIG_HOME).
fn skins_dir() -> Option<PathBuf> {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        return Some(PathBuf::from(x).join("smew").join("skins"));
    }
    Some(dirs::home_dir()?.join(".config").join("smew").join("skins"))
}

/// Resolves a skin name to a Theme. "" and "default" mean the built-in
/// default; other names try the built-ins, then the user skin directory.
pub fn load(name: &str) -> Result<Theme, String> {
    if name.is_empty() || name == "default" {
        return Ok(Theme::default());
    }
    if let Some((_, yaml)) = BUILTINS.iter().find(|(n, _)| *n == name) {
        return parse(yaml).map_err(|e| format!("built-in skin {name}: {e}"));
    }
    let Some(path) = skins_dir().map(|d| d.join(format!("{name}.yaml"))) else {
        return Err(format!("skin {name:?}: cannot resolve home directory"));
    };
    let builtin_names: Vec<&str> = BUILTINS.iter().map(|(n, _)| *n).collect();
    let data = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "skin {name:?}: {}: {e} (built-ins: default, {})",
            path.display(),
            builtin_names.join(", ")
        )
    })?;
    parse(&data).map_err(|e| format!("skin {name:?}: {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_empty_and_default_names() {
        assert_eq!(load("").unwrap(), Theme::default());
        assert_eq!(load("default").unwrap(), Theme::default());
    }

    #[test]
    fn builtins_parse() {
        for (name, _) in BUILTINS {
            let t = load(name).unwrap_or_else(|e| panic!("skin {name}: {e}"));
            assert_ne!(t, Theme::default(), "skin {name} changed nothing");
        }
    }

    #[test]
    fn partial_skin_keeps_default_roles() {
        let t = parse("green: \"#00ff00\"\nsel_bg: 17\n").unwrap();
        assert_eq!(t.green, Color::Rgb(0, 255, 0));
        assert_eq!(t.sel_bg, Color::Indexed(17));
        assert_eq!(t.cyan, Theme::default().cyan);
    }

    #[test]
    fn color_forms() {
        assert_eq!(parse("cyan: 39\n").unwrap().cyan, Color::Indexed(39));
        assert_eq!(parse("cyan: \"39\"\n").unwrap().cyan, Color::Indexed(39));
        assert_eq!(parse("cyan: default\n").unwrap().cyan, Color::Reset);
        assert_eq!(
            parse("cyan: \"#8be9fd\"\n").unwrap().cyan,
            Color::Rgb(0x8b, 0xe9, 0xfd)
        );
    }

    #[test]
    fn rejects_bad_values_and_unknown_keys() {
        assert!(parse("cyan: \"#12345\"\n").is_err());
        assert!(parse("cyan: bogus\n").is_err());
        assert!(parse("cyan: 300\n").is_err());
        assert!(parse("not_a_role: 1\n").is_err());
    }

    #[test]
    fn missing_user_skin_errors_with_hint() {
        let e = load("no-such-skin-xyz").unwrap_err();
        assert!(e.contains("no-such-skin-xyz"), "{e}");
        assert!(e.contains("built-ins"), "{e}");
    }
}
