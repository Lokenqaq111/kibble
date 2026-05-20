use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

pub fn is_git_repo(repo: &Path) -> bool {
    repo.join(".git").exists()
}

fn run(repo: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| anyhow!("failed to spawn git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "git {} failed: {}{}",
            args.join(" "),
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" | {}", stdout.trim())
            }
        ));
    }
    Ok(())
}

pub fn commit_and_push(repo: &Path, count: usize) -> Result<()> {
    run(repo, &["add", "-A"])?;

    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo)
        .output()
        .map_err(|e| anyhow!("failed to spawn git status: {e}"))?;
    if status.stdout.is_empty() {
        return Ok(());
    }

    let message = format!("kibble: add {} item(s)", count);
    run(repo, &["commit", "-m", &message])?;
    run(repo, &["push"])?;
    Ok(())
}
