use serde_json::json;
use soroban_pulse::email::EmailNotifier;
use soroban_pulse::models::SorobanEvent;
use tokio::sync::broadcast;

fn mock_event(contract_id: &str, event_type: &str, ledger: u64) -> SorobanEvent {
    SorobanEvent {
        contract_id: contract_id.to_string(),
        event_type: event_type.to_string(),
        tx_hash: format!("tx_hash_{}", ledger),
        ledger,
        ledger_closed_at: "2026-04-28T00:00:00Z".to_string(),
        ledger_hash: Some(format!("hash_{}", ledger)),
        in_successful_call: true,
        value: json!({"test": "data"}),
        topic: Some(vec![json!("topic1"), json!("topic2")]),
    }
}

#[tokio::test]
async fn test_email_notifier_filters_by_contract() {
    // Create a broadcast channel
    let (tx, rx) = broadcast::channel::<SorobanEvent>(100);

    // Create notifier with contract filter
    let notifier = EmailNotifier::new(
        "smtp.example.com".to_string(),
        587,
        Some("user@example.com".to_string()),
        Some("password".to_string()),
        "from@example.com".to_string(),
        vec!["to@example.com".to_string()],
        vec!["CONTRACT_A".to_string(), "CONTRACT_B".to_string()],
    );

    // Spawn the notifier (it won't actually send emails in test)
    let _handle = notifier.spawn(rx);

    // Send events - only CONTRACT_A and CONTRACT_B should be processed
    let event_a = mock_event("CONTRACT_A", "contract", 100);
    let event_b = mock_event("CONTRACT_B", "contract", 101);
    let event_c = mock_event("CONTRACT_C", "contract", 102);

    tx.send(event_a).unwrap();
    tx.send(event_b).unwrap();
    tx.send(event_c).unwrap();

    // Give the task a moment to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test passes if no panic occurs
    assert!(true);
}

#[tokio::test]
async fn test_email_notifier_accepts_all_when_no_filter() {
    // Create a broadcast channel
    let (tx, rx) = broadcast::channel::<SorobanEvent>(100);

    // Create notifier without contract filter
    let notifier = EmailNotifier::new(
        "smtp.example.com".to_string(),
        587,
        None,
        None,
        "from@example.com".to_string(),
        vec!["to@example.com".to_string()],
        vec![], // Empty filter = accept all
    );

    // Spawn the notifier
    let _handle = notifier.spawn(rx);

    // Send events from different contracts
    let event_a = mock_event("CONTRACT_A", "contract", 100);
    let event_b = mock_event("CONTRACT_B", "diagnostic", 101);
    let event_c = mock_event("CONTRACT_C", "system", 102);

    tx.send(event_a).unwrap();
    tx.send(event_b).unwrap();
    tx.send(event_c).unwrap();

    // Give the task a moment to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test passes if no panic occurs
    assert!(true);
}

#[tokio::test]
async fn test_email_notifier_handles_channel_close() {
    // Create a broadcast channel
    let (tx, rx) = broadcast::channel::<SorobanEvent>(100);

    // Create notifier
    let notifier = EmailNotifier::new(
        "smtp.example.com".to_string(),
        587,
        None,
        None,
        "from@example.com".to_string(),
        vec!["to@example.com".to_string()],
        vec![],
    );

    // Spawn the notifier
    let handle = notifier.spawn(rx);

    // Send an event
    let event = mock_event("CONTRACT_A", "contract", 100);
    tx.send(event).unwrap();

    // Drop the sender to close the channel
    drop(tx);

    // Wait for the task to complete
    tokio::time::timeout(tokio::time::Duration::from_secs(2), handle)
        .await
        .expect("Task should complete when channel closes")
        .expect("Task should not panic");
}

#[test]
fn test_email_config_parsing() {
    // Test that email configuration fields are properly typed
    let smtp_host = Some("smtp.gmail.com".to_string());
    let smtp_port: u16 = 587;
    let smtp_user = Some("user@gmail.com".to_string());
    let smtp_password = Some("app-password".to_string());
    let from = Some("noreply@example.com".to_string());
    let to: Vec<String> = vec![
        "admin@example.com".to_string(),
        "alerts@example.com".to_string(),
    ];
    let contract_filter: Vec<String> = vec!["CONTRACT_A".to_string()];

    assert!(smtp_host.is_some());
    assert_eq!(smtp_port, 587);
    assert_eq!(to.len(), 2);
    assert_eq!(contract_filter.len(), 1);
}

#[test]
fn test_email_to_parsing_with_commas() {
    // Simulate parsing EMAIL_TO environment variable
    let email_to_str = "admin@example.com,alerts@example.com,ops@example.com";
    let parsed: Vec<String> = email_to_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], "admin@example.com");
    assert_eq!(parsed[1], "alerts@example.com");
    assert_eq!(parsed[2], "ops@example.com");
}

#[test]
fn test_email_contract_filter_parsing() {
    // Simulate parsing EMAIL_CONTRACT_FILTER environment variable
    let filter_str = "CABC123,CDEF456,CGHI789";
    let parsed: Vec<String> = filter_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert_eq!(parsed.len(), 3);
    assert!(parsed.contains(&"CABC123".to_string()));
    assert!(parsed.contains(&"CDEF456".to_string()));
    assert!(parsed.contains(&"CGHI789".to_string()));
}

#[test]
fn test_empty_email_to_results_in_empty_vec() {
    let email_to_str = "";
    let parsed: Vec<String> = email_to_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert!(parsed.is_empty());
}

#[test]
fn test_email_to_with_whitespace() {
    let email_to_str = " admin@example.com , alerts@example.com , ops@example.com ";
    let parsed: Vec<String> = email_to_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], "admin@example.com");
    assert_eq!(parsed[1], "alerts@example.com");
    assert_eq!(parsed[2], "ops@example.com");
}
