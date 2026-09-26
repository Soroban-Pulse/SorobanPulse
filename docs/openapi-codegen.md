# OpenAPI Client Library Code Generation

Generates client library source (models + a thin HTTP client) for
TypeScript, Python, Go, and Rust from a spec description. Implemented in
[`src/codegen/openapi.rs`](../src/codegen/openapi.rs) as an extension to the
existing [`codegen`](../src/codegen/mod.rs) module (previously scoped to
subscription scaffolding).

## Why

Soroban Pulse exposes an HTTP API consumed from multiple language SDKs
(`sdk/typescript`, `sdk/python`, `sdk/go` — see [client-libraries.md](client-libraries.md)
and [codegen.md](codegen.md)). Hand-maintaining model/client code for four
languages in lockstep with the API is error-prone. This generator produces
that boilerplate directly from a spec model, so client SDKs can be
regenerated whenever the API surface changes.

## Spec model

The generator works against a minimal in-memory representation of the parts
of an OpenAPI document it needs — not a full OpenAPI parser:

- **`OpenApiSpec`** — `title`, `version`, a list of `SchemaDef` (models) and
  `OperationDef` (client methods).
- **`SchemaDef`** — a name plus a flat list of `FieldDef { name, ty, required }`.
- **`FieldType`** — `String | Integer | Number | Boolean | Array(Box<FieldType>) | Ref(String)`.
- **`OperationDef`** — `operation_id`, HTTP `method`, `path`, and optional
  request/response schema names.

Building this model from a real `openapi.yaml`/`openapi.json` file (via
`serde_yaml`/`serde_json`) is left to a thin adapter layer outside this
module, keeping the generators themselves format-agnostic.

## Generators

Each language backend implements the `ClientGenerator` trait
(`generate_models` + `generate_client`):

| Generator | Models output | Client output |
|---|---|---|
| `TypeScriptGenerator` | `models.ts` — `export interface` per schema, optional fields via `?` | `client.ts` — a `fetch`-based class, one async method per operation |
| `PythonGenerator` | `models.py` — `@dataclass` per schema, `Optional[...]` for non-required fields | `client.py` — a `requests`-based class with snake_case methods |
| `GoGenerator` | `models.go` — exported structs with `json:"..."` tags | `client.go` — a struct with PascalCase methods |
| `RustGenerator` | `models.rs` — `#[derive(Serialize, Deserialize)]` structs, `Option<T>` for optional fields | `client.rs` — an async client struct using `reqwest::Error` |

`generate_all(&spec)` runs every built-in generator and returns
`Vec<(&'static str, Vec<GeneratedFile>)>` keyed by language name.

## Usage

```rust
use soroban_pulse::codegen::openapi::{OpenApiSpec, SchemaDef, FieldDef, FieldType, OperationDef, generate_all};

let spec = OpenApiSpec {
    title: "Pulse API".into(),
    version: "1.0.0".into(),
    schemas: vec![SchemaDef {
        name: "Event".into(),
        fields: vec![
            FieldDef { name: "id".into(), ty: FieldType::String, required: true },
            FieldDef { name: "amount".into(), ty: FieldType::Integer, required: false },
        ],
    }],
    operations: vec![OperationDef {
        operation_id: "listEvents".into(),
        method: "get".into(),
        path: "/events".into(),
        request_schema: None,
        response_schema: Some("Event".into()),
    }],
};

for (language, files) in generate_all(&spec) {
    for file in files {
        std::fs::write(format!("generated/{language}/{}", file.path), file.contents)?;
    }
}
```

Generating a single language:

```rust
use soroban_pulse::codegen::openapi::{RustGenerator, ClientGenerator};

let files = RustGenerator.generate(&spec);
```

## Testing

```
cargo test codegen::openapi
```

Covers: TypeScript interface/optional-field generation, Python dataclass
generation with snake_case client methods, Go struct/JSON-tag generation
with PascalCase client methods, Rust struct/serde generation with async
client methods, `generate_all` covering every language, and the
`snake_case`/`PascalCase` naming helpers.
