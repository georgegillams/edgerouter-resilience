//! Poll the remote-commands webhook and run an allowlisted action.
//!
//! Commands (derived from configured WAN names):
//! - `restart`
//! - `reset-<iface>` for any WAN
//! - `disable-<iface>` / `enable-<iface>` for ethernet WANs

use edgerouter_scripts::config::{self, Config, WanInterface};
use edgerouter_scripts::http;
use edgerouter_scripts::timestamp::now_string;
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct CommandEnvelope {
    payload: Option<CommandPayload>,
}

#[derive(Debug, Deserialize)]
struct CommandPayload {
    command: Option<String>,
}

fn main() -> ExitCode {
    let config = match config::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("config error: {}", err);
            return ExitCode::from(1);
        }
    };

    let body = match http::get_text(
        &config.webhooks.router_commands_url,
        &config.webhooks.router_commands_access_key,
    ) {
        Ok(body) => body,
        Err(_) => {
            println!("No commands to process");
            return ExitCode::SUCCESS;
        }
    };

    let Some(command) = parse_command(&body) else {
        println!("No commands to process");
        return ExitCode::SUCCESS;
    };

    dispatch(&config, &command);
    ExitCode::SUCCESS
}

fn parse_command(json_body: &str) -> Option<String> {
    let parsed: CommandEnvelope = serde_json::from_str(json_body).ok()?;
    let command = parsed.payload?.command?;
    let command = command.trim().to_string();
    if command.is_empty() {
        None
    } else {
        Some(command)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_payload_command() {
        let body = r#"{"payload":{"command":"reset-pppoe0"}}"#;
        assert_eq!(parse_command(body).as_deref(), Some("reset-pppoe0"));
    }

    #[test]
    fn trims_and_rejects_empty_command() {
        assert_eq!(
            parse_command(r#"{"payload":{"command":"  restart  "}}"#).as_deref(),
            Some("restart")
        );
        assert_eq!(parse_command(r#"{"payload":{"command":"   "}}"#), None);
    }

    #[test]
    fn missing_or_invalid_body_is_none() {
        assert_eq!(parse_command("{}"), None);
        assert_eq!(parse_command(r#"{"payload":{}}"#), None);
        assert_eq!(parse_command("not json"), None);
        assert_eq!(parse_command(""), None);
    }
}
