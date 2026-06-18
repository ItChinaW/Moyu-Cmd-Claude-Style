use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub zhihu: ZhihuConfig,
    #[serde(default)]
    pub nga: NgaConfig,
    #[serde(default)]
    pub linuxdo: LinuxDoConfig,
    #[serde(default)]
    pub stock: StockConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZhihuConfig {
    #[serde(default)]
    pub cookie: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct NgaConfig { #[serde(default)] pub cookie: String }

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinuxDoConfig { #[serde(default)] pub cookie: String }

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockConfig {
    #[serde(default)]
    pub watchlist: Vec<StockWatchItem>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockWatchItem {
    pub code: String,
    #[serde(default)]
    pub name: String,
}

impl Config {
    pub fn cookie_for(&self, p: crate::platform::Platform) -> String {
        use crate::platform::Platform::*;
        match p {
            Zhihu => self.zhihu.cookie.clone(),
            Nga => self.nga.cookie.clone(),
            LinuxDo => self.linuxdo.cookie.clone(),
            _ => String::new(),
        }
    }
    pub fn set_cookie_for(&mut self, p: crate::platform::Platform, cookie: String) {
        use crate::platform::Platform::*;
        match p {
            Zhihu => self.zhihu.cookie = cookie,
            Nga => self.nga.cookie = cookie,
            LinuxDo => self.linuxdo.cookie = cookie,
            _ => {}
        }
    }

    pub fn config_path() -> PathBuf {
        // Test/CI override so behavior never depends on a developer's real config.
        if let Ok(p) = std::env::var("TOUCH_FISH_CONFIG") {
            return PathBuf::from(p);
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("touch-fish")
            .join("config.toml")
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).context("parse config.toml"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).context("read config.toml"),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("create config dir")?;
        }
        let s = toml::to_string_pretty(self).context("serialize config")?;
        std::fs::write(path, s).context("write config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.zhihu.cookie = "d_c0=abc; z_c0=xyz".into();
        cfg.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(cfg, loaded);
    }

    #[test]
    fn config_roundtrips_all_cookies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.zhihu.cookie = "z".into();
        cfg.nga.cookie = "n".into();
        cfg.linuxdo.cookie = "l".into();
        cfg.stock.watchlist = vec![StockWatchItem { code: "159941".into(), name: "纳指ETF".into() }];
        cfg.save_to(&path).unwrap();
        assert_eq!(cfg, Config::load_from(&path).unwrap());
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.toml");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    }
}
