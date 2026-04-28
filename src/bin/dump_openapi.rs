/// Dumps the generated OpenAPI spec to stdout as JSON.
/// Used by CI to validate and diff the committed docs/openapi.json.
fn main() {
    use soroban_pulse::routes::ApiDoc;
    use utoipa::OpenApi;
    let spec = ApiDoc::openapi();
    println!(
        "{}",
        serde_json::to_string_pretty(&spec).expect("failed to serialize OpenAPI spec")
    );
}
