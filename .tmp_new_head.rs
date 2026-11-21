//! Lua scripting hook for event transformation
//!
//! This module provides optional Lua scripting support for transforming events
//! before they are stored in the database. Scripts can modify event data or
//! return nil to skip events entirely.
//!
//! ## Resource limits
//!
//! Every script execution is bounded by two independent limits:
//!
//! * an **instruction limit** (`DEFAULT_LUA_INSTRUCTION_LIMIT`, configurable via
//!   `EVENT_TRANSFORM_INSTRUCTION_LIMIT`), and
//! * a **wall-clock deadline** (`EVENT_TRANSFORM_TIMEOUT_MS`).
//!
//! Both are enforced by an `mlua` instruction-count hook installed before the
//! script runs. When either limit is exceeded the hook returns an error, which
//! aborts the script *inside* the Lua VM. This is what actually terminates
//! runaway scripts such as `while true do end`; the surrounding
//! `tokio::time::timeout` is only a backstop, because Tokio cannot cancel a
//! `spawn_blocking` task once it has started.

use crate::models::{LuaPreviewItem, SorobanEvent};
use crate::metrics;
use anyhow::{Context, Result};
use mlua::{Lua, Table, Value as LuaValue};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// Default maximum number of Lua VM instructions a single transformation may
/// execute before it is aborted.
pub const DEFAULT_LUA_INSTRUCTION_LIMIT: u64 = 1_000_000;

/// Number of instructions between hook invocations. Smaller values give more
/// precise limits at the cost of more hook overhead.
const LUA_HOOK_INTERVAL: u32 = 10_000;

const LUA_MEMORY_LIMIT_MB: usize = 64;

/// Lua script executor for event transformation
pub struct LuaTransformer {
    lua: Arc<Lua>,
    timeout: Duration,
    instruction_limit: u64,
    script_path: String,
}

impl LuaTransformer {
    /// Create a new LuaTransformer by loading a script from the given path.
    ///
    /// `timeout_ms` is the wall-clock deadline for a single transformation; the
    /// default instruction limit ([`DEFAULT_LUA_INSTRUCTION_LIMIT`]) is used.
    pub fn new(script_path: &Path, timeout_ms: u64) -> Result<Self> {
        Self::with_limits(script_path, timeout_ms, DEFAULT_LUA_INSTRUCTION_LIMIT)
    }

    /// Create a new LuaTransformer with explicit time and instruction limits.
    pub fn with_limits(
        script_path: &Path,
        timeout_ms: u64,
        instruction_limit: u64,
    ) -> Result<Self> {
        let lua = Lua::new();

        // Set memory limit (in bytes)
        lua.set_memory_limit(LUA_MEMORY_LIMIT_MB * 1024 * 1024)?;

        // Load the script
        let script_content = std::fs::read_to_string(script_path)
            .with_context(|| format!("Failed to read Lua script: {}", script_path.display()))?;

        lua.load(&script_content)
            .exec()
            .with_context(|| format!("Failed to execute Lua script: {}", script_path.display()))?;

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

    /// The configured wall-clock deadline for a single transformation.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The configured instruction limit for a single transformation.
    pub fn instruction_limit(&self) -> u64 {
        self.instruction_limit
    }

