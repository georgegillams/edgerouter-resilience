//! Report configured WAN interface IPv4 addresses to a LAN network-monitor service.
//!
//! For each `[ip_reporter.targets]` entry with an address, issues:
//! `GET {base_url}/{path}?{ip}`

use edgerouter_scripts::config;
use edgerouter_scripts::http;
use edgerouter_scripts::ip_test;
use std::process::ExitCode;

fn main() -> ExitCode {
    let config = match config::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("config error: {}", err);
            return ExitCode::from(1);
        }
    };

    if !config.ip_reporter.is_configured() {
        println!("ip_reporter not configured; nothing to do");
        return ExitCode::SUCCESS;
    }

    let mut had_error = false;

    for target in &config.ip_reporter.targets {
        let label = target
            .label
            .as_deref()
            .unwrap_or(target.interface.as_str());

        let Some(ip) = ip_test::interface_ipv4(&target.interface) else {
            println!("{} ({}): no IPv4 address", label, target.interface);
            continue;
        };

        let url = config.ip_reporter.report_url(&target.path, &ip);
        println!("ADDING {} IP: {} -> {}", label, ip, url);

        match http::get_plain(&url) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("failed to report {} IP {}: {}", label, ip, err);
                had_error = true;
            }
        }
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
