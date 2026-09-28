# docs/site/static/

This directory holds static assets served directly by Docusaurus (no
processing). Files placed here are available at the site root — e.g.
`static/openapi.json` → `/openapi.json`.

## openapi.json

The canonical OpenAPI specification (`openapi.json`) is **not committed here**.
It is copied from the repository root by:

- **CI/CD** — the `docs.yml` workflow runs `cp ../../openapi.json static/openapi.json`
  before `npm run build`.
- **Local development** — run the following before `npm run start`:

  ```bash
  cp ../../openapi.json docs/site/static/openapi.json
  # then
  cd docs/site && npm run start
  ```

The `/api-reference` page loads the spec from `/openapi.json` at runtime.
