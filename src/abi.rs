use serde_json::{json, Value};
use sqlx::PgPool;
use stellar_xdr::curr::ScVal;

pub fn decode_event_data(abi: &Value, event_data: &Value) -> Option<Value> {
    let event_name = event_name_from_topic(event_data.get("topic")?)?;
    let def = abi
        .as_array()?
        .iter()
        .find(|e| e.get("name").and_then(Value::as_str) == Some(event_name))?;
    let inputs = def.get("inputs")?.as_array()?;
    let values = decode_values(event_data.get("value")?);

    let mut decoded = serde_json::Map::new();
    decoded.insert("event".to_string(), Value::String(event_name.to_string()));
    for (i, input) in inputs.iter().enumerate() {
        let name = input
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("field_{i}"));
        let value = values
            .get(&name)
            .or_else(|| values.get(&i.to_string()))
            .cloned()
            .unwrap_or(Value::Null);
        decoded.insert(name, value);
    }

    Some(Value::Object(decoded))
}

pub async fn decode_event_with_registered_abi(
    pool: &PgPool,
    contract_id: &str,
    event_data: &Value,
) -> Option<Value> {
    let abi: Value = sqlx::query_scalar("SELECT abi FROM contract_abis WHERE contract_id = $1")
        .bind(contract_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    decode_event_data(&abi, event_data)
}

pub async fn decode_existing_events(pool: PgPool, contract_id: String, abi: Value) {
    let rows = match sqlx::query("SELECT id, event_data FROM events WHERE contract_id = $1")
        .bind(&contract_id)
        .fetch_all(&pool)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(contract_id = %contract_id, error = %e, "failed to load events for ABI backfill");
            return;
        }
    };

    for row in rows {
        use sqlx::Row;
        let id: uuid::Uuid = match row.try_get("id") {
            Ok(id) => id,
            Err(_) => continue,
        };
        let event_data: Value = match row.try_get("event_data") {
            Ok(event_data) => event_data,
            Err(_) => continue,
        };
        let Some(decoded) = decode_event_data(&abi, &event_data) else {
            continue;
        };
        if let Err(e) = sqlx::query("UPDATE events SET event_data_decoded = $1 WHERE id = $2")
            .bind(decoded)
            .bind(id)
            .execute(&pool)
            .await
        {
            tracing::error!(event_id = %id, error = %e, "failed to backfill decoded event data");
        }
    }
}

fn event_name_from_topic(topic: &Value) -> Option<&str> {
    let first = topic.as_array()?.first()?;
    first
        .as_str()
        .or_else(|| first.get("symbol").and_then(Value::as_str))
        .or_else(|| first.get("string").and_then(Value::as_str))
}

fn decode_values(value: &Value) -> serde_json::Map<String, Value> {
    if let Some(obj) = value.as_object() {
        if obj.contains_key("vec") {
            if let Ok(ScVal::Vec(Some(vec))) = serde_json::from_value::<ScVal>(value.clone()) {
                return vec
                    .0
                    .to_vec()
                    .into_iter()
                    .enumerate()
                    .map(|(i, val)| (i.to_string(), scval_to_json(val)))
                    .collect();
            }
        }
        return obj.clone();
    }

    let mut map = serde_json::Map::new();
    map.insert("0".to_string(), decode_scalar(value));
    map
}

fn decode_scalar(value: &Value) -> Value {
    serde_json::from_value::<ScVal>(value.clone())
        .map(scval_to_json)
        .unwrap_or_else(|_| value.clone())
}

fn scval_to_json(value: ScVal) -> Value {
    match value {
        ScVal::Bool(v) => json!(v),
        ScVal::Void => Value::Null,
        ScVal::U32(v) => json!(v),
        ScVal::I32(v) => json!(v),
        ScVal::U64(v) => json!(v.to_string()),
        ScVal::I64(v) => json!(v.to_string()),
        ScVal::Symbol(v) => json!(v.to_string()),
        ScVal::String(v) => json!(v.to_string()),
        ScVal::Vec(Some(values)) => {
            Value::Array(values.0.to_vec().into_iter().map(scval_to_json).collect())
        }
        ScVal::Vec(None) => Value::Array(Vec::new()),
        other => serde_json::to_value(other).unwrap_or(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_named_fields_from_registered_abi() {
        let abi = json!([
            {"name": "transfer", "inputs": [
                {"name": "from", "type": "address"},
                {"name": "to", "type": "address"},
                {"name": "amount", "type": "i128"}
            ]}
        ]);
        let event_data = json!({
            "topic": ["transfer"],
            "value": {"from": "GABC", "to": "GDEF", "amount": "1000"}
        });

        let decoded = decode_event_data(&abi, &event_data).unwrap();
        assert_eq!(decoded["event"], "transfer");
        assert_eq!(decoded["amount"], "1000");
    }
}
