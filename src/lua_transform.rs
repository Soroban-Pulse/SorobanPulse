//! Lua scripting hook for event transformation
//!
//! This module provides optional Lua scripting support for transforming events
//! before they are stored in the database. Scripts can modify event data or
//! return nil to skip events entirely.

use crate::models::{LuaPreviewItem, SorobanEvent};
use crate::metrics;
use anyhow::{Context, Result};
use mlua::{Lua, LuaOptions, StdLib, Table, Value as LuaValue};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Lua script executor for event transformation
pub struct LuaTransformer {
    lua: Arc<Lua>,
    timeout: Duration,
    instruction_limit: u64,
    script_path: String,
}

const LUA_INSTRUCTION_LIMIT: u64 = 1_000_000;
const LUA_MEMORY_LIMIT_MB: usize = 64;
/// How often the instruction hook fires. Smaller values give tighter
/// enforcement of the deadline at the cost of a little overhead.
const LUA_HOOK_INTERVAL: u32 = 10_000;

/// Standard libraries exposed to user scripts. Notably this excludes `os`,
/// `io`, `package`, `debug` and `coroutine`.
fn sandbox_stdlib() -> StdLib {
    StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8
}

/// Build a sandboxed Lua state: restricted stdlib, memory limit, and the
/// dangerous globals removed from the global table.
fn create_sandboxed_lua() -> Result<Lua> {
    let lua = Lua::new_with(sandbox_stdlib(), LuaOptions::default())
        .context("failed to create sandboxed Lua state")?;

    // Set memory limit (in bytes)
    lua.set_memory_limit(LUA_MEMORY_LIMIT_MB * 1024 * 1024)?;

    // Remove escape hatches from the global environment.
    let globals = lua.globals();
    for name in [
        "load",
        "loadstring",
        "dofile",
        "loadfile",
        "require",
        "collectgarbage",
    ] {
        globals.set(name, LuaValue::Nil)?;
    }

    // `string.dump` can be used to produce bytecode that `load` would accept;
    // remove it as defense in depth.
    if let Ok(string_lib) = globals.get::<_, Table>("string") {
        string_lib.set("dump", LuaValue::Nil)?;
    }

    Ok(lua)
}

/// Build the sandboxed `_ENV` table used to run user code. It contains only an
/// explicit whitelist of safe globals.
fn create_sandbox_env(lua: &Lua) -> Result<Table> {
    let env = lua.create_table()?;
    let globals = lua.globals();

    // Whitelisted standard library tables.
    for name in ["string", "table", "math", "utf8"] {
        if let Ok(lib) = globals.get::<_, Table>(name) {
            env.set(name, lib)?;
        }
    }

    // Whitelisted safe globals.
    for name in ["assert", "error", "ipairs", "next", "pairs", "pcall", "select", "tonumber", "tostring", "type", "xpcall"] {
        if let Ok(v) = globals.get::<_, LuaValue>(name) {
            if !matches!(v, LuaValue::Nil) {
                env.set(name, v)?;
            }
        }
    }

    Ok(env)
}

/// Install an instruction-count hook that enforces both an instruction budget
/// and a wall-clock deadline. Returns `Err` from the hook when either limit is
/// exceeded, which aborts the running script.
fn install_limits(lua: &Lua, instruction_limit: u64, deadline: Instant) -> Result<()> {
    let mut executed: u64 = 0;
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
        move |_lua, _debug| {
            executed = executed.saturating_add(LUA_HOOK_INTERVAL as u64);
            if executed > instruction_limit {
                return Err(mlua::Error::RuntimeError("limit exceeded".to_string()));
            }
            if Instant::now() >= deadline {
                return Err(mlua::Error::RuntimeError("limit exceeded".to_string()));
            }
            Ok(mlua::VmState::Continue)
        },
    )?;
    Ok(())
}

/// Globals that are explicitly removed from the interpreter before any user
/// script runs. `os`/`io`/`package`/`debug` are never loaded (see
/// [`new_sandboxed_lua`]), but the remaining names are nil'd out defensively.
const REMOVED_GLOBALS: &[&str] = &[
    "load",
    "loadstring",
    "dofile",
    "loadfile",
    "require",
    "collectgarbage",
    "os",
    "io",
    "package",
    "debug",
];

