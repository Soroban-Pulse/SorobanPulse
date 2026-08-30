# API Changelog Process

This document describes the **process** for recording API-visible changes
going forward. For the versioning policy itself (what requires a major
version bump, deprecation timelines, etc.) see
[`docs/api-versioning.md`](api-versioning.md) — this doc does not repeat it.

There is no historical backfill here: `CHANGELOG.md` already records most
past changes chronologically under `[Unreleased]` and dated release headers.
This doc is about keeping that record accurate from now on, specifically for
changes that affect API **consumers**.

---

## What counts as an API-visible change

Record it if a change affects anything a client could observe or depend on:

- A new, removed, or renamed endpoint or path parameter
- A new, removed, or renamed request/response field or query parameter
- A change to a field's type, nullability, or default value
- A new or changed HTTP status code for an existing endpoint
- A new or changed response header (rate-limit headers, `Deprecation`, etc.)
- A change to rate limits, pagination defaults, or sort order
- A new or changed webhook payload shape or signature scheme
- A new SSE event type or a change to an existing one
- A new or removed GraphQL field/type/subscription

Do **not** record purely internal changes here (refactors, query plan
optimizations, dependency bumps, test changes) unless they change observable
behavior (e.g., a performance fix that also changes a default).

## Where to record it

1. Add an entry under the `### Added` / `### Changed` / `### Deprecated` /
   `### Removed` / `### Fixed` subsection of the `## [Unreleased]` block at
   the top of the root [`CHANGELOG.md`](../CHANGELOG.md) — this file already
   follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) format
   and is where every prior API-visible change has been recorded. There is no
   separate API-only changelog file; keep using this one so history stays in
   one place.
2. If the change requires a new API version per
   [`docs/api-versioning.md`](api-versioning.md#what-constitutes-a-version-change),
   say so explicitly in the entry (e.g., "Breaking: removed `sort` param from
   `/v1/events`, will be reintroduced with different semantics in `/v2/`").
3. If the change adds or modifies an endpoint covered by `openapi.json`,
   regenerate the spec (`make gen-openapi`) as part of the same PR so the
   spec and the changelog entry land together. See
   [`docs/api-guide.md`](api-guide.md#how-the-openapi-spec-is-produced) for
   how that generation works.
4. If the change affects a generated SDK (`sdk/python/`, `sdk/typescript/`),
   regenerate it (`make generate-sdk`) in the same PR — see
   [`docs/client-libraries.md`](client-libraries.md#regenerating-the-sdks).
   For the hand-written Go SDK, update it manually if the change is relevant.

## PR checklist for API-visible changes

- [ ] Entry added to `CHANGELOG.md` under `[Unreleased]`
- [ ] Marked "Breaking" in the entry if it requires a major version bump
      (see `docs/api-versioning.md`)
- [ ] `openapi.json` (and `docs/openapi.json`) regenerated or hand-edited to
      match, and re-validated as JSON
- [ ] New/changed endpoints or fields have at least one `example` in
      `openapi.json` (see `docs/api-guide.md` for the current coverage gap)
- [ ] Generated SDKs regenerated if the change touches a route they cover
- [ ] Deprecated paths/fields get a `Deprecation` response header (matching
      the pattern already used for unversioned legacy routes — see
      [`docs/api-usage.md`](api-usage.md#base-url-and-versioning)) rather
      than being removed outright

## Release cut

When cutting a release, move the accumulated `[Unreleased]` entries under a
new dated version heading in `CHANGELOG.md`, per the existing convention in
that file. `make changelog` (requires
[`git-cliff`](https://github.com/orhun/git-cliff)) can generate a draft from
git history if you want a starting point, but the authoritative record is
the hand-maintained `CHANGELOG.md` — reconcile the generated draft into it
rather than replacing it wholesale.
