//! API documentation handlers: OpenAPI JSON and Swagger UI (Issue #1241).

#[allow(unused_imports)]
use super::*;

/// Serve the raw OpenAPI JSON spec.
pub async fn openapi_json() -> impl IntoResponse {
    use crate::routes::ApiDoc;
    use utoipa::OpenApi;
    Json(ApiDoc::openapi())
}

/// Serve the branded SorobanPulse Swagger UI page.
///
/// The inline `<style>` block applies SorobanPulse brand colours and full dark
/// mode on top of the stock Swagger UI stylesheet loaded from unpkg.com.
/// The `<script>` block initialises SwaggerUIBundle.
///
/// Both blocks carry a SHA-256 hash that is allow-listed in the `/docs` CSP so
/// `unsafe-inline` is **not** needed.  If you modify either block you must
/// recompute the hash with:
///
/// ```sh
/// printf '%s' 'THE CONTENT' | openssl dgst -sha256 -binary | base64
/// ```
///
/// and update `csp_docs` in `src/middleware/security_headers.rs` accordingly.
pub async fn swagger_ui() -> impl IntoResponse {
    // Inline CSS — SHA-256: t5Nfs8a1PFuEVO00S72ZGB4P65C74f37u8w0VCVsqBw=
    let style = r#":root{--sp-bg:#0f1117;--sp-surface:#1a1d2e;--sp-border:#2d3158;--sp-accent:#7c3aed;--sp-accent-light:#a78bfa;--sp-text:#e2e8f0;--sp-text-muted:#94a3b8;--sp-success:#10b981;--sp-warning:#f59e0b;--sp-danger:#ef4444}
body{background:var(--sp-bg)!important;color:var(--sp-text)!important;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif}
#swagger-ui .swagger-ui .info{margin:2rem 0}
#swagger-ui .swagger-ui .info .title{color:var(--sp-accent-light)!important;font-size:2rem!important;font-weight:700}
#swagger-ui .swagger-ui .info .description p,#swagger-ui .swagger-ui .info .description li{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .info .description a{color:var(--sp-accent-light)!important}
#swagger-ui .topbar{background:var(--sp-surface)!important;border-bottom:1px solid var(--sp-border)!important;padding:.75rem 1rem}
#swagger-ui .swagger-ui .scheme-container{background:var(--sp-surface)!important;border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock-tag{color:var(--sp-text)!important;border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock{background:var(--sp-surface)!important;border:1px solid var(--sp-border)!important;border-radius:.5rem!important;margin-bottom:.75rem}
#swagger-ui .swagger-ui .opblock .opblock-summary{border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock .opblock-summary-method{border-radius:.25rem!important;font-weight:600}
#swagger-ui .swagger-ui .opblock.opblock-get .opblock-summary-method{background:var(--sp-success)!important}
#swagger-ui .swagger-ui .opblock.opblock-post .opblock-summary-method{background:var(--sp-accent)!important}
#swagger-ui .swagger-ui .opblock.opblock-delete .opblock-summary-method{background:var(--sp-danger)!important}
#swagger-ui .swagger-ui .opblock.opblock-patch .opblock-summary-method{background:var(--sp-warning)!important}
#swagger-ui .swagger-ui .opblock-description-wrapper p,.swagger-ui .markdown p{color:var(--sp-text)!important}
#swagger-ui .swagger-ui section.models{background:var(--sp-surface)!important;border:1px solid var(--sp-border)!important;border-radius:.5rem}
#swagger-ui .swagger-ui section.models h4{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .model-box{background:var(--sp-bg)!important}
#swagger-ui .swagger-ui .parameter__name,#swagger-ui .swagger-ui .parameter__type{color:var(--sp-text-muted)!important}
#swagger-ui .swagger-ui input[type=text],#swagger-ui .swagger-ui textarea{background:var(--sp-bg)!important;border:1px solid var(--sp-border)!important;color:var(--sp-text)!important}
#swagger-ui .swagger-ui select{background:var(--sp-bg)!important;color:var(--sp-text)!important;border:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .btn.authorize{background:var(--sp-accent)!important;border-color:var(--sp-accent)!important;color:#fff!important}
#swagger-ui .swagger-ui .btn.execute{background:var(--sp-success)!important;border-color:var(--sp-success)!important;color:#fff!important}
#swagger-ui .swagger-ui .response-col_status{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .response-col_description{color:var(--sp-text-muted)!important}
#swagger-ui .swagger-ui pre.microlight{background:var(--sp-bg)!important;color:var(--sp-accent-light)!important;border:1px solid var(--sp-border)!important;border-radius:.375rem}
#swagger-ui .swagger-ui .highlight-code{background:var(--sp-bg)!important}"#;

    // Inline JS — SHA-256: OlhJ06FtsPaiJ/1A+8VpeJOGKCgqkf63ajgd7nrxk98=
    let script = r##"SwaggerUIBundle({url:"/openapi.json",dom_id:"#swagger-ui",deepLinking:true,presets:[SwaggerUIBundle.presets.apis,SwaggerUIBundle.SwaggerUIStandalonePreset],layout:"BaseLayout",docExpansion:"list",defaultModelsExpandDepth:1})"##;

    // SorobanPulse logo — inline SVG, no external fetch required
    let logo_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 36 36" fill="none" aria-hidden="true">
  <circle cx="18" cy="18" r="18" fill="#7c3aed"/>
  <path d="M10 18 Q18 8 26 18 Q18 28 10 18Z" fill="#a78bfa"/>
  <circle cx="18" cy="18" r="4" fill="#0f1117"/>
</svg>"##;

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <title>SorobanPulse — API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css"/>
  <style>{style}</style>
</head>
<body>
  <header style="display:flex;align-items:center;gap:.75rem;padding:1rem 1.5rem;background:#1a1d2e;border-bottom:1px solid #2d3158;position:sticky;top:0;z-index:100">
    {logo_svg}
    <span style="font-size:1.25rem;font-weight:700;color:#a78bfa;letter-spacing:-.01em">SorobanPulse</span>
    <span style="font-size:.875rem;color:#94a3b8;margin-left:.25rem">API Explorer</span>
    <a href="/openapi.json" style="margin-left:auto;font-size:.8125rem;color:#a78bfa;text-decoration:none;border:1px solid #2d3158;padding:.25rem .625rem;border-radius:.375rem">OpenAPI JSON ↗</a>
  </header>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>{script}</script>
</body>
</html>"#,
        style = style,
        logo_svg = logo_svg,
        script = script,
    );

    axum::response::Html(html)
}
