use utoipa::OpenApi;
use soroban_pulse::routes::ApiDoc;

fn main() {
    let openapi_json = ApiDoc::openapi().to_pretty_json().expect("Failed to generate OpenAPI JSON");
    println!("{}", openapi_json);
}
