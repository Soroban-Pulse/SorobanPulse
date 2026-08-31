//! Client library code generators driven by an OpenAPI specification.
//!
//! Extends the [`crate::codegen`] module (previously scoped to subscription
//! scaffolding) with generators that turn a minimal in-memory OpenAPI model
//! into client library source for TypeScript, Python, Go, and Rust.

use serde::{Deserialize, Serialize};

/// A minimal representation of the parts of an OpenAPI spec these
/// generators care about: schemas (models) and operations (client methods).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenApiSpec {
    pub title: String,
    pub version: String,
    pub schemas: Vec<SchemaDef>,
    pub operations: Vec<OperationDef>,
}

/// A named object schema with a flat set of typed fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub ty: FieldType,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Array(Box<FieldType>),
    Ref(String),
}

/// An API operation (HTTP method + path) that becomes a client method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationDef {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub request_schema: Option<String>,
    pub response_schema: Option<String>,
}

/// A generated file: a relative path and its contents.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

/// Implemented by each per-language backend so `generate_all` can drive
/// every target uniformly.
pub trait ClientGenerator {
    fn language(&self) -> &'static str;
    fn generate_models(&self, spec: &OpenApiSpec) -> GeneratedFile;
    fn generate_client(&self, spec: &OpenApiSpec) -> GeneratedFile;

    fn generate(&self, spec: &OpenApiSpec) -> Vec<GeneratedFile> {
        vec![self.generate_models(spec), self.generate_client(spec)]
    }
}

