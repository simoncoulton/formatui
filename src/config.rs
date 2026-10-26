use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

use crate::theme::ThemeSpec;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub name: String,
    /// Regex with named capture groups, matched against the (pre-processed) file stem.
    pub pattern: String,
    /// Output template, e.g. `{title} - S{season|pad2}E{episode|pad2}`.
    pub template: String,
    /// Cleanup steps applied to the stem before matching.
    #[serde(default)]
    pub steps: Vec<String>,
    /// File extensions the rule applies to. Empty means any.
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Whether the TUI lists files that no rule matches.
    #[serde(default = "default_true")]
    pub show_unmatched: bool,
    /// Name of a theme: a built-in, a file in `themes/` beside this config, or one under `themes:`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Let the terminal's own background show instead of the theme's page colour.
    #[serde(default = "default_true")]
    pub transparent_background: bool,
    /// Extra or tweaked themes, layered over the files in `themes/`.
    #[serde(default)]
    pub themes: BTreeMap<String, ThemeSpec>,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "default".to_string()
}

fn default_extensions() -> Vec<String> {
    ["mkv", "mp4", "avi"].map(String::from).to_vec()
}

impl Default for Config {
    fn default() -> Self {
        let steps = ["strip_brackets", "dots_to_spaces", "underscores_to_spaces"].map(String::from);
        Self {
            show_unmatched: true,
            theme: default_theme(),
            transparent_background: true,
            themes: BTreeMap::new(),
            rules: vec![
                Rule {
                    name: "tv".into(),
                    pattern: r"(?i)^(?P<title>.+?)(?: (?:19|20)\d{2})?(?: -)? S(?P<season>\d+)E(?P<episode>\d+)".into(),
                    template: "{title} - S{season|pad2}E{episode|pad2}".into(),
                    steps: steps.to_vec(),
                    extensions: default_extensions(),
                },
                Rule {
                    name: "movie".into(),
                    pattern: r"^(?P<title>.+?) (?P<year>(?:19|20)\d{2})(?: |$)".into(),
                    template: "{title} ({year})".into(),
                    steps: steps.to_vec(),
                    extensions: default_extensions(),
                },
            ],
        }
    }
}

const DEFAULT_CONFIG: &str = r#"# formatui config. Edit it here (press E in the TUI) or from the rules modal (r).

# List files that no rule matches in the TUI (toggle with t).
show_unmatched: true

# Colour theme (pick one with T in the TUI). Built-ins: default, tokyo-night,
# catppuccin-frappe, catppuccin-mocha, catppuccin-macchiato, catppuccin-latte,
# dracula, one-dark, gruvbox-dark, nord, solarized-dark, rose-pine, monokai,
# github-dark, everforest, kanagawa. Every built-in also has an editable file in
# the themes/ folder next to this one, and any other .yml file there is a new theme.
theme: default

# Use the terminal's own background instead of the theme's page colour.
transparent_background: true

# Optional: define or tweak themes here. Unset colours come from `base`.
# themes:
#   mine:
#     base: dracula
#     accent: '#ff8800'

# Rules are tried in order and the first whose pattern matches a file wins.
#
# extensions: file types the rule applies to (default: mkv, mp4, avi; [] = any)
# steps:      strip_brackets, dots_to_spaces, underscores_to_spaces, collapse_spaces
#             (applied to the file name without extension, before matching)
# template:   {group} or {group|filter|filter}; filters: upper, lower, title, trim, pad2
rule:
  - name: tv
    extensions: [mkv, mp4, avi]
    steps: [strip_brackets, dots_to_spaces, underscores_to_spaces]
    pattern: '(?i)^(?P<title>.+?)(?: (?:19|20)\d{2})?(?: -)? S(?P<season>\d+)E(?P<episode>\d+)'
    template: '{title} - S{season|pad2}E{episode|pad2}'

  - name: movie
    extensions: [mkv, mp4, avi]
    steps: [strip_brackets, dots_to_spaces, underscores_to_spaces]
    pattern: '^(?P<title>.+?) (?P<year>(?:19|20)\d{2})(?: |$)'
    template: '{title} ({year})'
"#;

pub fn default_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/formatui/config.yml")
}

/// Loads the config, writing the default one first if the file doesn't exist.
pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, DEFAULT_CONFIG)
            .with_context(|| format!("writing default config to {}", path.display()))?;
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(path, &text)
}

/// Parse errors name the file and carry serde's line and column.
fn parse(path: &Path, text: &str) -> Result<Config> {
    serde_yaml_ng::from_str(text).map_err(|e| anyhow!("{}: {e}", path.display()))
}

/// Replaces one top-level key, keeping every other line and comment.
pub fn set_value<T: Serialize>(path: &Path, key: &str, value: &T) -> Result<()> {
    replace_key(path, key, serde_yaml_ng::to_value(value)?)
}

