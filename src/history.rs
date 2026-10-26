use std::fs;
use std::path::{Path, PathBuf};
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

pub fn default_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/formatui/history.json")
}

fn load(path: &Path) -> Journal {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn store(path: &Path, j: &Journal) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(j)?)?;
    Ok(())
}

pub fn record(journal: &Path, renames: Vec<(PathBuf, PathBuf)>) -> Result<()> {
    if renames.is_empty() {
        return Ok(());
    }
    let mut j = load(journal);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    j.batches.push(Batch { time, renames });
    store(journal, &j)
}

/// Reverts the most recent batch. Returns (restored count, skipped reasons).
pub fn undo_last(journal: &Path) -> Result<Option<(usize, Vec<String>)>> {
    let mut j = load(journal);
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
    store(journal, &j)?;
    Ok(Some((restored, skipped)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_restores_the_last_batch_only() {
        let dir =
            std::env::temp_dir().join(format!("formatui-test-{}-history", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let journal = dir.join("history.json");
        let (a, b, c) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.txt"));
        fs::write(&b, "").unwrap();
        fs::write(&c, "").unwrap();
        record(&journal, vec![(dir.join("old-b.txt"), b.clone())]).unwrap();
        record(&journal, vec![(a.clone(), c.clone())]).unwrap();

        let (restored, skipped) = undo_last(&journal).unwrap().unwrap();
        assert_eq!((restored, skipped.len()), (1, 0));
        assert!(a.exists() && !c.exists() && b.exists());
        assert!(undo_last(&journal).unwrap().is_some());
        assert!(undo_last(&journal).unwrap().is_none());
    }
}