fn method_name(operation_id: &str) -> String {
    let mut chars = operation_id.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// TypeScript
// ---------------------------------------------------------------------------

pub struct TypeScriptGenerator;

impl TypeScriptGenerator {
    fn ts_type(ty: &FieldType) -> String {
        match ty {
            FieldType::String => "string".into(),
            FieldType::Integer | FieldType::Number => "number".into(),
            FieldType::Boolean => "boolean".into(),
            FieldType::Array(inner) => format!("{}[]", Self::ts_type(inner)),
            FieldType::Ref(name) => name.clone(),
        }
    }
}

impl ClientGenerator for TypeScriptGenerator {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn generate_models(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = format!("// Generated from {} v{}\n\n", spec.title, spec.version);
        for schema in &spec.schemas {
            out.push_str(&format!("export interface {} {{\n", schema.name));
            for field in &schema.fields {
                let optional = if field.required { "" } else { "?" };
                out.push_str(&format!("  {}{}: {};\n", field.name, optional, Self::ts_type(&field.ty)));
            }
            out.push_str("}\n\n");
        }
        GeneratedFile { path: "models.ts".into(), contents: out }
    }

    fn generate_client(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = format!("import type * as Models from './models';\n\nexport class {}Client {{\n  constructor(private baseUrl: string) {{}}\n\n", spec.title.replace(' ', ""));
        for op in &spec.operations {
            let name = method_name(&op.operation_id);
            let ret = op.response_schema.clone().map(|s| format!("Models.{s}")).unwrap_or_else(|| "void".into());
            out.push_str(&format!(
                "  async {name}(): Promise<{ret}> {{\n    const res = await fetch(`${{this.baseUrl}}{path}`, {{ method: '{method}' }});\n    return res.json();\n  }}\n\n",
                name = name,
                ret = ret,
                path = op.path,
                method = op.method.to_uppercase(),
            ));
        }
        out.push_str("}\n");
        GeneratedFile { path: "client.ts".into(), contents: out }
    }
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

pub struct PythonGenerator;

impl PythonGenerator {
    fn py_type(ty: &FieldType) -> String {
        match ty {
            FieldType::String => "str".into(),
            FieldType::Integer => "int".into(),
            FieldType::Number => "float".into(),
            FieldType::Boolean => "bool".into(),
            FieldType::Array(inner) => format!("List[{}]", Self::py_type(inner)),
            FieldType::Ref(name) => name.clone(),
        }
    }
}

impl ClientGenerator for PythonGenerator {
    fn language(&self) -> &'static str {
        "python"
    }

    fn generate_models(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = format!(
            "# Generated from {} v{}\nfrom dataclasses import dataclass\nfrom typing import List, Optional\n\n",
            spec.title, spec.version
        );
        for schema in &spec.schemas {
            out.push_str("@dataclass\n");
            out.push_str(&format!("class {}:\n", schema.name));
            if schema.fields.is_empty() {
                out.push_str("    pass\n\n");
                continue;
            }
            for field in &schema.fields {
                let ty = Self::py_type(&field.ty);
                let ty = if field.required { ty } else { format!("Optional[{ty}]") };
                out.push_str(&format!("    {}: {}\n", field.name, ty));
            }
            out.push('\n');
        }
        GeneratedFile { path: "models.py".into(), contents: out }
    }

    fn generate_client(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = "import requests\nfrom . import models\n\n".to_string();
        out.push_str(&format!("class {}Client:\n    def __init__(self, base_url: str):\n        self.base_url = base_url\n\n", spec.title.replace(' ', "")));
        for op in &spec.operations {
            let name = to_snake_case(&op.operation_id);
            out.push_str(&format!(
                "    def {name}(self):\n        response = requests.request('{method}', f'{{self.base_url}}{path}')\n        return response.json()\n\n",
                name = name,
                method = op.method.to_uppercase(),
                path = op.path,
            ));
        }
        GeneratedFile { path: "client.py".into(), contents: out }
    }
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

pub struct GoGenerator;

impl GoGenerator {
    fn go_type(ty: &FieldType) -> String {
        match ty {
            FieldType::String => "string".into(),
            FieldType::Integer => "int64".into(),
            FieldType::Number => "float64".into(),
            FieldType::Boolean => "bool".into(),
            FieldType::Array(inner) => format!("[]{}", Self::go_type(inner)),
            FieldType::Ref(name) => name.clone(),
        }
    }
}

fn to_pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut cap_next = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            cap_next = true;
        } else if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

impl ClientGenerator for GoGenerator {
    fn language(&self) -> &'static str {
        "go"
    }

    fn generate_models(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = format!("// Generated from {} v{}\npackage client\n\n", spec.title, spec.version);
        for schema in &spec.schemas {
            out.push_str(&format!("type {} struct {{\n", schema.name));
            for field in &schema.fields {
                out.push_str(&format!(
                    "\t{} {} `json:\"{}\"`\n",
                    to_pascal_case(&field.name),
                    Self::go_type(&field.ty),
                    field.name
                ));
            }
            out.push_str("}\n\n");
        }
        GeneratedFile { path: "models.go".into(), contents: out }
    }

    fn generate_client(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let struct_name = format!("{}Client", to_pascal_case(&spec.title.replace(' ', "_")));
        let mut out = format!("package client\n\ntype {struct_name} struct {{\n\tBaseURL string\n}}\n\n");
        for op in &spec.operations {
            out.push_str(&format!(
                "func (c *{struct}) {method_name}() error {{\n\t// {http_method} {path}\n\treturn nil\n}}\n\n",
                struct = struct_name,
                method_name = to_pascal_case(&op.operation_id),
                http_method = op.method.to_uppercase(),
                path = op.path,
            ));
        }
        GeneratedFile { path: "client.go".into(), contents: out }
    }
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

pub struct RustGenerator;

impl RustGenerator {
    fn rust_type(ty: &FieldType) -> String {
        match ty {
            FieldType::String => "String".into(),
            FieldType::Integer => "i64".into(),
            FieldType::Number => "f64".into(),
            FieldType::Boolean => "bool".into(),
            FieldType::Array(inner) => format!("Vec<{}>", Self::rust_type(inner)),
            FieldType::Ref(name) => name.clone(),
        }
    }
}

impl ClientGenerator for RustGenerator {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn generate_models(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let mut out = format!(
            "// Generated from {} v{}\nuse serde::{{Deserialize, Serialize}};\n\n",
            spec.title, spec.version
        );
        for schema in &spec.schemas {
            out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
            out.push_str(&format!("pub struct {} {{\n", schema.name));
            for field in &schema.fields {
                let ty = Self::rust_type(&field.ty);
                let ty = if field.required { ty } else { format!("Option<{ty}>") };
                out.push_str(&format!("    pub {}: {},\n", field.name, ty));
            }
            out.push_str("}\n\n");
        }
        GeneratedFile { path: "models.rs".into(), contents: out }
    }

    fn generate_client(&self, spec: &OpenApiSpec) -> GeneratedFile {
        let struct_name = format!("{}Client", to_pascal_case(&spec.title.replace(' ', "_")));
        let mut out = format!(
            "pub struct {struct_name} {{\n    base_url: String,\n}}\n\nimpl {struct_name} {{\n    pub fn new(base_url: impl Into<String>) -> Self {{\n        Self {{ base_url: base_url.into() }}\n    }}\n\n"
        );
        for op in &spec.operations {
            out.push_str(&format!(
                "    pub async fn {name}(&self) -> Result<(), reqwest::Error> {{\n        // {method} {path}\n        Ok(())\n    }}\n\n",
                name = to_snake_case(&op.operation_id),
                method = op.method.to_uppercase(),
                path = op.path,
            ));
        }
        out.push_str("}\n");
        GeneratedFile { path: "client.rs".into(), contents: out }
    }
}

/// Runs every built-in language generator against a spec.
pub fn generate_all(spec: &OpenApiSpec) -> Vec<(&'static str, Vec<GeneratedFile>)> {
    let generators: Vec<Box<dyn ClientGenerator>> = vec![
        Box::new(TypeScriptGenerator),
        Box::new(PythonGenerator),
        Box::new(GoGenerator),
        Box::new(RustGenerator),
    ];
    generators.iter().map(|g| (g.language(), g.generate(spec))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec() -> OpenApiSpec {
        OpenApiSpec {
            title: "Pulse API".into(),
            version: "1.0.0".into(),
            schemas: vec![SchemaDef {
                name: "Event".into(),
                fields: vec![
                    FieldDef { name: "id".into(), ty: FieldType::String, required: true },
                    FieldDef { name: "amount".into(), ty: FieldType::Integer, required: false },
                    FieldDef { name: "topics".into(), ty: FieldType::Array(Box::new(FieldType::String)), required: true },
                ],
            }],
            operations: vec![OperationDef {
                operation_id: "listEvents".into(),
                method: "get".into(),
                path: "/events".into(),
                request_schema: None,
                response_schema: Some("Event".into()),
            }],
        }
    }

    #[test]
    fn typescript_generates_interface_and_client() {
        let files = TypeScriptGenerator.generate(&sample_spec());
        assert_eq!(files.len(), 2);
        assert!(files[0].contents.contains("export interface Event"));
        assert!(files[0].contents.contains("topics: string[]"));
        assert!(files[1].contents.contains("listEvents"));
    }

    #[test]
    fn typescript_marks_optional_fields() {
        let files = TypeScriptGenerator.generate(&sample_spec());
        assert!(files[0].contents.contains("amount?: number"));
    }

    #[test]
    fn python_generates_dataclass() {
        let files = PythonGenerator.generate(&sample_spec());
        assert!(files[0].contents.contains("class Event"));
        assert!(files[0].contents.contains("id: str"));
        assert!(files[0].contents.contains("amount: Optional[int]"));
    }

    #[test]
    fn python_client_uses_snake_case_methods() {
        let files = PythonGenerator.generate(&sample_spec());
        assert!(files[1].contents.contains("def list_events"));
    }

    #[test]
    fn go_generates_struct_with_json_tags() {
        let files = GoGenerator.generate(&sample_spec());
        assert!(files[0].contents.contains("type Event struct"));
        assert!(files[0].contents.contains(r#"Id string `json:"id"`"#));
    }

    #[test]
    fn go_client_uses_pascal_case_methods() {
        let files = GoGenerator.generate(&sample_spec());
        assert!(files[1].contents.contains("func (c *PulseAPIClient) ListEvents"));
    }

    #[test]
    fn rust_generates_struct_with_serde() {
        let files = RustGenerator.generate(&sample_spec());
        assert!(files[0].contents.contains("pub struct Event"));
        assert!(files[0].contents.contains("pub amount: Option<i64>"));
        assert!(files[0].contents.contains("Vec<String>"));
    }

    #[test]
    fn rust_client_generates_async_methods() {
        let files = RustGenerator.generate(&sample_spec());
        assert!(files[1].contents.contains("pub async fn list_events"));
    }

    #[test]
    fn generate_all_covers_every_language() {
        let results = generate_all(&sample_spec());
        let languages: Vec<&str> = results.iter().map(|(lang, _)| *lang).collect();
        assert_eq!(languages, vec!["typescript", "python", "go", "rust"]);
        for (_, files) in &results {
            assert_eq!(files.len(), 2);
        }
    }

    #[test]
    fn snake_case_conversion() {
        assert_eq!(to_snake_case("listEvents"), "list_events");
        assert_eq!(to_snake_case("Get"), "get");
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(to_pascal_case("list_events"), "ListEvents");
        assert_eq!(to_pascal_case("pulse-api"), "PulseApi");
    }
}
