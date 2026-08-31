//! Subscription configuration validator.
//!
//! Validates subscription configurations before they are deployed, catching
//! malformed filters, unsupported schema mismatches, unsafe transformations
//! and resource limit violations ahead of runtime.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A single validation finding produced by a rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub path: Option<String>,
}

impl ValidationFinding {
    pub fn error(rule: &str, message: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            severity: Severity::Error,
            message: message.into(),
            path: None,
        }
    }

    pub fn warning(rule: &str, message: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            severity: Severity::Warning,
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
}

/// The subscription configuration under validation. This is intentionally a
/// loosely-typed subset of the real subscription schema so this tool can be
/// run against raw JSON configs before they are parsed by the runtime.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionConfig {
    pub name: String,
    pub filter: Option<String>,
    pub schema_version: Option<String>,
    pub transform: Option<String>,
    pub max_events_per_second: Option<u64>,
    pub max_payload_bytes: Option<u64>,
    pub max_concurrent_deliveries: Option<u32>,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// A validation rule that inspects a `SubscriptionConfig` and produces zero
/// or more findings.
pub trait ValidationRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding>;
}

/// Result of a full validation run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationReport {
    pub subscription: String,
    pub findings: Vec<ValidationFinding>,
}

impl ValidationReport {
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationFinding> {
        self.findings.iter().filter(|f| f.severity == Severity::Error)
    }
}

/// The rule engine that runs a collection of `ValidationRule`s against a
/// subscription configuration and aggregates their findings.
pub struct ValidationEngine {
    rules: Vec<Box<dyn ValidationRule>>,
}

impl Default for ValidationEngine {
    fn default() -> Self {
        Self::with_default_rules()
    }
}

impl ValidationEngine {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Construct an engine pre-populated with all built-in rules.
    pub fn with_default_rules() -> Self {
        let mut engine = Self::new();
        engine.register(Box::new(FilterSyntaxRule));
        engine.register(Box::new(PerformanceRule));
        engine.register(Box::new(SchemaCompatibilityRule));
        engine.register(Box::new(TransformationRule));
        engine.register(Box::new(ResourceLimitRule));
        engine
    }

    pub fn register(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    pub fn validate(&self, config: &SubscriptionConfig) -> ValidationReport {
        let mut findings = Vec::new();
        for rule in &self.rules {
            findings.extend(rule.validate(config));
        }
        ValidationReport {
            subscription: config.name.clone(),
            findings,
        }
    }

    pub fn validate_all(&self, configs: &[SubscriptionConfig]) -> Vec<ValidationReport> {
        configs.iter().map(|c| self.validate(c)).collect()
    }
}

/// Validates the filter expression syntax (balanced parens/brackets and
/// known operator tokens).
pub struct FilterSyntaxRule;

impl ValidationRule for FilterSyntaxRule {
    fn name(&self) -> &'static str {
        "filter_syntax"
    }

    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        let mut findings = Vec::new();
        let Some(filter) = &config.filter else {
            return findings;
        };

        if filter.trim().is_empty() {
            findings.push(
                ValidationFinding::error(self.name(), "filter expression is empty")
                    .with_path("filter"),
            );
            return findings;
        }

        let mut depth = 0i32;
        for ch in filter.chars() {
            match ch {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                findings.push(
                    ValidationFinding::error(self.name(), "unbalanced brackets in filter")
                        .with_path("filter"),
                );
                return findings;
            }
        }
        if depth != 0 {
            findings.push(
                ValidationFinding::error(self.name(), "unbalanced brackets in filter")
                    .with_path("filter"),
            );
        }

        const KNOWN_OPERATORS: &[&str] = &["==", "!=", ">=", "<=", ">", "<", "&&", "||", "in", "contains"];
        if filter.contains("=") && !KNOWN_OPERATORS.iter().any(|op| filter.contains(op)) {
            findings.push(ValidationFinding::warning(
                self.name(),
                "filter uses '=' — did you mean '=='?",
            ));
        }

        findings
    }
}

/// Flags filter/transform combinations that are likely to be slow at scale.
pub struct PerformanceRule;

impl ValidationRule for PerformanceRule {
    fn name(&self) -> &'static str {
        "performance"
    }

    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        let mut findings = Vec::new();

        if let Some(filter) = &config.filter {
            let wildcard_count = filter.matches('*').count();
            if wildcard_count > 2 {
                findings.push(ValidationFinding::warning(
                    self.name(),
                    format!(
                        "filter contains {wildcard_count} wildcards; broad wildcard filters degrade throughput"
                    ),
                ));
            }
            if filter.to_lowercase().contains("contains") && config.fields.len() > 10 {
                findings.push(ValidationFinding::warning(
                    self.name(),
                    "'contains' filter combined with a large field selection may be slow to evaluate",
                ));
            }
        }

        if config.fields.is_empty() {
            findings.push(ValidationFinding::warning(
                self.name(),
                "no field projection specified; full-payload delivery increases bandwidth cost",
            ));
        }

        findings
    }
}

/// Checks that the configured schema version is one this deployment knows
/// how to serve.
pub struct SchemaCompatibilityRule;

const SUPPORTED_SCHEMA_VERSIONS: &[&str] = &["v1", "v2", "v3"];

