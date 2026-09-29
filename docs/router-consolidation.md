# Router Consolidation

Router builders should be composed from per-domain route modules and registered once from application bootstrap.

Avoid duplicated middleware setup by sharing authentication, tracing, and error layers through a single router factory.
