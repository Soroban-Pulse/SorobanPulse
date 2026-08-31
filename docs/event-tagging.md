# Event Tagging

`src/event_tagging.rs` adds a flexible tagging system for indexed events,
backed by three tables added in
`migrations/20260831000002_event_tagging_system.sql`.

## Schema

- **`event_tags`** — the tag catalog. Each tag has a unique `name`, an
  optional `description`/`color`, and an optional `parent_id` referencing
  another tag, forming a hierarchy/taxonomy (e.g. `defi` → `defi:swap` →
  `defi:swap:arbitrage`).
- **`event_tag_assignments`** — the many-to-many relationship between
  `events` and `event_tags`, with a `source` column (`manual` or `auto`)
  recording how the tag was applied.
- **`event_auto_tag_rules`** — rules used to auto-tag events as they're
  indexed (see below).

## Tag management

`event_tagging::create_tag`, `delete_tag`, and `list_tags` manage the tag
catalog. `apply_tag`/`remove_tag` attach/detach a tag from a specific
event (`apply_tag` is idempotent — reapplying an existing assignment is a
no-op via `ON CONFLICT DO NOTHING`). `tags_for_event` lists a given
event's tags.

## Tag-based filtering

`event_tagging::event_ids_by_tag(pool, tag_name, limit, offset)` returns
event ids carrying a given tag, most recent first, for use in event
search/listing queries.

## Auto-tagging

`event_auto_tag_rules` rows describe a match predicate:

- `event_type_match`: exact match against the event's `event_type`
  (`contract`/`diagnostic`/`system`), when set.
- `property_path` + `property_value`: a dotted path into the event's
  `event_data` JSON (e.g. `action.kind`) whose string value must equal
  `property_value`, when both are set.

A rule with **neither** predicate set matches nothing — auto-tagging is
always based on an actual event property or type, never applied blindly.
When both predicates are set, both must match (AND, not OR).

`event_tagging::auto_tag_event(pool, event_id, event_type, event_data)`
evaluates every active rule against one event and applies the matching
tags with `source = 'auto'`. Call it from the event ingestion path (e.g.
`src/event_handler.rs` or `src/indexer.rs`) after an event is inserted, to
tag events as they arrive.

## Analytics

`event_tagging::tag_usage_metrics(pool)` returns, per tag, the total
number of tagged events plus a manual/auto breakdown — useful for
identifying unused tags or auto-tagging rules that are over/under-firing.

## Testing

`src/event_tagging.rs` includes unit tests for the auto-tagging rule
matcher: a rule with no predicates never matches, event-type-only rules,
nested-property rules, rules requiring both predicates, and missing JSON
paths correctly failing to match.
