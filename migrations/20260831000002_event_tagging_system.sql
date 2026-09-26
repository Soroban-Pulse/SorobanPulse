-- Issue #936: Add flexible tagging system for events

CREATE TABLE IF NOT EXISTS event_tags (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL UNIQUE,
    description TEXT,
    -- Hierarchy/taxonomy support: NULL for a root tag, otherwise the parent
    -- tag's id (e.g. "defi" -> "defi:swap" -> "defi:swap:arbitrage").
    parent_id   UUID        REFERENCES event_tags(id) ON DELETE SET NULL,
    color       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_event_tags_parent_id ON event_tags(parent_id);

CREATE TABLE IF NOT EXISTS event_tag_assignments (
    event_id    UUID        NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    tag_id      UUID        NOT NULL REFERENCES event_tags(id) ON DELETE CASCADE,
    -- 'manual' (user/API applied) or 'auto' (matched an auto-tagging rule).
    source      TEXT        NOT NULL DEFAULT 'manual',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (event_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_event_tag_assignments_tag_id ON event_tag_assignments(tag_id);
CREATE INDEX IF NOT EXISTS idx_event_tag_assignments_event_id ON event_tag_assignments(event_id);

-- Auto-tagging rules: assign `tag_id` to any event whose `event_type`
-- matches `event_type_match` (when set) and/or whose `event_data` JSON
-- contains `property_path` = `property_value` (when both set). A rule with
-- neither predicate set is inert (never matches).
CREATE TABLE IF NOT EXISTS event_auto_tag_rules (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tag_id             UUID        NOT NULL REFERENCES event_tags(id) ON DELETE CASCADE,
    event_type_match   TEXT,
    property_path      TEXT,
    property_value     TEXT,
    active             BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_event_auto_tag_rules_tag_id ON event_auto_tag_rules(tag_id);
CREATE INDEX IF NOT EXISTS idx_event_auto_tag_rules_active ON event_auto_tag_rules(active) WHERE active;
