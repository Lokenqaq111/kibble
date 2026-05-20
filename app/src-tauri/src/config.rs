use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const TEMPLATE: &str = r#"# Kibble configuration
# Path to a local clone of your data repository.
# Kibble does NOT clone or init this for you.
repo_path = ""

# Optional: meal type time thresholds (24h format).
# Kibble does not use these directly — the downstream Skill does.
[meal_times]
breakfast = ["06:00", "09:59"]
lunch     = ["10:00", "13:59"]
snack     = ["14:00", "16:59"]
dinner    = ["17:00", "19:59"]
# 20:00 - 05:59 falls into late_night automatically

# UI preferences
[ui]
chew_animation_speed = "normal"  # fast / normal / slow
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub repo_path: String,
    #[serde(default)]
    pub meal_times: toml::value::Table,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_speed")]
    pub chew_animation_speed: String,
}

fn default_speed() -> String {
    "normal".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_path: String::new(),
            meal_times: toml::value::Table::new(),
            ui: UiConfig::default(),
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not resolve OS config dir")?;
    Ok(base.join("kibble").join("config.toml"))
}

pub fn load_or_init() -> Result<(Config, PathBuf)> {
    let path = config_path()?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&path, TEMPLATE).context("writing config template")?;
        eprintln!("kibble: created config template at {}", path.display());
        return Ok((Config::default(), path));
    }
    let text = fs::read_to_string(&path).context("reading config")?;
    match toml::from_str::<Config>(&text) {
        Ok(cfg) => Ok((cfg, path)),
        Err(e) => {
            eprintln!(
                "kibble: config at {} is invalid ({}), using defaults",
                path.display(),
                e
            );
            Ok((Config::default(), path))
        }
    }
}

pub fn write_repo_path(repo_path: &str) -> Result<()> {
    let path = config_path()?;
    let text = if path.exists() {
        fs::read_to_string(&path).unwrap_or_else(|_| TEMPLATE.to_string())
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        TEMPLATE.to_string()
    };

    let mut replaced = false;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !replaced && trimmed.starts_with("repo_path") {
            out.push_str(&format!("repo_path = \"{}\"\n", repo_path.replace('"', "\\\"")));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        out.push_str(&format!("\nrepo_path = \"{}\"\n", repo_path.replace('"', "\\\"")));
    }

    fs::write(&path, out).context("writing config")?;
    Ok(())
}
