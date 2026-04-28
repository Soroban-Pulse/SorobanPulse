use arrow_array::{
    ArrayRef, Int64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

pub struct EventRow {
    pub id: Uuid,
    pub contract_id: String,
    pub event_type: String,
    pub tx_hash: String,
    pub ledger: i64,
    pub timestamp: DateTime<Utc>,
    pub event_data: Value,
    pub created_at: DateTime<Utc>,
}

/// Serialize a slice of `EventRow` to Parquet bytes.
pub fn write_events_parquet(events: &[EventRow]) -> Result<Vec<u8>, parquet::errors::ParquetError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("contract_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("tx_hash", DataType::Utf8, false),
        Field::new("ledger", DataType::Int64, false),
        // Unix microseconds
        Field::new("timestamp_us", DataType::Int64, false),
        Field::new("event_data", DataType::Utf8, false),
        Field::new("created_at_us", DataType::Int64, false),
    ]));

    let ids: ArrayRef = Arc::new(StringArray::from(
        events.iter().map(|e| e.id.to_string()).collect::<Vec<_>>(),
    ));
    let contract_ids: ArrayRef = Arc::new(StringArray::from(
        events.iter().map(|e| e.contract_id.clone()).collect::<Vec<_>>(),
    ));
    let event_types: ArrayRef = Arc::new(StringArray::from(
        events.iter().map(|e| e.event_type.clone()).collect::<Vec<_>>(),
    ));
    let tx_hashes: ArrayRef = Arc::new(StringArray::from(
        events.iter().map(|e| e.tx_hash.clone()).collect::<Vec<_>>(),
    ));
    let ledgers: ArrayRef = Arc::new(Int64Array::from(
        events.iter().map(|e| e.ledger).collect::<Vec<_>>(),
    ));
    let timestamps: ArrayRef = Arc::new(Int64Array::from(
        events
            .iter()
            .map(|e| e.timestamp.timestamp_micros())
            .collect::<Vec<_>>(),
    ));
    let event_datas: ArrayRef = Arc::new(StringArray::from(
        events
            .iter()
            .map(|e| e.event_data.to_string())
            .collect::<Vec<_>>(),
    ));
    let created_ats: ArrayRef = Arc::new(Int64Array::from(
        events
            .iter()
            .map(|e| e.created_at.timestamp_micros())
            .collect::<Vec<_>>(),
    ));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            ids, contract_ids, event_types, tx_hashes, ledgers, timestamps, event_datas,
            created_ats,
        ],
    )
    .map_err(|e| parquet::errors::ParquetError::General(e.to_string()))?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, schema, Some(props))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(buf)
}
