#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod git;
mod ingest;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Manager;

struct AppState {
    cfg: Mutex<config::Config>,
}

fn validate_repo(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is empty".into());
    }
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("git@")
        || trimmed.contains("://")
    {
        return Err("looks like a URL — clone it first, then paste the local path".into());
    }
    let expanded = if let Some(rest) = trimmed.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or_else(|| "could not resolve home dir".to_string())?
            .join(rest)
    } else {
        PathBuf::from(trimmed)
    };
    if !expanded.exists() {
        return Err(format!("path does not exist: {}", expanded.display()));
    }
    if !git::is_git_repo(&expanded) {
        return Err(format!("not a git repo: {}", expanded.display()));
    }
    Ok(expanded)
}

#[tauri::command]
fn startup_check(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let cfg = state.cfg.lock().map_err(|e| e.to_string())?;
    validate_repo(&cfg.repo_path).map(|_| ())
}

#[tauri::command]
fn set_repo_path(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let resolved = validate_repo(&path)?;
    let as_str = resolved.to_string_lossy().to_string();
    config::write_repo_path(&as_str).map_err(|e| format!("{e:#}"))?;
    let mut cfg = state.cfg.lock().map_err(|e| e.to_string())?;
    cfg.repo_path = as_str;
    Ok(())
}

#[tauri::command]
async fn ingest(
    state: tauri::State<'_, AppState>,
    files: Vec<String>,
    note: String,
) -> Result<usize, String> {
    let repo_path = {
        let cfg = state.cfg.lock().map_err(|e| e.to_string())?;
        if cfg.repo_path.is_empty() {
            return Err("repo_path is empty".into());
        }
        PathBuf::from(&cfg.repo_path)
    };

    let count = tokio::task::spawn_blocking(move || -> Result<usize, String> {
        let n = ingest::ingest(&repo_path, &files, &note).map_err(|e| format!("{e:#}"))?;
        if n == 0 {
            return Ok(0);
        }
        git::commit_and_push(&repo_path, n).map_err(|e| format!("{e:#}"))?;
        Ok(n)
    })
    .await
    .map_err(|e| format!("join error: {e}"))??;

    Ok(count)
}

fn main() {
    let (cfg, path) = match config::load_or_init() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("kibble: failed to load config: {e:#}");
            (config::Config::default(), Default::default())
        }
    };
    eprintln!("kibble: config = {}", path.display());

    if cfg.repo_path.is_empty() {
        eprintln!("kibble: repo_path is unset — set it in the app");
    } else if let Err(e) = validate_repo(&cfg.repo_path) {
        eprintln!("kibble: repo invalid ({e})");
    }

    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                cfg: Mutex::new(cfg),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_check,
            set_repo_path,
            ingest
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
