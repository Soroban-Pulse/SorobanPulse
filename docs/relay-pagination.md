# Relay Cursor Pagination

GraphQL list fields should return `edges`, `node`, `cursor`, and `pageInfo`.

Cursors must be opaque and stable across retries. Prefer ledger plus event offset when building event cursors.
