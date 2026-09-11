//! Detect which WAN outbound traffic is using, and whether a better one is available.
//!
//! Compares the public NAT address for a configured LAN probe host with each
//! configured WAN interface address. Preference order comes from config
//! (`wan_interfaces`, highest first).

use crate::config::{Config, WanInterface};
use std::process::Command;

/// Snapshot of NAT + per-WAN IPv4 addresses (same order as config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceIps {
    pub nat_ip: Option<String>,
    /// `(interface_name, ipv4)` in config preference order.
    pub wans: Vec<(String, Option<String>)>,
}

impl InterfaceIps {
    pub fn ip_for(&self, name: &str) -> Option<&str> {
        self.wans
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, ip)| ip.as_deref())
    }

    pub fn is_up(&self, name: &str) -> bool {
        self.ip_for(name).is_some()
    }
}

/// Which WAN the NAT public IP currently matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpTestResult {
    /// Matched a configured WAN. `index` is preference rank (0 = highest).
    Matched {
        index: usize,
        interface: String,
        ip: String,
        interfaces: InterfaceIps,
    },
    /// NAT IP missing, or it does not match any configured WAN IP.
    MissingIp { interfaces: InterfaceIps },
}

impl IpTestResult {
    pub fn interfaces(&self) -> &InterfaceIps {
        match self {
            IpTestResult::Matched { interfaces, .. } | IpTestResult::MissingIp { interfaces } => {
                interfaces
            }
        }
    }

    /// If traffic is on a lower-preference WAN but a higher one is up, return
    /// the higher-preference interface name to reset.
    pub fn preferred_upgrade_reset(&self, wans: &[WanInterface]) -> Option<String> {
        let IpTestResult::Matched {
            index: current_index,
            interfaces,
            ..
        } = self
        else {
            return None;
        };

        // Among interfaces preferred over the current one, pick the best that is up.
        for (i, wan) in wans.iter().enumerate() {
            if i >= *current_index {
                break;
            }
            if interfaces.is_up(&wan.name) {
                return Some(wan.name.clone());
            }
        }
        None
    }
}

/// Classify which WAN the probe device's NAT public IP matches.
pub fn check(config: &Config) -> IpTestResult {
    let mut wans = Vec::with_capacity(config.wan_interfaces.len());
    for wan in &config.wan_interfaces {
        let ip = interface_ipv4(&wan.name);
        println!("{} IP: {}", wan.name, ip.as_deref().unwrap_or(""));
        wans.push((wan.name.clone(), ip));
    }

    let nat_ip = nat_ip_for_device(&config.runop, &config.nat_probe_device);
    println!("Current IP: {}", nat_ip.as_deref().unwrap_or(""));

    let interfaces = InterfaceIps { nat_ip, wans };

    let Some(nat) = interfaces.nat_ip.clone() else {
        println!("NAT IP missing");
        return IpTestResult::MissingIp { interfaces };
    };

    for (index, (name, ip)) in interfaces.wans.iter().enumerate() {
        if ip.as_ref() == Some(&nat) {
            println!("IPs match {} (preference rank {})", name, index);
            return IpTestResult::Matched {
                index,
                interface: name.clone(),
                ip: nat,
                interfaces,
            };
        }
    }

    println!("NAT IP does not match any configured WAN");
    IpTestResult::MissingIp { interfaces }
}

fn nat_ip_for_device(runop: &str, device: &str) -> Option<String> {
    let output = Command::new(runop)
        .args(["show", "nat", "translations"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.contains(device) {
            continue;
        }
        let public = line.split_whitespace().nth(1)?;
        if !public.is_empty() {
            return Some(public.to_string());
        }
    }
    None
}

fn interface_ipv4(interface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["addr", "show", interface])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("inet ") else {
            continue;
        };
        let addr = rest.split_whitespace().next()?;
        let ip = addr.split('/').next()?;
        if is_ipv4(ip) {
            return Some(ip.to_string());
        }
    }
    None
}