impl ValidationRule for SchemaCompatibilityRule {
    fn name(&self) -> &'static str {
        "schema_compatibility"
    }

    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        let mut findings = Vec::new();
        match &config.schema_version {
            None => findings.push(ValidationFinding::warning(
                self.name(),
                "no schema_version specified; defaulting to latest may break on upgrade",
            )),
            Some(v) if !SUPPORTED_SCHEMA_VERSIONS.contains(&v.as_str()) => {
                findings.push(
                    ValidationFinding::error(
                        self.name(),
                        format!(
                            "schema_version '{v}' is not supported (supported: {SUPPORTED_SCHEMA_VERSIONS:?})"
                        ),
                    )
                    .with_path("schema_version"),
                );
            }
            _ => {}
        }
        findings
    }
}

/// Validates transformation scripts for obviously unsafe or unsupported
/// constructs before they reach the runtime interpreter.
pub struct TransformationRule;

impl ValidationRule for TransformationRule {
    fn name(&self) -> &'static str {
        "transformation"
    }

    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        let mut findings = Vec::new();
        let Some(transform) = &config.transform else {
            return findings;
        };

        const BANNED_TOKENS: &[&str] = &["os.", "io.open", "require(", "eval(", "process."];
        for token in BANNED_TOKENS {
            if transform.contains(token) {
                findings.push(
                    ValidationFinding::error(
                        self.name(),
                        format!("transform script uses disallowed construct '{token}'"),
                    )
                    .with_path("transform"),
                );
            }
        }

        if transform.len() > 10_000 {
            findings.push(ValidationFinding::warning(
                self.name(),
                "transform script exceeds 10KB; consider simplifying",
            ));
        }

        findings
    }
}

/// Checks configured resource limits are present and within sane bounds.
pub struct ResourceLimitRule;

impl ValidationRule for ResourceLimitRule {
    fn name(&self) -> &'static str {
        "resource_limits"
    }

    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        let mut findings = Vec::new();

        match config.max_events_per_second {
            None => findings.push(ValidationFinding::warning(
                self.name(),
                "max_events_per_second not set; subscription is unbounded",
            )),
            Some(v) if v == 0 => {
                findings.push(ValidationFinding::error(
                    self.name(),
                    "max_events_per_second must be greater than zero",
                ));
            }
            Some(v) if v > 100_000 => {
                findings.push(ValidationFinding::warning(
                    self.name(),
                    format!("max_events_per_second={v} is unusually high"),
                ));
            }
            _ => {}
        }

        if let Some(bytes) = config.max_payload_bytes {
            if bytes > 10 * 1024 * 1024 {
                findings.push(ValidationFinding::warning(
                    self.name(),
                    "max_payload_bytes exceeds 10MB; consider payload compression",
                ));
            }
        }

        if let Some(concurrency) = config.max_concurrent_deliveries {
            if concurrency == 0 {
                findings.push(ValidationFinding::error(
                    self.name(),
                    "max_concurrent_deliveries must be greater than zero",
                ));
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> SubscriptionConfig {
        SubscriptionConfig {
            name: "test-sub".into(),
            filter: Some("type == \"payment\"".into()),
            schema_version: Some("v2".into()),
            transform: Some("event.amount".into()),
            max_events_per_second: Some(100),
            max_payload_bytes: Some(1024),
            max_concurrent_deliveries: Some(4),
            fields: vec!["amount".into(), "type".into()],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn valid_config_has_no_errors() {
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&base_config());
        assert!(!report.has_errors(), "unexpected errors: {:?}", report.findings);
    }

    #[test]
    fn empty_filter_is_error() {
        let mut config = base_config();
        config.filter = Some("".into());
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(report.has_errors());
    }

    #[test]
    fn unbalanced_brackets_detected() {
        let mut config = base_config();
        config.filter = Some("(type == \"payment\"".into());
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(report.has_errors());
    }

    #[test]
    fn unsupported_schema_version_is_error() {
        let mut config = base_config();
        config.schema_version = Some("v99".into());
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(report.has_errors());
    }

    #[test]
    fn banned_transform_token_is_error() {
        let mut config = base_config();
        config.transform = Some("require('fs')".into());
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(report.has_errors());
    }

    #[test]
    fn zero_rate_limit_is_error() {
        let mut config = base_config();
        config.max_events_per_second = Some(0);
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(report.has_errors());
    }

    #[test]
    fn missing_rate_limit_is_warning_not_error() {
        let mut config = base_config();
        config.max_events_per_second = None;
        let engine = ValidationEngine::with_default_rules();
        let report = engine.validate(&config);
        assert!(!report.has_errors());
        assert!(report.findings.iter().any(|f| f.severity == Severity::Warning));
    }

    #[test]
    fn validate_all_runs_across_configs() {
        let engine = ValidationEngine::with_default_rules();
        let configs = vec![base_config(), base_config()];
        let reports = engine.validate_all(&configs);
        assert_eq!(reports.len(), 2);
    }

    #[test]
    fn custom_rule_can_be_registered() {
        struct AlwaysFails;
        impl ValidationRule for AlwaysFails {
            fn name(&self) -> &'static str {
                "always_fails"
            }
            fn validate(&self, _config: &SubscriptionConfig) -> Vec<ValidationFinding> {
                vec![ValidationFinding::error("always_fails", "boom")]
            }
        }

        let mut engine = ValidationEngine::new();
        engine.register(Box::new(AlwaysFails));
        let report = engine.validate(&base_config());
        assert!(report.has_errors());
    }
}
