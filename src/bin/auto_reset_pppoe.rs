//! Automatically repair WAN connections when they misbehave.
//!
//! Intended to run every ~5 minutes via cron. Checks (in order):
//! 1. WAN preference (config order) — if traffic is on a lower-preference
//!    link but a better one is up, reset that better link immediately
//! 2. Offline (pinger) — reset PPPoE WANs after repeated failures; reboot later
//! 3. Missing interface IPs — reset as needed
//!
//! Also appends a status line during a short window at 02:xx and 14:xx.

use edgerouter_scripts::config::{self, Config, WanInterface};
use edgerouter_scripts::ip_test;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let config = match config::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("config error: {}", err);
            return ExitCode::from(1);
        }
    };

    println!("Checking WAN connections");

    let wan_ips: Vec<(String, Option<String>)> = config
        .wan_interfaces
        .iter()
        .map(|w| (w.name.clone(), interface_has_inet(&w.name)))
        .collect();

    for (name, ip) in &wan_ips {
        println!("{} IP: {}", name, ip.as_deref().unwrap_or(""));
    }

    maybe_log_scheduled_status(&config, &wan_ips);

    if handle_wan_preference(&config) {
        return ExitCode::SUCCESS;
    }

    if handle_ping_failures(&config) {
        return ExitCode::SUCCESS;
    }

    if handle_missing_ips(&config, &wan_ips) {
        return ExitCode::SUCCESS;
    }

    ExitCode::SUCCESS
}

fn interface_has_inet(interface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["addr", "show", interface])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("inet ") {
            return Some(line.trim().to_string());
        }
    }
    None
}

fn now_string() -> String {
    Command::new("date")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown-date".to_string())
}

fn append_log(config: &Config, message: &str) {
    let line = format!("{}\n", message);
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(line.as_bytes())
        });
}

fn read_count(path: &str) -> u32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_count(path: &str, count: u32) {
    let _ = fs::write(path, format!("{}\n", count));
}

fn run_helper(config: &Config, script: &str, args: &[String]) {
    let path = Path::new(&config.scripts_dir).join(script);
    let status = Command::new("bash").arg(&path).args(args).status();
    if let Err(err) = status {
        eprintln!("Failed to run {}: {}", path.display(), err);
    }
}

fn reset_wan(config: &Config, wan: &WanInterface) {
    let (script, args) = config.reset_command(wan);
    run_helper(config, script, &args);
}

fn flush_conntrack(config: &Config) {
    match Command::new(&config.conntrack_path).arg("-F").status() {
        Ok(status) if status.success() => println!("Flushed conntrack"),
        Ok(status) => eprintln!("conntrack -F exited with {}", status),
        Err(err) => eprintln!("Failed to run conntrack -F: {}", err),
    }
}

fn current_hour_minute() -> Option<(u32, u32)> {
    let hour = Command::new("date")
        .args(["+%H"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())?
        .trim()
        .parse()
        .ok()?;
    let minute = Command::new("date")
        .args(["+%M"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())?
        .trim()
        .parse()
        .ok()?;
    Some((hour, minute))
}

fn maybe_log_scheduled_status(config: &Config, wan_ips: &[(String, Option<String>)]) {
    let Some((hour, minute)) = current_hour_minute() else {
        return;
    };
    if (hour == 2 || hour == 14) && (2..=8).contains(&minute) {
        let summary: Vec<String> = wan_ips
            .iter()
            .map(|(n, ip)| format!("{}: {}", n, ip.as_deref().unwrap_or("")))
            .collect();
        append_log(
            config,
            &format!(
                "{} Unconditionally logging. Not resetting. {}",
                now_string(),
                summary.join(", ")
            ),
        );
    }
}

fn handle_wan_preference(config: &Config) -> bool {
    let result = ip_test::check(config);
    let Some(iface_name) = result.preferred_upgrade_reset(&config.wan_interfaces) else {
        return false;
    };
    let Some(wan) = config.wan_by_name(&iface_name) else {
        return false;
    };

    append_log(
        config,
        &format!(
            "{} Traffic not on preferred WAN; better link {} is up. Resetting {} and flushing conntrack.",
            now_string(),
            wan.name,
            wan.name
        ),
    );
    reset_wan(config, wan);
    thread::sleep(Duration::from_secs(
        config.timing.post_reset_conntrack_delay_secs,
    ));
    flush_conntrack(config);
    true
}

fn handle_ping_failures(config: &Config) -> bool {
    let mut ping_fail_count = read_count(&config.ping_fail_count_file);

    let ping_ok = Command::new(Path::new(&config.scripts_dir).join("pinger.sh"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ping_ok {
        ping_fail_count = 0;
    } else {
        ping_fail_count += 1;
    }
    write_count(&config.ping_fail_count_file, ping_fail_count);

    if (1..=2).contains(&ping_fail_count) {
        append_log(
            config,
            &format!(
                "{} Ping failed {} times. Taking no action for now.",
                now_string(),
                ping_fail_count
            ),
        );
    }

    if (3..=8).contains(&ping_fail_count) {
        append_log(
            config,
            &format!(
                "{} Ping failed {} times. Resetting all PPPoE WANs",
                now_string(),
                ping_fail_count
            ),
        );
        reset_all_pppoe(config);
        return true;
    }

    if ping_fail_count >= 9 {
        append_log(
            config,
            &format!(
                "{} Ping failed {} times. Restarting router",
                now_string(),
                ping_fail_count
            ),
        );
        thread::sleep(Duration::from_secs(config.timing.reboot_delay_secs));
        run_helper(config, "reboot.sh", &[]);
        return true;
    }

    false
}

/// Reset PPPoE WANs lowest-preference first (matches historic bt-then-trooli order).
fn reset_all_pppoe(config: &Config) {
    let mut pppoes: Vec<&WanInterface> = config.pppoe_wans().collect();
    pppoes.reverse();
    for (i, wan) in pppoes.iter().enumerate() {
        if i > 0 {
            thread::sleep(Duration::from_secs(
                config.timing.between_resets_delay_secs,
            ));
        }
        reset_wan(config, wan);
    }
}

fn handle_missing_ips(config: &Config, wan_ips: &[(String, Option<String>)]) -> bool {
    let pppoe_names: Vec<&str> = config.pppoe_wans().map(|w| w.name.as_str()).collect();
    if pppoe_names.is_empty() {
        return false;
    }

    let pppoe_down = |name: &str| {
        wan_ips
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, ip)| ip.is_none())
            .unwrap_or(true)
    };

    if pppoe_names.iter().all(|n| pppoe_down(n)) {
        println!("All PPPoE WANs are down");
        append_log(
            config,
            &format!(
                "{} All PPPoE WANs have no IP addresses. Resetting them.",
                now_string()
            ),
        );
        reset_all_pppoe(config);
        return true;
    }

    if let Some(primary) = config.primary_wan() {
        if primary.kind == edgerouter_scripts::config::WanKind::Pppoe && pppoe_down(&primary.name)
        {
            println!("{} is down", primary.name);
            append_log(
                config,
                &format!(
                    "{} {} has no IP address. Resetting it.",
                    now_string(),
                    primary.name
                ),
            );
            reset_wan(config, primary);
            return true;
        }
    }

    false
}