/// The only globals exposed to user scripts through the sandboxed `_ENV`.
///
/// Issue #1423: admin-supplied Lua must not be able to reach the host
/// environment, so user code runs with an explicit whitelist instead of the
/// full global table.
const SANDBOX_GLOBALS: &[&str] = &[
    "string",
    "table",
    "math",
    "utf8",
    "pairs",
    "ipairs",
    "next",
    "type",
    "tostring",
    "tonumber",
    "error",
    "assert",
    "select",
    "pcall",
    "xpcall",
    "setmetatable",
    "getmetatable",
    "rawget",
    "rawset",
    "rawequal",
    "rawlen",
    "unpack",
];

/// Build a sandboxed Lua interpreter plus the restricted `_ENV` table that
/// user scripts must run in.
///
/// Issue #1423: `Lua::new()` loads mlua's "safe" stdlib set, which still
/// includes `os` and `io` — allowing `os.execute`/`io.popen` and therefore
/// remote code execution from the admin Lua endpoints. We instead load only
/// the pure-data libraries, remove every escape hatch, and hand back a
/// whitelist `_ENV` table for `Chunk::set_environment`.
fn new_sandboxed_lua() -> Result<(Lua, Table<'static>)> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .context("Failed to create sandboxed Lua interpreter")?;

    // Set memory limit (in bytes)
    lua.set_memory_limit(LUA_MEMORY_LIMIT_MB * 1024 * 1024)?;

    let globals = lua.globals();

    // Remove dangerous globals (no-ops for libs that were never loaded).
    for name in REMOVED_GLOBALS {
        globals.set(*name, LuaValue::Nil)?;
    }

    // `string.dump` can emit bytecode; remove it even though `load` is gone.
    if let Ok(string_lib) = globals.get::<_, Table>("string") {
        string_lib.set("dump", LuaValue::Nil)?;
    }

    // Build the whitelist `_ENV` table from the (already sanitized) globals.
    let env = lua.create_table()?;
    for name in SANDBOX_GLOBALS {
        let value: LuaValue = globals.get(*name)?;
        if !matches!(value, LuaValue::Nil) {
            env.set(*name, value)?;
        }
    }

    Ok((lua, env))
}

impl LuaTransformer {
    /// Create a new LuaTransformer by loading a script from the given path
    pub fn new(script_path: &Path, timeout_ms: u64) -> Result<Self> {
        // Issue #1423: run user scripts in a sandboxed interpreter.
        let (lua, env) = new_sandboxed_lua()?;

        // Load the script
        let script_content = std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read Lua script: {}", script_path.display()))?;

        // Run the script body inside the sandboxed environment.
        let env = create_sandbox_env(&lua)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        install_limits(&lua, instruction_limit, deadline)?;
        lua.load(&script_content)
            .set_environment(env)
            .exec()
            .with_context(|| format!("Failed to execute Lua script: {}", script_path.display()))?;
        lua.remove_hook();

        debug!(
            script_path = %script_path.display(),
            timeout_ms = timeout_ms,
            instruction_limit = instruction_limit,
            memory_limit_mb = LUA_MEMORY_LIMIT_MB,
            "Lua transformer initialized"
        );

        Ok(Self {
            lua: Arc::new(lua),
            timeout: Duration::from_millis(timeout_ms),
            instruction_limit,
            script_path: script_path.display().to_string(),
        })
    }

    /// Transform an event using the loaded Lua script
    ///
    /// Returns:
    /// - Ok(Some(event)) if the event was transformed or passed through
    /// - Ok(None) if the script returned nil (skip the event)
    /// - Err if the transformation failed
    pub async fn transform(&self, event: SorobanEvent) -> Result<Option<SorobanEvent>> {
        let lua = self.lua.clone();
        let timeout = self.timeout;
        let instruction_limit = self.instruction_limit;
        let script_path = self.script_path.clone();

        // The instruction/time hook is the primary termination mechanism; the
        // async timeout is only a backstop. The hook fires inside the blocking
        // task and aborts the script, so the blocking thread is released.
        let result = tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking(move || {
                Self::transform_sync(&lua, event, instruction_limit, timeout)
            }),
        )
        .await;

