//! Poll the remote-commands webhook and run an allowlisted action.
//!
//! Commands (derived from configured WAN names):
//! - `restart`
//! - `reset-<iface>` for any WAN
//! - `disable-<iface>` / `enable-<iface>` for ethernet WANs

use edgerouter_scripts::config::{self, Config, WanInterface};
use std::fs;
use std::io::Write;
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

    let curl_result = Command::new("curl")
        .args([
            "-X",
            "GET",
            &config.webhooks.router_commands_url,
            "-H",
            &format!("access-key: {}", config.webhooks.router_commands_access_key),
        ])
        .output();

    let Ok(output) = curl_result else {
        println!("No commands to process");
        return ExitCode::SUCCESS;
    };

    let body = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !body.contains("\"command\"") {
        println!("No commands to process");
        return ExitCode::SUCCESS;
    }

    let command = extract_command(&body).unwrap_or_default();
    dispatch(&config, &command);
    ExitCode::SUCCESS
}

fn extract_command(json_body: &str) -> Option<String> {
    let jq = Command::new("jq")
        .args(["-r", ".payload.command"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn();

    if let Ok(mut child) = jq {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(json_body.as_bytes());
        }
        if let Ok(output) = child.wait_with_output() {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() && value != "null" {
                    return Some(value);
                }
            }
        }
    }

    const KEY: &str = "\"command\"";
    let idx = json_body.find(KEY)?;
    let after = &json_body[idx + KEY.len()..];
    let after = after.trim_start().trim_start_matches(':').trim_start();
    let after = after.trim_start_matches('"');
    let end = after.find('"')?;
    Some(after[..end].to_string())
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
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_file)
        .and_then(|mut f| writeln!(f, "{}", message));
}

fn run_helper(config: &Config, script: &str, args: &[String]) {
    let path = Path::new(&config.scripts_dir).join(script);
    let _ = Command::new("bash").arg(&path).args(args).status();
}

fn reset_wan(config: &Config, wan: &WanInterface) {
    let (script, args) = config.reset_command(wan);
    run_helper(config, script, &args);
}

fn dispatch(config: &Config, command: &str) {
    if command == "restart" {
        println!("Restarting");
        append_log(
            config,
            &format!("{} Command received: restarting", now_string()),
        );
        thread::sleep(Duration::from_secs(config.timing.reboot_delay_secs));
        run_helper(config, "reboot.sh", &[]);
        return;
    }

    if let Some(name) = command.strip_prefix("reset-") {
        if let Some(wan) = config.wan_by_name(name) {
            println!("Resetting {}", wan.name);
            append_log(
                config,
                &format!(
                    "{} Command received: resetting {}",
                    now_string(),
                    wan.name
                ),
            );
            reset_wan(config, wan);
        }
        return;
    }

    if let Some(name) = command.strip_prefix("disable-") {
        if let Some(wan) = config.wan_by_name(name) {
            if let Some((script, args)) = config.disable_command(wan) {
                println!("Disabling {}", wan.name);
                append_log(
                    config,
                    &format!(
                        "{} Command received: disabling {}",
                        now_string(),
                        wan.name
                    ),
                );
                run_helper(config, script, &args);
            }
        }
        return;
    }

    if let Some(name) = command.strip_prefix("enable-") {
        if let Some(wan) = config.wan_by_name(name) {
            if let Some((script, args)) = config.enable_command(wan) {
                println!("Enabling {}", wan.name);
                append_log(
                    config,
                    &format!("{} Command received: enabling {}", now_string(), wan.name),
                );
                run_helper(config, script, &args);
            }
        }
        return;
    }
}
