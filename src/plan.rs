use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use walkdir::WalkDir;

use crate::engine::Compiled;

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    NoMatch,
    Unchanged,
    Ready,
    Conflict(String),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub target: Option<PathBuf>,
    /// Index of the rule that matched.
    pub rule: Option<usize>,
    pub status: Status,
}

/// Gathers files from paths. Directories are listed (recursively with `recursive`); hidden files are skipped.
pub fn collect(paths: &[PathBuf], recursive: bool, exts: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            let depth = if recursive { usize::MAX } else { 1 };
            for e in WalkDir::new(p)
                .min_depth(1)
                .max_depth(depth)
                .into_iter()
                .flatten()
            {
                if e.file_type().is_file() {
                    files.push(e.into_path());
                }
            }
        } else {
            files.push(p.clone());
        }
    }
    files.retain(|f| {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = f
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        !name.starts_with('.') && (exts.is_empty() || exts.contains(&ext))
    });
    files.sort();
    files
}

/// First matching rule wins.
pub fn plan(files: &[PathBuf], rules: &[Result<Compiled, String>]) -> Vec<Entry> {
    let mut seen: HashSet<String> = HashSet::new();
    files
        .iter()
        .map(|path| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let hit = rules.iter().enumerate().find_map(|(i, r)| {
                r.as_ref()
                    .ok()
                    .and_then(|c| c.apply(name))
                    .map(|new| (i, new))
            });
            let Some((rule, new)) = hit else {
                return Entry {
                    path: path.clone(),
                    target: None,
                    rule: None,
                    status: Status::NoMatch,
                };
            };
            let target = path.with_file_name(&new);
            let status = if new == name {
                Status::Unchanged
            } else if !seen.insert(target.to_string_lossy().to_lowercase()) {
                Status::Conflict("another file maps to the same name".into())
            } else if target.exists() && !same_ignoring_case(path, &target) {
                Status::Conflict("target already exists".into())
            } else {
                Status::Ready
            };
            Entry {
                path: path.clone(),
                target: Some(target),
                rule: Some(rule),
                status,
            }
        })
        .collect()
}

fn same_ignoring_case(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// Renames every `Ready` entry. Returns completed (from, to) pairs and per-file errors.
pub fn apply(entries: &[Entry]) -> (Vec<(PathBuf, PathBuf)>, Vec<String>) {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for e in entries.iter().filter(|e| e.status == Status::Ready) {
        let to = e.target.as_ref().expect("ready entries have a target");
        match fs::rename(&e.path, to) {
            Ok(()) => done.push((e.path.clone(), to.clone())),
            Err(err) => errors.push(format!("{}: {err}", e.path.display())),
        }
    }
    (done, errors)
}
