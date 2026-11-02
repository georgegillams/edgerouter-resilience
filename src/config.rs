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

/// Report a WAN interface's IPv4 to a network-monitor endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IpReporterTarget {
    /// Interface name (e.g. `pppoe0`).
    pub interface: String,
    /// Path appended to `base_url` (e.g. `add-fttp-ip` → `{base}/{path}?{ip}`).
    pub path: String,
    /// Optional label for log lines (defaults to `interface`).
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct IpReporter {
    /// Base URL of the network monitor (e.g. `http://192.168.1.96:3001`).
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub targets: Vec<IpReporterTarget>,
}

impl IpReporter {
    pub fn is_configured(&self) -> bool {
        !self.base_url.trim().is_empty() && !self.targets.is_empty()
    }

    /// `{base_url}/{path}?{ip}`
    pub fn report_url(&self, path: &str, ip: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}?{}", base, path, ip)
    }
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
    /// Optional: report selected WAN IPs to a LAN network-monitor service.
    #[serde(default)]
    pub ip_reporter: IpReporter,
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

[ip_reporter]
base_url = "http://192.168.1.96:3001"

[[ip_reporter.targets]]
interface = "pppoe0"
path = "add-fttp-ip"
label = "FTTP"

[[ip_reporter.targets]]
interface = "pppoe1"
path = "add-fttc-ip"
label = "FTTC"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.wan_interfaces.len(), 3);
        assert_eq!(config.primary_wan().unwrap().name, "pppoe0");
        assert_eq!(config.timing.post_reset_conntrack_delay_secs, 15);
        assert!(config.ip_reporter.is_configured());
        assert_eq!(
            config.ip_reporter.report_url("add-fttp-ip", "1.2.3.4"),
            "http://192.168.1.96:3001/add-fttp-ip?1.2.3.4"
        );
    }
}
