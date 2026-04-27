//! Issue #267: XDR validation for Soroban event data using the `stellar-xdr` crate.
//!
//! Validates `event_data.value` and each element of `event_data.topic` as `ScVal`
//! (Soroban Contract Value). Events that fail validation are logged at WARN and
//! counted in the `soroban_pulse_events_xdr_invalid_total` metric.

use serde_json::Value;
use stellar_xdr::curr::ScVal;
use tracing::warn;

use crate::metrics;

/// Validate that a JSON value can be deserialized as a `ScVal`.
/// Returns `true` if valid, `false` otherwise.
fn is_valid_sc_val(v: &Value) -> bool {
    serde_json::from_value::<ScVal>(v.clone()).is_ok()
}

/// Validate the `event_data.value` and `event_data.topic` fields of a Soroban event.
///
/// Returns `true` if the event passes XDR validation, `false` if it should be skipped.
/// On failure, logs a WARN and increments `soroban_pulse_events_xdr_invalid_total`.
pub fn validate_xdr(
    tx_hash: &str,
    contract_id: &str,
    ledger: u64,
    value: &Value,
    topic: Option<&Vec<Value>>,
) -> bool {
    // Null value is acceptable (no XDR to validate)
    if !value.is_null() && !is_valid_sc_val(value) {
        warn!(
            tx_hash = %tx_hash,
            contract_id = %contract_id,
            ledger = ledger,
            raw_value = %value,
            "event_data.value failed XDR/ScVal validation, skipping event",
        );
        metrics::record_xdr_invalid();
        return false;
    }

    if let Some(topics) = topic {
        for (i, t) in topics.iter().enumerate() {
            if !is_valid_sc_val(t) {
                warn!(
                    tx_hash = %tx_hash,
                    contract_id = %contract_id,
                    ledger = ledger,
                    topic_index = i,
                    raw_topic = %t,
                    "event_data.topic[{}] failed XDR/ScVal validation, skipping event",
                    i,
                );
                metrics::record_xdr_invalid();
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(value: Value, topic: Option<Vec<Value>>) -> bool {
        validate_xdr("txhash", "contract1", 100, &value, topic.as_ref())
    }

    #[test]
    fn null_value_is_valid() {
        assert!(call(Value::Null, None));
    }

    #[test]
    fn valid_sc_val_void_passes() {
        // ScVal::Void serializes as {"void": null} in stellar-xdr serde
        let v = json!({"void": null});
        assert!(call(v, None));
    }

    #[test]
    fn valid_sc_val_bool_passes() {
        let v = json!({"bool": true});
        assert!(call(v, None));
    }

    #[test]
    fn invalid_value_fails() {
        // A plain string is not a valid ScVal
        let v = json!("not_a_scval");
        assert!(!call(v, None));
    }

    #[test]
    fn invalid_number_value_fails() {
        let v = json!(42);
        assert!(!call(v, None));
    }

    #[test]
    fn valid_topic_passes() {
        let v = Value::Null;
        let topic = vec![json!({"void": null}), json!({"bool": false})];
        assert!(call(v, Some(topic)));
    }

    #[test]
    fn invalid_topic_element_fails() {
        let v = Value::Null;
        let topic = vec![json!({"void": null}), json!("bad_topic")];
        assert!(!call(v, Some(topic)));
    }

    #[test]
    fn empty_topic_passes() {
        assert!(call(Value::Null, Some(vec![])));
    }
}
