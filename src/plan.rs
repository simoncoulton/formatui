use std::collections::{BTreeSet, HashSet};
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

#[derive(Clone, Debug, PartialEq)]
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

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Lower-cased paths of everything beside `files`, so planning needs no filesystem access.
pub fn existing(files: &[PathBuf]) -> HashSet<String> {
    let dirs: BTreeSet<&Path> = files.iter().filter_map(|f| f.parent()).collect();
    let mut found = HashSet::new();
    for dir in dirs {
        let real = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        for entry in fs::read_dir(real).into_iter().flatten().flatten() {
            found.insert(key(&dir.join(entry.file_name())));
        }
    }
    found
}

/// First matching rule wins. `existing` is the result of [`existing`].
pub fn plan(
    files: &[PathBuf],
    rules: &[Result<Compiled, String>],
    existing: &HashSet<String>,
) -> Vec<Entry> {
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
            } else if !seen.insert(key(&target)) {
                Status::Conflict("another file maps to the same name".into())
            } else if existing.contains(&key(&target)) && !same_ignoring_case(path, &target) {
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
    key(a) == key(b)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn rules() -> Vec<Result<Compiled, String>> {
        Config::default()
            .rules
            .iter()
            .map(Compiled::compile)
            .collect()
    }

    fn names(entries: &[Entry]) -> Vec<(Status, Option<String>)> {
        entries
            .iter()
            .map(|e| {
                (
                    e.status.clone(),
                    e.target
                        .as_ref()
                        .map(|t| t.file_name().unwrap().to_string_lossy().into()),
                )
            })
            .collect()
    }

    #[test]
    fn conflicts_with_existing_files_and_each_other() {
        let files: Vec<PathBuf> = [
            "d/Show.S01E01.mkv",
            "d/Show.S01E01.720p.mkv",
            "d/Other.S02E03.mkv",
            "d/notes.txt",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        let existing: HashSet<String> = ["d/other - s02e03.mkv".to_string()].into();
        let got = names(&plan(&files, &rules(), &existing));
        assert_eq!(got[0], (Status::Ready, Some("Show - S01E01.mkv".into())));
        assert!(matches!(got[1].0, Status::Conflict(_)));
        assert!(matches!(got[2].0, Status::Conflict(_)));
        assert_eq!(got[3], (Status::NoMatch, None));
    }

    #[test]
    fn a_case_only_rename_is_not_a_conflict() {
        let files = vec![PathBuf::from("d/another series - S01E01.mkv")];
        let existing: HashSet<String> = ["d/another series - s01e01.mkv".to_string()].into();
        let rules: Vec<_> = vec![Compiled::compile(&crate::config::Rule {
            name: "up".into(),
            pattern: "(?P<t>.+)".into(),
            template: "{t|title}".into(),
            steps: vec![],
            extensions: vec![],
        })];
        let got = plan(&files, &rules, &existing);
        assert_eq!(got[0].status, Status::Ready);
    }
}