fn is_ipv4(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WanKind;

    fn sample_wans() -> Vec<WanInterface> {
        vec![
            WanInterface {
                name: "pppoe0".into(),
                kind: WanKind::Pppoe,
            },
            WanInterface {
                name: "pppoe1".into(),
                kind: WanKind::Pppoe,
            },
            WanInterface {
                name: "eth2".into(),
                kind: WanKind::Ethernet,
            },
        ]
    }

    fn ifaces(nat: &str, ips: &[(&str, Option<&str>)]) -> InterfaceIps {
        InterfaceIps {
            nat_ip: Some(nat.into()),
            wans: ips
                .iter()
                .map(|(n, ip)| ((*n).to_string(), ip.map(str::to_string)))
                .collect(),
        }
    }

    fn matched(index: usize, name: &str, ip: &str, interfaces: InterfaceIps) -> IpTestResult {
        IpTestResult::Matched {
            index,
            interface: name.into(),
            ip: ip.into(),
            interfaces,
        }
    }

    #[test]
    fn upgrade_from_lowest_prefers_best_available() {
        let wans = sample_wans();
        let on_eth2_with_pppoe0 = matched(
            2,
            "eth2",
            "3.3.3.3",
            ifaces(
                "3.3.3.3",
                &[
                    ("pppoe0", Some("1.1.1.1")),
                    ("pppoe1", Some("2.2.2.2")),
                    ("eth2", Some("3.3.3.3")),
                ],
            ),
        );
        assert_eq!(
            on_eth2_with_pppoe0.preferred_upgrade_reset(&wans),
            Some("pppoe0".into())
        );

        let on_eth2_pppoe0_down = matched(
            2,
            "eth2",
            "3.3.3.3",
            ifaces(
                "3.3.3.3",
                &[
                    ("pppoe0", None),
                    ("pppoe1", Some("2.2.2.2")),
                    ("eth2", Some("3.3.3.3")),
                ],
            ),
        );
        assert_eq!(
            on_eth2_pppoe0_down.preferred_upgrade_reset(&wans),
            Some("pppoe1".into())
        );

        let on_eth2_only = matched(
            2,
            "eth2",
            "3.3.3.3",
            ifaces(
                "3.3.3.3",
                &[("pppoe0", None), ("pppoe1", None), ("eth2", Some("3.3.3.3"))],
            ),
        );
        assert_eq!(on_eth2_only.preferred_upgrade_reset(&wans), None);
    }

    #[test]
    fn upgrade_from_middle_only_if_better_up() {
        let wans = sample_wans();
        let best_available = matched(
            1,
            "pppoe1",
            "2.2.2.2",
            ifaces(
                "2.2.2.2",
                &[
                    ("pppoe0", None),
                    ("pppoe1", Some("2.2.2.2")),
                    ("eth2", Some("3.3.3.3")),
                ],
            ),
        );
        assert_eq!(best_available.preferred_upgrade_reset(&wans), None);

        let can_upgrade = matched(
            1,
            "pppoe1",
            "2.2.2.2",
            ifaces(
                "2.2.2.2",
                &[
                    ("pppoe0", Some("1.1.1.1")),
                    ("pppoe1", Some("2.2.2.2")),
                    ("eth2", None),
                ],
            ),
        );
        assert_eq!(
            can_upgrade.preferred_upgrade_reset(&wans),
            Some("pppoe0".into())
        );
    }

    #[test]
    fn already_on_best_needs_no_upgrade() {
        let wans = sample_wans();
        let on_best = matched(
            0,
            "pppoe0",
            "1.1.1.1",
            ifaces(
                "1.1.1.1",
                &[
                    ("pppoe0", Some("1.1.1.1")),
                    ("pppoe1", Some("2.2.2.2")),
                    ("eth2", Some("3.3.3.3")),
                ],
            ),
        );
        assert_eq!(on_best.preferred_upgrade_reset(&wans), None);
    }
}
