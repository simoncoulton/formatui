use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
struct Journal {
    batches: Vec<Batch>,
}

#[derive(Serialize, Deserialize)]
struct Batch {
    time: u64,
    renames: Vec<(PathBuf, PathBuf)>,
}

fn journal_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/formatui/history.json")
}

fn load() -> Journal {
    fs::read_to_string(journal_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn store(j: &Journal) -> Result<()> {
    let path = journal_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(j)?)?;
    Ok(())
}

pub fn record(renames: Vec<(PathBuf, PathBuf)>) -> Result<()> {
    if renames.is_empty() {
        return Ok(());
    }
    let mut j = load();
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    j.batches.push(Batch { time, renames });
    store(&j)
}

/// Reverts the most recent batch. Returns (restored count, skipped reasons).
pub fn undo_last() -> Result<Option<(usize, Vec<String>)>> {
    let mut j = load();
    let Some(batch) = j.batches.pop() else {
        return Ok(None);
    };
    let mut restored = 0;
    let mut skipped = Vec::new();
    for (from, to) in batch.renames.iter().rev() {
        if !to.exists() {
            skipped.push(format!("{} no longer exists", to.display()));
        } else if from.exists() {
            skipped.push(format!("{} already exists", from.display()));
        } else if let Err(e) = fs::rename(to, from) {
            skipped.push(format!("{}: {e}", to.display()));
        } else {
            restored += 1;
        }
    }
    store(&j)?;
    Ok(Some((restored, skipped)))
}
