use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

pub fn ingest(repo: &Path, files: &[String], note: &str) -> Result<usize> {
    if !repo.exists() {
        return Err(anyhow!(
            "repo_path does not exist: {}",
            repo.display()
        ));
    }
    let inbox = repo.join("inbox");
    fs::create_dir_all(&inbox).context("creating inbox/")?;

    let mut count = 0usize;
    for raw in files {
        let src = PathBuf::from(raw);
        if !src.is_file() {
            continue;
        }
        let dest = unique_path(&inbox, &src);
        fs::copy(&src, &dest)
            .with_context(|| format!("copy {} -> {}", src.display(), dest.display()))?;

        if !note.trim().is_empty() {
            let note_path = {
                let mut p = dest.clone();
                let fname = p
                    .file_name()
                    .map(|n| n.to_owned())
                    .unwrap_or_default();
                p.set_file_name(format!("{}.note.txt", fname.to_string_lossy()));
                p
            };
            fs::write(&note_path, note).context("writing note")?;
        }
        count += 1;
    }
    Ok(count)
}

fn unique_path(dir: &Path, src: &Path) -> PathBuf {
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".into());
    let target = dir.join(&name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), Some(e.to_string())),
        None => (name.clone(), None),
    };
    let mut i = 1usize;
    loop {
        let candidate = match &ext {
            Some(e) => dir.join(format!("{}_{}.{}", stem, i, e)),
            None => dir.join(format!("{}_{}", stem, i)),
        };
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}
