# Soroban Pulse Governance

This document describes how the Soroban Pulse project is run: who can do what,
how decisions are made, and how contributors grow into maintainers.

All participants are expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Roles

### Contributor

Anyone who opens an issue, comments on a discussion, submits a pull request, or
improves documentation is a contributor. Contributors:

- follow [CONTRIBUTING.md](CONTRIBUTING.md) and the Code of Conduct;
- may pick up any issue labelled `good first issue` or `help wanted` by
  commenting on it first, so work is not duplicated;
- may propose changes of any size, including RFCs (see below).

### Reviewer

Reviewers are trusted contributors who help keep review latency low. They:

- have had at least **five non-trivial pull requests merged** within the last
  six months;
- are nominated by a maintainer and confirmed by lazy consensus of the
  maintainers (no objection within 7 days);
- can approve pull requests in the areas they are listed for in
  [MAINTAINERS.md](MAINTAINERS.md), triage issues, and apply labels.

A reviewer's approval counts towards the review requirement, but a pull request
still needs at least one maintainer approval (usually via
[CODEOWNERS](.github/CODEOWNERS)) before it is merged.

### Maintainer

Maintainers are responsible for the long-term health of the project. They:

- have merge rights and own one or more areas listed in
  [MAINTAINERS.md](MAINTAINERS.md);
- review and merge pull requests, cut releases (see [RELEASING.md](RELEASING.md)),
  and handle security reports (see [SECURITY.md](SECURITY.md));
- enforce the Code of Conduct;
- vote on RFCs, governance changes, and new maintainers.

## Decision Making

### Day-to-day changes: lazy consensus

Most decisions — bug fixes, features scoped to one area, documentation — are
made through normal pull request review. A pull request can be merged when:

1. CI is green;
2. it has approval from at least one code owner of every touched area; and
3. no maintainer has an outstanding "request changes" review.

If reviewers disagree, they should try to reach consensus in the pull request.
If that fails, any maintainer can escalate the question to a maintainer vote.

### Significant changes: RFCs

A change needs an RFC before implementation starts if it:

- adds or removes a public API endpoint, SDK surface, or CLI command in a
  backwards-incompatible way;
- changes the database schema in a way that requires a data migration;
- introduces a new runtime dependency on an external service;
- changes the security model (authentication, encryption, key management); or
- changes this governance document.

RFC process:

1. Open a GitHub issue titled `[RFC] <summary>` describing the motivation, the
   proposal, alternatives considered, and migration/compatibility impact.
   Architectural decisions should also be captured as an
   [ADR](docs/adr/README.md) using `docs/adr/0000-template.md`.
2. The RFC stays open for comments for **at least 7 days** (14 days for
   governance changes).
3. Maintainers then decide by lazy consensus. If any maintainer objects, the RFC
   goes to a vote.
4. The outcome (accepted / rejected / postponed) is recorded on the issue and,
   where relevant, in the ADR.

### Voting

When a vote is needed, each maintainer has one vote. A proposal passes with a
simple majority of maintainers who vote within 7 days, provided at least half
of all maintainers participated. Governance changes and maintainer removal
require a two-thirds majority.

## Becoming a Maintainer

A reviewer can be nominated as maintainer by any existing maintainer when they
have:

- been an active reviewer for at least **three months**;
- demonstrated good judgement in reviews and sustained, high-quality
  contributions in at least one area;
- shown that they follow and uphold the Code of Conduct.

The nomination is made as a pull request that adds the person to
[MAINTAINERS.md](MAINTAINERS.md) and `.github/CODEOWNERS`. It is accepted by
lazy consensus of the maintainers after 7 days, or by a vote if anyone objects.

## Stepping Down and Emeritus Status

Maintainers and reviewers who can no longer commit time are encouraged to step
down by opening a pull request that moves them to the *Emeritus* section of
[MAINTAINERS.md](MAINTAINERS.md). Someone who has been inactive (no reviews,
commits, or issue activity) for six months may be moved to emeritus by a
maintainer vote after being contacted. Emeritus members can return through the
normal nomination process.

A maintainer may be removed for a serious or repeated breach of the Code of
Conduct by a two-thirds vote of the other maintainers.

## Branch Protection

The `main` branch is protected with the following settings. Maintainers must
not weaken them without an RFC:

- **Required status check:** `ci-success` must pass before a pull request can
  be merged. `ci-success` is the aggregate job of the CI workflow and fails
  when any required job fails, is cancelled, or is skipped, and when the CI
  workflow did not run at all for the pull request. A skipped Rust job (for
  example because of a path filter) therefore does not count as a pass.
- **No bypass:** administrators and maintainers are subject to the same
  required checks; force pushes and direct pushes to `main` are disabled.
- **Up-to-date branches:** a pull request must be up to date with `main`
  before merging, so the checks that passed are the ones that will land.
- **Review requirement:** at least one code owner approval for every touched
  area, as described under *Decision Making*.

### Dependabot Auto-Merge

Dependabot patch updates labelled `dependencies` may be auto-merged by
`.github/workflows/auto-merge.yml`, but only under these conditions:

- Auto-merge is enabled only when the full Rust test suite has actually run
  and passed on the pull request. Because `ci-success` fails on skipped jobs,
  a pull request whose Rust jobs were skipped cannot satisfy the required
  check and will not merge.
- Auto-merge is disabled entirely while CI is known to be broken or invalid.
- Crates that use build scripts (`build.rs`) or proc macros are excluded from
  auto-merge; their updates require at least one human approval.
- The `dependabot/fetch-metadata` action is pinned by commit SHA.

## Changes to This Document

Changes to this document follow the RFC process above.
