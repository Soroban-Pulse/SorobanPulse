//! End-to-end tests for the `spulse` binary against a mocked API (Issue #1136).

use assert_cmd::Command;
use mockito::{Matcher, Server};
use predicates::prelude::*;
use tempfile::TempDir;

const CONTRACT: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

/// `spulse` pointed at `base_url`, with config isolated to a temp dir.
fn spulse(base_url: &str, home: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("spulse").unwrap();
    cmd.env("SPULSE_BASE_URL", base_url)
        .env("SPULSE_API_KEY", "sk_test")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .env("NO_COLOR", "1");
    cmd
}

fn event_json(ledger: i64) -> String {
    format!(
        r#"{{"contract_id":"{CONTRACT}","event_type":"contract","tx_hash":"ab12","ledger":{ledger},"ledger_closed_at":"2026-01-01T00:00:00Z","in_successful_call":true,"value":{{"amount":"10"}}}}"#
    )
}

#[test]
fn events_prints_json_and_sends_query_and_key() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/v1/events")
        .match_header("x-api-key", "sk_test")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "5".into()),
            Matcher::UrlEncoded("page".into(), "1".into()),
            Matcher::UrlEncoded("event_type".into(), "contract".into()),
        ]))
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{"events":[{}],"page":1,"limit":5,"total":1}}"#,
            event_json(42)
        ))
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .args([
            "-f",
            "json",
            "events",
            "--limit",
            "5",
            "--event-type",
            "contract",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(r#""ledger": 42"#).and(predicate::str::contains(CONTRACT)),
        );
    mock.assert();
}

#[test]
fn events_with_contract_uses_contract_endpoint() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", format!("/v1/events/contract/{CONTRACT}").as_str())
        .match_query(Matcher::Any)
        .with_body(format!("[{}]", event_json(7)))
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .args(["-f", "csv", "events", "-c", CONTRACT])
        .assert()
        .success()
        .stdout(predicate::str::contains("ab12"));
    mock.assert();
}

#[test]
fn events_reports_http_errors_with_server_message() {
    let mut server = Server::new();
    server
        .mock("GET", "/v1/events")
        .match_query(Matcher::Any)
        .with_status(500)
        .with_body(r#"{"error":"database unavailable"}"#)
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .arg("events")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("HTTP 500")
                .and(predicate::str::contains("database unavailable")),
        );
}

#[test]
fn stats_prints_totals() {
    let mut server = Server::new();
    server
        .mock("GET", "/v1/events/stats")
        .with_body(r#"{"total_events":1234,"total_contracts":5,"latest_ledger":999,"events_last_24h":null}"#)
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .arg("stats")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Total events    : 1234")
                .and(predicate::str::contains("Latest ledger   : 999")),
        );
}

#[test]
fn stats_fails_on_rejected_api_key() {
    let mut server = Server::new();
    server
        .mock("GET", "/v1/events/stats")
        .with_status(401)
        .with_body(r#"{"message":"invalid api key"}"#)
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .arg("stats")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("HTTP 401").and(predicate::str::contains("invalid api key")),
        );
}

#[test]
fn tail_streams_ndjson() {
    let mut server = Server::new();
    let mock = server
        .mock("GET", "/v1/events/stream")
        .match_header("accept", "text/event-stream")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("contract_id".into(), CONTRACT.into()),
            Matcher::UrlEncoded("event_type".into(), "contract".into()),
        ]))
        .with_header("content-type", "text/event-stream")
        .with_body(format!(
            ": keep-alive\n\nid: 1\ndata: {}\n\nid: 2\ndata: {}\n\n",
            event_json(1),
            event_json(2)
        ))
        .create();
    let home = TempDir::new().unwrap();

    let out = spulse(&server.url(), &home)
        .args(["tail", "--contract", CONTRACT, "--type", "contract"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    mock.assert();

    let lines: Vec<serde_json::Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("each line is one JSON object"))
        .collect();
    assert_eq!(
        lines
            .iter()
            .map(|v| v["ledger"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn tail_fails_on_http_error() {
    let mut server = Server::new();
    server
        .mock("GET", "/v1/events/stream")
        .match_query(Matcher::Any)
        .with_status(400)
        .with_body(r#"{"error":"invalid contract_id"}"#)
        .create();
    let home = TempDir::new().unwrap();

    spulse(&server.url(), &home)
        .args(["tail", "-c", "not-a-contract"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("HTTP 400")
                .and(predicate::str::contains("invalid contract_id")),
        );
}

#[test]
fn tail_requires_contract() {
    let home = TempDir::new().unwrap();
    spulse("http://127.0.0.1:9", &home)
        .arg("tail")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--contract"));
}

#[test]
fn config_path_is_under_the_config_dir() {
    let home = TempDir::new().unwrap();
    spulse("http://127.0.0.1:9", &home)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            home.path().to_string_lossy().to_string(),
        ));
}

#[test]
fn completions_generates_scripts() {
    let home = TempDir::new().unwrap();
    spulse("http://127.0.0.1:9", &home)
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("#compdef spulse"));
    spulse("http://127.0.0.1:9", &home)
        .args(["completions", "tcsh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'tcsh'"));
}

#[test]
fn version_includes_git_commit() {
    let home = TempDir::new().unwrap();
    spulse("http://127.0.0.1:9", &home)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^spulse \d+\.\d+\.\d+ \([0-9a-z]+\)\n$").unwrap());
}
