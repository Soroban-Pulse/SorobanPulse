# Cargo Workspace Plan

Convert the repository into a Cargo workspace with crates for:

- API server
- indexer
- shared domain types
- CLI
- SDK helpers

Keep binary crates thin and move reusable logic into library crates.
