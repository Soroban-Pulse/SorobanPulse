//! Webhook test command — Issue #964
//!
//! Sends a synthetic event payload — shaped exactly like a real delivery
//! from `/subscriptions/{id}` — directly to a callback URL, so a user can
//! verify their receiver before wiring up a live subscription. This talks
//! straight to the target URL; it does not go through the Soroban Pulse API.

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;
use std::time::{Duration, Instant};

/// Build a representative sample event payload, matching the shape of a
/// real webhook delivery body.
fn sample_payload(contract_id: &str) -> serde_json::Value {
    json!({
        "id": "00000000-0000-4000-8000-000000000000",
        "contract_id": contract_id,
        "event_type": "contract",
        "tx_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "ledger": 1,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "event_data": { "topic": ["test"], "value": {} },
        "test_delivery": true
    })
}

pub struct TestResult {
    pub status: u16,
    pub duration_ms: u128,
    pub body_snippet: String,
}

/// POST a sample event payload to `url` and report status/latency.
/// `timeout_secs` bounds how long we wait for the receiver to respond.
pub fn send(url: &str, contract_id: &str, timeout_secs: u64) -> Result<TestResult> {
    let payload = sample_payload(contract_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .context("building HTTP client")?;

    let start = Instant::now();
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-soroban-pulse-test", "true")
        .json(&payload)
        .send()
        .with_context(|| format!("sending test webhook to {url}"))?;
    let duration_ms = start.elapsed().as_millis();

    let status = resp.status().as_u16();
    let body = resp.text().unwrap_or_default();
    let body_snippet = if body.len() > 500 { format!("{}…", &body[..500]) } else { body };

    Ok(TestResult { status, duration_ms, body_snippet })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_payload_is_flagged_as_a_test_delivery() {
        let payload = sample_payload("CABCDEF");
        assert_eq!(payload["test_delivery"], true);
        assert_eq!(payload["contract_id"], "CABCDEF");
        assert_eq!(payload["event_type"], "contract");
    }

    #[test]
    fn send_reports_2xx_from_a_local_receiver() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/hook")
            .match_header("x-soroban-pulse-test", "true")
            .with_status(200)
            .with_body("ok")
            .create();

        let url = format!("{}/hook", server.url());
        let result = send(&url, "CTEST", 5).unwrap();

        mock.assert();
        assert_eq!(result.status, 200);
        assert_eq!(result.body_snippet, "ok");
    }

    #[test]
    fn send_reports_non_2xx_status() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/hook").with_status(500).create();

        let url = format!("{}/hook", server.url());
        let result = send(&url, "CTEST", 5).unwrap();

        assert_eq!(result.status, 500);
    }
}

pub fn print_result(url: &str, result: &TestResult) {
    let ok = (200..300).contains(&result.status);
    let status_display = if ok {
        result.status.to_string().green().bold()
    } else {
        result.status.to_string().red().bold()
    };

    println!("{} {url}", "Testing webhook:".bold());
    println!("  Status   : {status_display}");
    println!("  Latency  : {}ms", result.duration_ms);
    if !result.body_snippet.is_empty() {
        println!("  Response : {}", result.body_snippet.dimmed());
    }
    if ok {
        println!("{}", "✓ Receiver responded successfully.".green());
    } else {
        println!("{}", "✗ Receiver did not respond with a 2xx status.".red());
    }
}
