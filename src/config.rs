//! Shared configuration loaded from `config.toml`.

use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// How an interface is brought up/down on EdgeOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WanKind {
    /// Disconnect/connect via op-mode (`reset-pppoe.sh`).
    Pppoe,
    /// Disable/enable via config (`reset-ethernet.sh` / enable / disable).
    Ethernet,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WanInterface {
    pub name: String,
    pub kind: WanKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Webhooks {
    pub reset_logs_url: String,
    pub reset_logs_access_key: String,
    pub router_commands_url: String,
    pub router_commands_access_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Timing {
    #[serde(default = "default_post_reset_conntrack_delay_secs")]
    pub post_reset_conntrack_delay_secs: u64,
    #[serde(default = "default_between_resets_delay_secs")]
    pub between_resets_delay_secs: u64,
    #[serde(default = "default_reboot_delay_secs")]
    pub reboot_delay_secs: u64,
}

fn default_post_reset_conntrack_delay_secs() -> u64 {
    15
}
fn default_between_resets_delay_secs() -> u64 {
    15
}
fn default_reboot_delay_secs() -> u64 {
    5
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            post_reset_conntrack_delay_secs: default_post_reset_conntrack_delay_secs(),
            between_resets_delay_secs: default_between_resets_delay_secs(),
            reboot_delay_secs: default_reboot_delay_secs(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Config {
    /// Directory containing helper `.sh` scripts (usually `/config/scripts`).
    pub scripts_dir: String,
    pub log_file: String,
    pub ping_fail_count_file: String,
    pub log_last_upload_file: String,
    pub conntrack_path: String,
    pub runop: String,
    /// LAN host whose NAT translation reveals the public IP in use.
    pub nat_probe_device: String,
    /// WAN interfaces in preference order (highest preference first).
    pub wan_interfaces: Vec<WanInterface>,
    pub webhooks: Webhooks,
    #[serde(default)]
    pub timing: Timing,
}

impl Config {
    /// Highest-preference WAN (first in the list).
    pub fn primary_wan(&self) -> Option<&WanInterface> {
        self.wan_interfaces.first()
    }

    pub fn wan_by_name(&self, name: &str) -> Option<&WanInterface> {
        self.wan_interfaces.iter().find(|w| w.name == name)
    }

    pub fn pppoe_wans(&self) -> impl Iterator<Item = &WanInterface> {
        self.wan_interfaces
            .iter()
            .filter(|w| w.kind == WanKind::Pppoe)
    }

    /// Script + args to bounce an interface back up.
    pub fn reset_command(&self, wan: &WanInterface) -> (&'static str, Vec<String>) {
        match wan.kind {
            WanKind::Pppoe => ("reset-pppoe.sh", vec![wan.name.clone()]),
            WanKind::Ethernet => ("reset-ethernet.sh", vec![wan.name.clone()]),
        }
    }

    pub fn enable_command(&self, wan: &WanInterface) -> Option<(&'static str, Vec<String>)> {
        match wan.kind {
            WanKind::Ethernet => Some(("enable-ethernet.sh", vec![wan.name.clone()])),
            WanKind::Pppoe => None,
        }
    }

    pub fn disable_command(&self, wan: &WanInterface) -> Option<(&'static str, Vec<String>)> {
        match wan.kind {
            WanKind::Ethernet => Some(("disable-ethernet.sh", vec![wan.name.clone()])),
            WanKind::Pppoe => None,
        }
    }
}

/// Load config from `EDGEROUTER_SCRIPTS_CONFIG`, then alongside the binary, then `/config/scripts/config.toml`.
pub fn load() -> Result<Config, String> {
    let path = find_config_path().ok_or_else(|| {
        "config.toml not found (set EDGEROUTER_SCRIPTS_CONFIG or place config.toml next to the binary)"
            .to_string()
    })?;
    load_from_path(&path)
}

pub fn load_from_path(path: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let config: Config = toml::from_str(&text)
        .map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    if config.wan_interfaces.is_empty() {
        return Err("wan_interfaces must not be empty".to_string());
    }
    Ok(config)
}

fn find_config_path() -> Option<PathBuf> {
    if let Ok(path) = env::var("EDGEROUTER_SCRIPTS_CONFIG") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("config.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let fallback = PathBuf::from("/config/scripts/config.toml");
    if fallback.is_file() {
        return Some(fallback);
    }

    // Convenience when running from the repo during development.
    let local = PathBuf::from("config.toml");
    if local.is_file() {
        return Some(local);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_shape() {
        let toml = r#"
scripts_dir = "/config/scripts"
log_file = "/tmp/auto-reset-pppoe.log"
ping_fail_count_file = "/tmp/auto-reset-pppoe-ping-fail-count.log"
log_last_upload_file = "/tmp/auto-reset-pppoe-last-upload.log"
conntrack_path = "/usr/sbin/conntrack"
runop = "/opt/vyatta/bin/vyatta-op-cmd-wrapper"
nat_probe_device = "192.168.1.96"

[[wan_interfaces]]
name = "pppoe0"
kind = "pppoe"

[[wan_interfaces]]
name = "pppoe1"
kind = "pppoe"

[[wan_interfaces]]
name = "eth2"
kind = "ethernet"

[webhooks]
reset_logs_url = "https://example.com/logs"
reset_logs_access_key = "key1"
router_commands_url = "https://example.com/commands"
router_commands_access_key = "key2"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.wan_interfaces.len(), 3);
        assert_eq!(config.primary_wan().unwrap().name, "pppoe0");
        assert_eq!(config.timing.post_reset_conntrack_delay_secs, 15);
    }
}
