//! `format=native|json|xdr` rendering for ScVal event data (#1064).
//!
//! Event `event_data` (and each `topic` entry) is stored as stellar-xdr's
//! serde JSON representation. Consumers can ask for three renderings:
//!
//! - `json` (default, backward compatible): the raw stellar-xdr serde JSON
//!   as stored.
//! - `native`: a friendly JSON shape with `i128`/`u128`/`i64`/`u64` as
//!   strings and addresses as strkeys, easy to consume without knowing the
//!   XDR type tags.
//! - `xdr`: the base64-encoded XDR bytes for the value.

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use stellar_xdr::curr::{Limits, ScVal, WriteXdr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScValFormat {
    /// stellar-xdr's own serde JSON representation (current default).
    #[default]
    Json,
    /// Friendly JSON: i128/u128/i64/u64 as strings, addresses as strkeys.
    Native,
    /// Base64-encoded raw XDR bytes.
    Xdr,
}

impl std::str::FromStr for ScValFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(ScValFormat::Json),
            "native" => Ok(ScValFormat::Native),
            "xdr" => Ok(ScValFormat::Xdr),
            other => Err(format!(
                "invalid format '{other}': expected one of native, json, xdr"
            )),
        }
    }
}

/// Render a stored ScVal JSON value (stellar-xdr serde JSON) in the
/// requested format. Falls back to returning the value unchanged if it does
/// not parse as an `ScVal` (e.g. already-decoded/anonymized event data).
pub fn render(value: &Value, format: ScValFormat) -> Value {
    match format {
        ScValFormat::Json => value.clone(),
        ScValFormat::Native => match serde_json::from_value::<ScVal>(value.clone()) {
            Ok(scval) => to_native(&scval),
            Err(_) => value.clone(),
        },
        ScValFormat::Xdr => match serde_json::from_value::<ScVal>(value.clone()) {
            Ok(scval) => match scval.to_xdr_base64(Limits::none()) {
                Ok(b64) => json!(b64),
                Err(_) => value.clone(),
            },
            Err(_) => value.clone(),
        },
    }
}

/// Render `event_data.value` and every `event_data.topic` entry using the
/// requested format, leaving other fields untouched.
pub fn render_event_data(event_data: &Value, format: ScValFormat) -> Value {
    if format == ScValFormat::Json {
        return event_data.clone();
    }
    let mut out = event_data.clone();
    if let Some(obj) = out.as_object_mut() {
        if let Some(v) = obj.get("value") {
            obj.insert("value".to_string(), render(v, format));
        }
        if let Some(Value::Array(topics)) = obj.get("topic").cloned() {
            let rendered: Vec<Value> = topics.iter().map(|t| render(t, format)).collect();
            obj.insert("topic".to_string(), Value::Array(rendered));
        }
    }
    out
}

fn to_native(value: &ScVal) -> Value {
    match value {
        ScVal::Bool(v) => json!(*v),
        ScVal::Void => Value::Null,
        ScVal::U32(v) => json!(*v),
        ScVal::I32(v) => json!(*v),
        ScVal::U64(v) => json!(v.to_string()),
        ScVal::I64(v) => json!(v.to_string()),
        ScVal::Timepoint(t) => json!(t.0.to_string()),
        ScVal::Duration(d) => json!(d.0.to_string()),
        ScVal::U128(v) => json!(((v.hi as u128) << 64 | v.lo as u128).to_string()),
        ScVal::I128(v) => {
            let combined = ((v.hi as i128) << 64) | (v.lo as i128);
            json!(combined.to_string())
        }
        ScVal::U256(v) => json!(format!("{v:?}")),
        ScVal::I256(v) => json!(format!("{v:?}")),
        ScVal::Bytes(b) => json!(base64::engine::general_purpose::STANDARD.encode(b.as_slice())),
        ScVal::String(s) => json!(s.to_string()),
        ScVal::Symbol(s) => json!(s.to_string()),
        ScVal::Vec(Some(vals)) => Value::Array(vals.0.iter().map(to_native).collect()),
        ScVal::Vec(None) => Value::Array(Vec::new()),
        ScVal::Map(Some(entries)) => {
            let mut obj = serde_json::Map::new();
            for entry in entries.0.iter() {
                let key = match to_native(&entry.key) {
                    Value::String(s) => s,
                    other => other.to_string(),
                };
                obj.insert(key, to_native(&entry.val));
            }
            Value::Object(obj)
        }
        ScVal::Map(None) => Value::Object(serde_json::Map::new()),
        ScVal::Address(addr) => json!(addr.to_string()),
        other => serde_json::to_value(other).unwrap_or(Value::Null),
    }
}

/// Parse a `format` query parameter, defaulting to `json` when absent so
/// existing callers keep their current behaviour.
pub fn parse_format(raw: Option<&str>) -> Result<ScValFormat, String> {
    match raw {
        None => Ok(ScValFormat::Json),
        Some(s) => s.parse(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn json_format_is_identity() {
        let v = json!({"i128": {"hi": 0, "lo": 42}});
        assert_eq!(render(&v, ScValFormat::Json), v);
    }

    #[test]
    fn native_renders_bool_and_void() {
        assert_eq!(render(&serde_json::to_value(ScVal::Bool(true)).unwrap(), ScValFormat::Native), json!(true));
        assert_eq!(render(&serde_json::to_value(ScVal::Void).unwrap(), ScValFormat::Native), Value::Null);
    }

    #[test]
    fn parse_format_rejects_unknown() {
        assert!(parse_format(Some("yaml")).is_err());
    }

    #[test]
    fn parse_format_defaults_to_json() {
        assert_eq!(parse_format(None).unwrap(), ScValFormat::Json);
    }

    proptest! {
        /// Issue #1064 acceptance criterion: xdr -> json -> xdr round-trips
        /// losslessly for simple scalar ScVals.
        #[test]
        fn xdr_json_xdr_round_trips(n in any::<i32>()) {
            let scval = ScVal::I32(n);
            let xdr_b64 = scval.to_xdr_base64(Limits::none()).unwrap();
            let json_val = serde_json::to_value(&scval).unwrap();
            let scval_back: ScVal = serde_json::from_value(json_val).unwrap();
            let xdr_b64_back = scval_back.to_xdr_base64(Limits::none()).unwrap();
            prop_assert_eq!(xdr_b64, xdr_b64_back);
        }
    }
}
