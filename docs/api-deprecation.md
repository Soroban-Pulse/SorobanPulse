# API Deprecation Policy and Timeline

## Policy

- Only the current major version (`/v1`) and the immediately previous one are supported.
- Breaking changes are never made inside a major version; they ship in a new one (see `api-versioning.md`).
- Any endpoint, field, or parameter slated for removal is deprecated first.

## Timeline

| Phase | Duration | What happens |
|-------|----------|--------------|
| Announcement | Day 0 | Deprecation noted in `CHANGELOG.md` and `docs/api-changelog.md`; announcement published |
| Warning | Day 0 to removal | Responses carry `Deprecation: true`, `Sunset: <HTTP-date>` and `Link: <guide>; rel="deprecation"` headers |
| Migration window | Minimum 6 months | Old and new behaviour both work; upgrade guide available |
| Brownout (optional) | Final 2 weeks | Short scheduled periods where the deprecated endpoint returns `410 Gone` |
| Removal | Sunset date | Endpoint returns `410 Gone` with a pointer to the replacement |

Security fixes may shorten the window; this is called out explicitly in the notice.

## Deprecation warnings in code

Mark a route with a middleware layer that adds the headers below, and annotate the handler with `deprecated = true` in `#[utoipa::path]` so `openapi.json` reflects it.

```
Deprecation: true
Sunset: Wed, 31 Dec 2026 23:59:59 GMT
Link: <https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/api-deprecation.md>; rel="deprecation"
```

## Deprecation metrics

Track remaining usage of deprecated endpoints so removal is data driven. Use a counter such as `soroban_pulse_deprecated_requests_total{endpoint,version}`; remove an endpoint only when its traffic is near zero or the sunset date has passed.

## Deprecation notice template

```
Title: Deprecation of <endpoint/field> in API <version>
Deprecated on: <date>
Sunset date:   <date>
Replacement:   <new endpoint/field>
Reason:        <why>
Impact:        <who is affected>
Migration:     <link to upgrade guide>
```

## Announcement template

> We are deprecating `<item>` in Soroban Pulse API `<version>`. It remains available until `<sunset date>`. Please migrate to `<replacement>` using the guide at `<link>`. Questions: open a GitHub issue with the `deprecation` label.

## Upgrade guides and migration tooling

- Per-change upgrade guides live in `docs/migration-guides/`.
- Each guide lists old request, new request, response differences, and a before/after example.
- The generated SDKs (see `docs/codegen.md`) are regenerated on each release so clients can upgrade by bumping the SDK version.