/// Replaces the `rule:` block, keeping everything else in the file.
pub fn set_rules(path: &Path, rules: &[Rule]) -> Result<()> {
    replace_key(path, "rule", serde_yaml_ng::to_value(rules)?)
}

fn replace_key(path: &Path, key: &str, value: Value) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    // Refuse to touch a file that is already broken: the error blames the file, not the value.
    parse(path, &text)?;
    let before: Value = serde_yaml_ng::from_str(&text)?;

    let mut entry = serde_yaml_ng::Mapping::new();
    entry.insert(Value::String(key.to_string()), value.clone());
    let block = serde_yaml_ng::to_string(&Value::Mapping(entry))?;
    let new_text = splice(&text, key, &block);

    // Refuse to write anything that would not read back as exactly this change.
    let after: Value = serde_yaml_ng::from_str(&new_text)
        .with_context(|| format!("updating '{key}' would produce invalid YAML"))?;
    let (Some(after_map), before_map) = (
        after.as_mapping(),
        before.as_mapping().cloned().unwrap_or_default(),
    ) else {
        bail!("updating '{key}' would produce an unexpected document");
    };
    let others_same = before_map
        .iter()
        .filter(|(k, _)| k.as_str() != Some(key))
        .all(|(k, v)| after_map.get(k) == Some(v));
    let expected_len = before_map.len() + usize::from(!before_map.contains_key(key));
    if after_map.get(key) != Some(&value) || !others_same || after_map.len() != expected_len {
        bail!("updating '{key}' would change other settings; not writing");
    }
    parse(path, &new_text)?;
    write_atomic(path, &new_text)
}

/// Writes beside the target and renames over it, so a crash never leaves half a file.
pub(crate) fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

/// Where theme files live: a `themes` folder beside the config file.
pub fn themes_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("themes")
}

fn is_top_level_key(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Swaps the block that starts at top-level `key:` for `block`, or appends it.
fn splice(text: &str, key: &str, block: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| {
        l.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
    });
    let Some(start) = start else {
        let mut out = text.trim_end_matches('\n').to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(block);
        return out;
    };
    let mut end = lines.len();
    if let Some(i) = lines
        .iter()
        .skip(start + 1)
        .position(|l| is_top_level_key(l))
    {
        end = start + 1 + i;
    }
    // Blank and comment lines just before the next key belong to that key.
    let mut keep_from = end;
    while keep_from > start + 1
        && (lines[keep_from - 1].is_empty() || lines[keep_from - 1].starts_with('#'))
    {
        keep_from -= 1;
    }
    let mut out: Vec<&str> = lines[..start].to_vec();
    out.extend(block.lines());
    out.extend(&lines[keep_from..]);
    out.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("formatui-test-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yml");
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn default_template_equals_defaults() {
        let parsed: Config = serde_yaml_ng::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn unknown_keys_are_errors_with_the_path() {
        let path = temp_file("unknown", "show_unmatchd: true\n");
        let err = load(&path).unwrap_err().to_string();
        assert!(
            err.contains("config.yml") && err.contains("show_unmatchd"),
            "{err}"
        );
    }

    #[test]
    fn set_value_keeps_comments_and_other_keys() {
        let path = temp_file("value", DEFAULT_CONFIG);
        set_value(&path, "theme", &"nord").unwrap();
        set_value(&path, "show_unmatched", &false).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Colour theme") && text.contains("# Rules are tried in order"));
        let cfg = load(&path).unwrap();
        assert_eq!((cfg.theme.as_str(), cfg.show_unmatched), ("nord", false));
        assert_eq!(cfg.rules, Config::default().rules);
    }

    #[test]
    fn set_rules_replaces_only_the_rule_block() {
        let path = temp_file("rules", &format!("{DEFAULT_CONFIG}\n# trailing note\n"));
        let mut rules = Config::default().rules;
        rules.remove(1);
        rules[0].name = "show".into();
        set_rules(&path, &rules).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Rules are tried in order") && text.contains("# trailing note"));
        assert_eq!(load(&path).unwrap().rules, rules);
        assert_eq!(text.matches("name:").count(), 1);
    }

    #[test]
    fn appends_a_missing_key_and_refuses_broken_files() {
        let path = temp_file("append", "rule: []\n");
        set_value(&path, "theme", &"dracula").unwrap();
        assert_eq!(load(&path).unwrap().theme, "dracula");

        let broken = temp_file("broken", "theme: [unclosed\n");
        let err = set_value(&broken, "theme", &"nord")
            .unwrap_err()
            .to_string();
        assert!(err.contains("config.yml"), "{err}");
        assert_eq!(fs::read_to_string(&broken).unwrap(), "theme: [unclosed\n");
    }
}
