//! Blocking HTTPS helpers for the webhook endpoints.
//!
//! Uses native-tls (vendored OpenSSL on MIPS) because rustls/ring does not
//! support 32-bit big-endian MIPS.

use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use ureq::tls::{TlsConfig, TlsProvider};
use ureq::Agent;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .build(),
            )
            .build()
            .new_agent()
    })
}

/// GET `url` with the webhook `access-key` header. Returns the response body.
pub fn get_text(url: &str, access_key: &str) -> Result<String, String> {
    let mut response = agent()
        .get(url)
        .header("access-key", access_key)
        .call()
        .map_err(|err| err.to_string())?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|err| err.to_string())
}

/// POST JSON `body` with the webhook `access-key` header. Returns the HTTP status.
pub fn post_json<T: Serialize>(url: &str, access_key: &str, body: &T) -> Result<u16, String> {
    let response = agent()
        .post(url)
        .header("access-key", access_key)
        .send_json(body)
        .map_err(|err| err.to_string())?;
    Ok(response.status().as_u16())
}
