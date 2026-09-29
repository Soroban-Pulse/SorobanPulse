use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};

use crate::client::ApiClient;

// ---------------------------------------------------------------------------
// `spulse tail` (Issue #1133)
//
// Streams /v1/events/stream and prints each event as one compact JSON line
// (NDJSON), flushed immediately so `spulse tail ... | jq .ledger` is live.
// ---------------------------------------------------------------------------

/// One dispatched Server-Sent Events message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseMessage {
    pub id: Option<String>,
    pub event: String,
    pub data: String,
}

/// Line-oriented SSE parser: `data:` lines are joined with `\n`, `:` lines are
/// keep-alive comments, a blank line dispatches, `retry:` and unknown fields
/// are ignored.
#[derive(Debug, Default)]
pub struct SseParser {
    data: Vec<String>,
    event: String,
    id: Option<String>,
    /// Most recent `id:` seen, for `Last-Event-ID` resume.
    pub last_event_id: Option<String>,
}

impl SseParser {
    /// Feed one line (with or without its line terminator).
    pub fn push_line(&mut self, line: &str) -> Option<SseMessage> {
        let line = line.trim_end_matches('\n').trim_end_matches('\r');
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "data" => self.data.push(value.to_string()),
            "event" => self.event = value.to_string(),
            "id" if !value.contains('\0') => self.id = Some(value.to_string()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseMessage> {
        if self.id.is_some() {
            self.last_event_id.clone_from(&self.id);
        }
        let message = (!self.data.is_empty()).then(|| SseMessage {
            id: self.id.clone(),
            event: if self.event.is_empty() {
                "message".into()
            } else {
                self.event.clone()
            },
            data: self.data.join("\n"),
        });
        self.data.clear();
        self.event.clear();
        self.id = None;
        message
    }
}

/// Copy events from an SSE body to `out` as NDJSON. Frames whose data is not
/// JSON are skipped. Returns the number of events written; a closed pipe on
/// `out` (e.g. `| head`) ends the stream quietly.
pub fn pipe_ndjson<R: Read, W: Write>(body: R, out: &mut W) -> Result<u64> {
    let mut reader = BufReader::new(body);
    let mut parser = SseParser::default();
    let mut line = String::new();
    let mut written = 0;
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .context("reading event stream")?
            == 0
        {
            return Ok(written);
        }
        let Some(message) = parser.push_line(&line) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(&message.data) else {
            continue;
        };

        let result = writeln!(out, "{event}").and_then(|_| out.flush());
        match result {
            Ok(()) => written += 1,
            Err(e) if e.kind() == ErrorKind::BrokenPipe => return Ok(written),
            Err(e) => return Err(e).context("writing event"),
        }
    }
}

pub fn run(client: &ApiClient, contract: &str, event_type: Option<&str>) -> Result<u64> {
    let mut params = vec![("contract_id", contract)];
    if let Some(t) = event_type {
        params.push(("event_type", t));
    }
    let resp = client.stream("/v1/events/stream", &params)?;
    pipe_ndjson(resp, &mut std::io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> (Vec<SseMessage>, SseParser) {
        let mut parser = SseParser::default();
        let messages = input
            .split_inclusive('\n')
            .filter_map(|l| parser.push_line(l))
            .collect();
        (messages, parser)
    }

    #[test]
    fn parses_messages_and_skips_keepalives() {
        let (messages, parser) = parse(": ka\n\nid: 1\r\ndata: {\"a\":1}\r\n\r\nevent: lag\nretry: 5\ndata: x\ndata:y\n\nid: 9\n\n");
        assert_eq!(
            messages,
            vec![
                SseMessage {
                    id: Some("1".into()),
                    event: "message".into(),
                    data: "{\"a\":1}".into()
                },
                SseMessage {
                    id: None,
                    event: "lag".into(),
                    data: "x\ny".into()
                },
            ]
        );
        assert_eq!(parser.last_event_id.as_deref(), Some("9"));
    }

    #[test]
    fn pipes_json_events_as_ndjson() {
        let body = "id: 1\ndata: not json\n\n: keep-alive\n\ndata: {\"ledger\":6}\n\n";
        let mut out = Vec::new();
        let n = pipe_ndjson(body.as_bytes(), &mut out).unwrap();
        assert_eq!(n, 1);
        assert_eq!(String::from_utf8(out).unwrap(), "{\"ledger\":6}\n");
    }

    #[test]
    fn multi_line_data_is_joined_before_parsing() {
        let body = "data: {\"ledger\":\ndata: 7}\n\n";
        let mut out = Vec::new();
        assert_eq!(pipe_ndjson(body.as_bytes(), &mut out).unwrap(), 1);
        assert_eq!(String::from_utf8(out).unwrap(), "{\"ledger\":7}\n");
    }

    struct ClosedPipe;
    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn closed_stdout_ends_quietly() {
        assert_eq!(
            pipe_ndjson("data: {}\n\n".as_bytes(), &mut ClosedPipe).unwrap(),
            0
        );
    }
}
