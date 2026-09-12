//! Upload new lines from the auto-reset log to the webhook.

use edgerouter_scripts::config::{self, Config};
use edgerouter_scripts::http;
use serde::Serialize;
use std::fs;
use std::process::ExitCode;

#[derive(Serialize)]
struct LogsBody<'a> {
    logs: &'a str,
}

fn main() -> ExitCode {
    let config = match config::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("config error: {}", err);
            return ExitCode::from(1);
        }
    };

    if fs::metadata(&config.log_file).is_err() {
        println!("Log file not found!");
        return ExitCode::SUCCESS;
    }

    let (_last_uploaded_line, last_uploaded_line_number) = load_bookmark(&config);
    let upload_from_line_number = last_uploaded_line_number + 1;
    println!("Uploading from line number: {}", upload_from_line_number);

    let logs_since_last_upload = new_log_lines(&config, upload_from_line_number);
    println!("Logs since last upload: {}", logs_since_last_upload);

    if logs_since_last_upload.trim().is_empty() {
        println!("No logs since last upload");
        return ExitCode::SUCCESS;
    }

    let status = match http::post_json(
        &config.webhooks.reset_logs_url,
        &config.webhooks.reset_logs_access_key,
        &LogsBody {
            logs: &logs_since_last_upload,
        },
    ) {
        Ok(status) => status,
        Err(_) => {
            println!("Failed to upload logs");
            return ExitCode::from(1);
        }
    };

    if status != 200 {
        println!("Failed to upload logs");
        return ExitCode::from(1);
    }

    let last_line = logs_since_last_upload
        .lines()
        .rev()
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string();

    println!("Last uploaded line: {}", last_line);
    let _ = fs::write(&config.log_last_upload_file, &last_line);

    ExitCode::SUCCESS
}

fn load_bookmark(config: &Config) -> (String, i32) {
    if fs::metadata(&config.log_last_upload_file).is_err() {
        println!("No last upload file found");
        return ("NO_LAST_UPLOAD_LINE".to_string(), -1);
    }

    println!("Last upload file found");
    let last_uploaded_line = fs::read_to_string(&config.log_last_upload_file)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .to_string();

    let log = fs::read_to_string(&config.log_file).unwrap_or_default();
    let last_uploaded_line_number = log
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == last_uploaded_line)
        .map(|(i, _)| (i + 1) as i32)
        .last()
        .unwrap_or(0);

    (last_uploaded_line, last_uploaded_line_number)
}

fn new_log_lines(config: &Config, from_line_number: i32) -> String {
    let log = fs::read_to_string(&config.log_file).unwrap_or_default();
    let start = if from_line_number <= 1 {
        0
    } else {
        (from_line_number - 1) as usize
    };

    log.lines()
        .skip(start)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_body_json_escapes_newlines_and_quotes() {
        let body = LogsBody {
            logs: "line 1\nline \"two\"",
        };
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(json, r#"{"logs":"line 1\nline \"two\""}"#);
    }
}
