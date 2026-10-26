use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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

fn default_true() -> bool {
    true
}

fn default_extensions() -> Vec<String> {
    ["mkv", "mp4", "avi"].map(String::from).to_vec()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    /// Whether the TUI lists files that no rule matches.
    #[serde(default = "default_true")]
    pub show_unmatched: bool,
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

const DEFAULT_CONFIG: &str = r#"# formatui rules. The first rule whose pattern matches a file wins.
#
# extensions: file types the rule applies to (default: mkv, mp4, avi; [] = any)
# steps:      strip_brackets, dots_to_spaces, underscores_to_spaces, collapse_spaces
#             (applied to the file name without extension, before matching)
# template:   {group} or {group|filter|filter}; filters: upper, lower, title, trim, pad2

# show_unmatched: list files that no rule matches in the TUI (toggle with t)
show_unmatched: true

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
        .join(".config/formatui/config.yaml")
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
    serde_yaml_ng::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(path: &Path, config: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_yaml_ng::to_string(config)?)?;
    Ok(())
}