        match result {
            Ok(Ok(transformed)) => Ok(transformed),
            Ok(Err(e)) => {
                if Self::is_limit_error(&e) {
                    metrics::record_lua_timeout();
                }
                warn!(error = %e, script_path = %script_path, "Lua transformation failed");
                Err(e)
            }
            Err(_) => {
                warn!(timeout_ms = ?timeout.as_millis(), script_path = %script_path, "Lua script timeout");
                metrics::record_lua_timeout();
                Err(anyhow::anyhow!("Lua script execution timeout"))
            }
        }
    }

    /// Whether an error originated from the instruction/time limit hook.
    fn is_limit_error(err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            cause
                .downcast_ref::<mlua::Error>()
                .map(|e| matches!(e, mlua::Error::RuntimeError(msg) if msg == "limit exceeded"))
                .unwrap_or(false)
        })
    }

    /// Synchronous transformation logic
    fn transform_sync(
        lua: &Lua,
        event: SorobanEvent,
        instruction_limit: u64,
        timeout: Duration,
    ) -> Result<Option<SorobanEvent>> {
        // Install the instruction/time limit hook for this invocation.
        let deadline = Instant::now() + timeout;
        install_limits(lua, instruction_limit, deadline)?;

        // Get the transform_event function from the Lua script
        let transform_fn: mlua::Function = lua
            .globals()
            .get("transform_event")
            .context("Lua script must define a 'transform_event' function")?;

        // Convert the event to a Lua table
        let event_table = Self::event_to_lua_table(lua, &event)?;

        // Call the transform function
        let result: LuaValue = transform_fn
            .call(event_table)
            .context("Error calling transform_event function")?;

        // Remove the hook so it does not leak into subsequent invocations.
        lua.remove_hook();

        // Handle the result
        match result {
            LuaValue::Nil => {
                debug!(
                    tx_hash = %event.tx_hash,
                    contract_id = %event.contract_id,
                    "Event skipped by Lua script"
                );
                Ok(None)
            }
            LuaValue::Table(table) => {
                let transformed = Self::lua_table_to_event(lua, table, event)?;
                Ok(Some(transformed))
            }
            _ => Err(anyhow::anyhow!(
                "transform_event must return a table or nil, got {:?}",
                result.type_name()
            )),
        }
    }

    /// Convert a SorobanEvent to a Lua table
    fn event_to_lua_table<'lua>(lua: &'lua Lua, event: &SorobanEvent) -> Result<Table<'lua>> {
        let table = lua.create_table()?;

        table.set("contract_id", event.contract_id.clone())?;
        table.set("event_type", event.event_type.clone())?;
        table.set("tx_hash", event.tx_hash.clone())?;
        table.set("ledger", event.ledger)?;
        table.set("ledger_closed_at", event.ledger_closed_at.clone())?;

        if let Some(ref hash) = event.ledger_hash {
            table.set("ledger_hash", hash.clone())?;
        }

        table.set("in_successful_call", event.in_successful_call)?;

        // Convert JSON values to Lua values
        let value_lua = Self::json_to_lua(lua, &event.value)?;
        table.set("value", value_lua)?;

        if let Some(ref topic) = event.topic {
            let topic_table = lua.create_table()?;
            for (i, item) in topic.iter().enumerate() {
                let item_lua = Self::json_to_lua(lua, item)?;
                topic_table.set(i + 1, item_lua)?; // Lua arrays are 1-indexed
            }
            table.set("topic", topic_table)?;
        }

        Ok(table)
    }

    /// Convert a Lua table back to a SorobanEvent
    fn lua_table_to_event(lua: &Lua, table: Table, original: SorobanEvent) -> Result<SorobanEvent> {
        Ok(SorobanEvent {
            contract_id: table.get("contract_id").unwrap_or(original.contract_id),
            event_type: table.get("event_type").unwrap_or(original.event_type),
            tx_hash: table.get("tx_hash").unwrap_or(original.tx_hash),
            ledger: table.get("ledger").unwrap_or(original.ledger),
            ledger_closed_at: table
                .get("ledger_closed_at")
                .unwrap_or(original.ledger_closed_at),
            ledger_hash: table.get("ledger_hash").ok().or(original.ledger_hash),
            in_successful_call: table
                .get("in_successful_call")
                .unwrap_or(original.in_successful_call),
            value: Self::lua_to_json(lua, table.get("value")?)?,
            topic: match table.get::<_, LuaValue>("topic")? {
                LuaValue::Nil => original.topic,
                LuaValue::Table(topic_table) => {
                    let mut topic_vec = Vec::new();
                    for pair in topic_table.pairs::<usize, LuaValue>() {
                        let (_, value) = pair?;
                        topic_vec.push(Self::lua_to_json(lua, value)?);
                    }
                    Some(topic_vec)
                }
                _ => original.topic,
            },
        })
    }

    /// Convert a JSON value to a Lua value
    fn json_to_lua<'lua>(lua: &'lua Lua, value: &Value) -> Result<LuaValue<'lua>> {
        match value {
            Value::Null => Ok(LuaValue::Nil),
            Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(LuaValue::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(LuaValue::Number(f))
                } else {
                    Ok(LuaValue::Nil)
                }
            }
            Value::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
            Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, item) in arr.iter().enumerate() {
                    table.set(i + 1, Self::json_to_lua(lua, item)?)?;
                }
                Ok(LuaValue::Table(table))
            }
            Value::Object(obj) => {
                let table = lua.create_table()?;
                for (key, val) in obj.iter() {
                    table.set(key.as_str(), Self::json_to_lua(lua, val)?)?;
                }
                Ok(LuaValue::Table(table))
            }
        }
    }

    /// Convert a Lua value to a JSON value
    fn lua_to_json(_lua: &Lua, value: LuaValue) -> Result<Value> {
        match value {
            LuaValue::Nil => Ok(Value::Null),
            LuaValue::Boolean(b) => Ok(Value::Bool(b)),
            LuaValue::Integer(i) => Ok(Value::Number(i.into())),
            LuaValue::Number(n) => Ok(serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null)),
            LuaValue::String(s) => Ok(Value::String(s.to_str()?.to_string())),
            LuaValue::Table(table) => {
                // Check if it's an array or object
                let mut is_array = true;
                let mut max_index = 0;

                for pair in table.clone().pairs::<LuaValue, LuaValue>() {
                    let (key, _) = pair?;
                    if let LuaValue::Integer(i) = key {
                        if i > 0 {
                            max_index = max_index.max(i as usize);
                        } else {
                            is_array = false;
                            break;
                        }
                    } else {
                        is_array = false;
                        break;
                    }
                }

                if is_array && max_index > 0 {
                    let mut arr = Vec::new();
                    for i in 1..=max_index {
                        let val: LuaValue = table.get(i)?;
                        arr.push(Self::lua_to_json(_lua, val)?);
                    }
                    Ok(Value::Array(arr))
                } else {
                    let mut obj = serde_json::Map::new();
                    for pair in table.pairs::<LuaValue, LuaValue>() {
                        let (key, val) = pair?;
                        let key_str = match key {
                            LuaValue::String(s) => s.to_str()?.to_string(),
                            LuaValue::Integer(i) => i.to_string(),
                            LuaValue::Number(n) => n.to_string(),
                            _ => continue,
                        };
                        obj.insert(key_str, Self::lua_to_json(_lua, val)?);
                    }
                    Ok(Value::Object(obj))
                }
            }
            _ => Err(anyhow::anyhow!(
                "Unsupported Lua type: {}",
                value.type_name()
            )),
        }
    }

    /// Preview a Lua script against a list of events **without writing to the DB**.
    ///
    /// Applies the same CPU instruction limit, memory limit, and timeout as the
    /// production transformer. Each event is transformed independently; an error
    /// on one event does not abort the rest.
    ///
    /// Returns one [`LuaPreviewItem`] per input event containing the original
    /// event data, the transformed result (or `None` when the script returns
    /// `nil`), and any per-event error message.
    pub async fn preview_events(
        script: String,
        events: Vec<SorobanEvent>,
        timeout_ms: u64,
    ) -> Vec<LuaPreviewItem> {
        let timeout = Duration::from_millis(timeout_ms);
        let mut results = Vec::with_capacity(events.len());

        for event in events {
            let event_id = uuid::Uuid::new_v4(); // placeholder; caller should supply real IDs
            let original = serde_json::to_value(&event).unwrap_or(serde_json::Value::Null);
            let script_clone = script.clone();

            let outcome = tokio::time::timeout(
                timeout,
                tokio::task::spawn_blocking(move || {
                    Self::preview_event_sync(
                        &script_clone,
                        event,
                        LUA_INSTRUCTION_LIMIT,
                        timeout,
                    )
                }),
            )
            .await;

            let item = match outcome {
                Ok(Ok(Ok(Some(transformed)))) => LuaPreviewItem {
                    event_id,
                    original,
                    transformed: serde_json::to_value(&transformed).ok(),
                    error: None,
                },
                Ok(Ok(Ok(None))) => LuaPreviewItem {
                    event_id,
                    original,
                    transformed: None,
                    error: None,
                },
                Ok(Ok(Err(e))) => LuaPreviewItem {
                    event_id,
                    original,
                    transformed: None,
                    error: Some(e.to_string()),
                },
                Ok(Err(e)) => LuaPreviewItem {
                    event_id,
                    original,
                    transformed: None,
                    error: Some(format!("internal error: {e}")),
                },
                Err(_) => LuaPreviewItem {
                    event_id,
                    original,
                    transformed: None,
                    error: Some(format!(
                        "script timeout after {}ms",
                        timeout_ms
                    )),
                },
            };
            results.push(item);
        }

        results
    }

    /// Synchronous single-event preview used inside `spawn_blocking`.
    /// Compiles the script from a string (not a file) and applies the same limits.
    fn preview_event_sync(script: &str, event: SorobanEvent) -> Result<Option<SorobanEvent>> {
        // Issue #1423: previews use the same sandbox as production transforms.
        let (lua, env) = new_sandboxed_lua()?;
        lua.load(script)
            .set_environment(env)
            .exec()
            .context("failed to compile preview script")?;
        lua.remove_hook();
        Self::transform_sync(&lua, event, instruction_limit, timeout)
    }

    /// Wrap a bare script body into an implicit `transform_event` function
    /// definition, so callers of the replay-with-transform endpoint can pass
    /// a short expression (e.g. `"return event"`) instead of a full script
    /// that defines `transform_event` itself.
    pub fn wrap_script_body(body: &str) -> String {
        format!("function transform_event(event)\n{body}\nend")
    }

    /// Check that a (already-wrapped) script compiles as valid Lua, without
    /// executing it. Used to reject malformed scripts with a 400 before any
    /// events are touched.
    pub fn validate_syntax(script: &str) -> Result<(), String> {
        // Issue #1423: compile against the sandboxed interpreter so syntax
        // validation matches what will actually run.
        let (lua, env) = new_sandboxed_lua().map_err(|e| e.to_string())?;
        lua.load(script)
            .set_environment(env)
            .into_function()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_event() -> SorobanEvent {
        SorobanEvent {
            contract_id: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
            event_type: "contract".to_string(),
            tx_hash: "abc123".to_string(),
            ledger: 100,
            ledger_closed_at: "2024-01-01T00:00:00Z".to_string(),
            ledger_hash: Some("hash123".to_string()),
            in_successful_call: true,
            value: json!({"amount": 1000}),
            topic: Some(vec![json!("transfer")]),
        }
    }

    #[tokio::test]
    async fn test_passthrough_script() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event.clone()).await.unwrap();

        assert!(result.is_some());
        let transformed = result.unwrap();
        assert_eq!(transformed.contract_id, event.contract_id);
        assert_eq!(transformed.tx_hash, event.tx_hash);
    }

    #[tokio::test]
    async fn test_skip_event() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                return nil
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_modify_event() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                event.value.amount = event.value.amount * 2
                event.value.transformed = true
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_some());
        let transformed = result.unwrap();
        assert_eq!(transformed.value["amount"], 2000);
        assert_eq!(transformed.value["transformed"], true);
    }

    #[tokio::test]
    async fn test_filter_by_contract() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                if event.contract_id == "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM" then
                    return event
                else
                    return nil
                end
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 100).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await.unwrap();

        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                while true do end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 50).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_memory_limit() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                local t = {}
                for i = 1, 10000000 do
                    t[i] = string.rep("x", 1000)
                end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 5000).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_instruction_limit() {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(
            script_file,
            r#"
            function transform_event(event)
                local sum = 0
                for i = 1, 100000000 do
                    sum = sum + i
                end
                return event
            end
            "#
        )
        .unwrap();

        let transformer = LuaTransformer::new(script_file.path(), 5000).unwrap();
        let event = create_test_event();
        let result = transformer.transform(event).await;

        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Issue #1423: sandbox security regression tests
    // ------------------------------------------------------------------

    /// Run a script through the file-based transformer and return the result
    /// (which must be an error for escape attempts).
    async fn run_script(script: &str) -> Result<Option<SorobanEvent>> {
        let mut script_file = NamedTempFile::new().unwrap();
        writeln!(script_file, "{}", script).unwrap();
        let transformer = LuaTransformer::new(script_file.path(), 500).unwrap();
        transformer.transform(create_test_event()).await
    }

    #[tokio::test]
    async fn test_os_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                os.execute("echo pwned")
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "os.execute must not be reachable");

        let result = run_script(
            r#"
            function transform_event(event)
                event.value.t = os.time()
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "os.time must not be reachable");
    }

    #[tokio::test]
    async fn test_io_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                local f = io.open("/etc/passwd", "r")
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "io.open must not be reachable");
    }

    #[tokio::test]
    async fn test_require_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                require("os")
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "require must not be reachable");
    }

    #[tokio::test]
    async fn test_load_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                local f = load("return 1")
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "load must not be reachable");
    }

    #[tokio::test]
    async fn test_load_bytecode_rejected() {
        let result = run_script(
            r#"
            function transform_event(event)
                local f = load(string.dump(function() end))
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_err(), "bytecode loading must be rejected");
    }

    #[tokio::test]
    async fn test_package_and_debug_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                if package ~= nil then error("package exposed") end
                if debug ~= nil then error("debug exposed") end
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_ok(), "package/debug must be nil");
    }

    #[tokio::test]
    async fn test_string_dump_unavailable() {
        let result = run_script(
            r#"
            function transform_event(event)
                if string.dump ~= nil then error("string.dump exposed") end
                return event
            end
            "#,
        )
        .await;
        assert!(result.is_ok(), "string.dump must be nil");
    }

    #[tokio::test]
    async fn test_sandbox_whitelist_allows_normal_scripts() {
        let result = run_script(
            r#"
            function transform_event(event)
                local parts = {}
                for i, v in ipairs({ "a", "b" }) do
                    parts[i] = tostring(v)
                end
                local keys = {}
                for k, _ in pairs({ x = 1 }) do
                    keys[#keys + 1] = k
                end
                event.value.joined = table.concat(parts, "-")
                event.value.upper = string.upper("ok")
                event.value.sqrt = math.floor(math.sqrt(16))
                event.value.kind = type(event.value)
                event.value.first_key = keys[1]
                return event
            end
            "#,
        )
        .await
        .unwrap()
        .expect("script should transform the event");

        assert_eq!(result.value["joined"], "a-b");
        assert_eq!(result.value["upper"], "OK");
        assert_eq!(result.value["sqrt"], 4);
        assert_eq!(result.value["kind"], "table");
        assert_eq!(result.value["first_key"], "x");
    }

    #[tokio::test]
    async fn test_preview_rejects_os_execute() {
        let script = r#"
            function transform_event(event)
                os.execute("echo pwned")
                return event
            end
        "#;
        let items =
            LuaTransformer::preview_events(script.to_string(), vec![create_test_event()], 500).await;
        assert_eq!(items.len(), 1);
        assert!(items[0].error.is_some(), "preview must reject os.execute");
    }

    #[test]
    fn test_validate_syntax_uses_sandbox() {
        assert!(LuaTransformer::validate_syntax("function transform_event(e) return e end").is_ok());
        assert!(LuaTransformer::validate_syntax("function transform_event(e) return").is_err());
    }
}
