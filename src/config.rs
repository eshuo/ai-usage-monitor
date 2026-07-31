/**
 * 配置管理 — 持久化用户配置到 %APPDATA%/ai-usage-monitor/config.json
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub creds: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default = "default_true")]
    pub auto_refresh: bool,
    #[serde(default = "default_interval")]
    pub refresh_interval: u64,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_true() -> bool { true }
fn default_interval() -> u64 { 60 }
fn default_theme() -> String { "dark".into() }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            providers: vec![],
            auto_refresh: true,
            refresh_interval: 60,
            minimize_to_tray: true,
            start_with_windows: false,
            theme: "dark".into(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("ai-usage-monitor");
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir.join("config.json")
}

pub fn load() -> AppConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save(config: &AppConfig) {
    let path = config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = fs::write(&path, json);
    }
}

pub fn get() -> AppConfig {
    load()
}

pub fn update(partial: serde_json::Value) -> AppConfig {
    let mut current = load();
    if let serde_json::Value::Object(map) = partial {
        let mut merged = serde_json::to_value(&current).unwrap_or_default();
        if let serde_json::Value::Object(merged_map) = &mut merged {
            for (k, v) in map {
                merged_map.insert(k, v);
            }
        }
        current = serde_json::from_value(merged).unwrap_or_default();
    }
    save(&current);
    current
}

pub fn add_provider(provider_config: ProviderConfig) -> ProviderConfig {
    let mut config = load();
    let new_config = ProviderConfig {
        id: if provider_config.id.is_empty() {
            format!("prov_{}", uuid::Uuid::new_v4().simple())
        } else {
            provider_config.id
        },
        name: if provider_config.name.is_empty() {
            "未命名".into()
        } else {
            provider_config.name
        },
        provider_id: provider_config.provider_id,
        creds: provider_config.creds,
        enabled: provider_config.enabled,
    };
    config.providers.push(new_config.clone());
    save(&config);
    new_config
}

pub fn update_provider(id: &str, updates: serde_json::Value) -> Option<ProviderConfig> {
    let mut config = load();
    if let Some(idx) = config.providers.iter().position(|p| p.id == id) {
        let mut current = serde_json::to_value(&config.providers[idx]).unwrap_or_default();
        if let serde_json::Value::Object(map) = updates {
            if let serde_json::Value::Object(current_map) = &mut current {
                for (k, v) in map {
                    current_map.insert(k, v);
                }
            }
        }
        if let Ok(updated) = serde_json::from_value::<ProviderConfig>(current) {
            config.providers[idx] = updated.clone();
            save(&config);
            return Some(config.providers[idx].clone());
        }
    }
    None
}

pub fn remove_provider(id: &str) {
    let mut config = load();
    config.providers.retain(|p| p.id != id);
    save(&config);
}
