# Release Process

This document describes the process for cutting a new release of Soroban Pulse.

## Versioning

Soroban Pulse follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html):

- **MAJOR** version for incompatible API changes
- **MINOR** version for new functionality in a backward-compatible manner
- **PATCH** version for backward-compatible bug fixes

## Release Steps

### 1. Prepare the Release

1. Create a new branch for the release:
   ```bash
   git checkout -b release/v{VERSION}
   ```

2. Update `Cargo.toml` with the new version:
   ```toml
   [package]
   version = "{VERSION}"
   ```

3. Update `CHANGELOG.md`:
   - Move items from `[Unreleased]` section to a new `[{VERSION}] - {DATE}` section
   - Use the format: `## [{VERSION}] - YYYY-MM-DD`
   - Ensure all changes are documented under appropriate categories: Added, Changed, Deprecated, Removed, Fixed, Security
   - Update the comparison links at the bottom:
     ```markdown
     [Unreleased]: https://github.com/Soroban-Pulse/SorobanPulse/compare/v{VERSION}...HEAD
     [{VERSION}]: https://github.com/Soroban-Pulse/SorobanPulse/releases/tag/v{VERSION}
     ```

4. Commit the changes:
   ```bash
   git add Cargo.toml Cargo.lock CHANGELOG.md
   git commit -m "chore: Prepare release v{VERSION}"
   ```

### 2. Create a Pull Request

1. Push the release branch:
   ```bash
   git push -u origin release/v{VERSION}
   ```

2. Create a pull request with:
   - Title: `Release v{VERSION}`
   - Description: Include a summary of changes from the CHANGELOG
   - Link to the CHANGELOG section for this release

3. Ensure all CI checks pass (fmt, clippy, test, integration-tests, build)

4. Get approval from at least one maintainer

5. Merge the PR to `main`

### 3. Create a Git Tag

1. Checkout main and pull the latest changes:
   ```bash
   git checkout main
   git pull origin main
   ```

2. Create an annotated tag:
   ```bash
   git tag -a v{VERSION} -m "Release v{VERSION}"
   ```

3. Push the tag:
   ```bash
   git push origin v{VERSION}
   ```

### 4. Create a GitHub Release

1. Go to [Releases](https://github.com/Soroban-Pulse/SorobanPulse/releases)

2. Click "Draft a new release"

3. Select the tag `v{VERSION}`

4. Set the title to `v{VERSION}`

5. Copy the CHANGELOG section for this release into the description

6. If this is a pre-release (alpha, beta, rc), check "Set as a pre-release"

7. Click "Publish release"

### 5. Docker Image

The Docker image is automatically built and pushed to `ghcr.io/Soroban-Pulse/SorobanPulse` when:
- A commit is pushed to `main`
- All CI checks pass

The image is tagged with:
- `latest` (for the most recent release)
- `{VERSION}` (semantic version tag)
- `{COMMIT_SHA}` (full commit SHA)

To manually build and push:
```bash
docker build -t ghcr.io/Soroban-Pulse/SorobanPulse:v{VERSION} .
docker push ghcr.io/Soroban-Pulse/SorobanPulse:v{VERSION}
```

## Changelog Categories

When updating the CHANGELOG, use these categories:

- **Added**: New features
- **Changed**: Changes in existing functionality
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security vulnerability fixes

## Example Release

```markdown
## [0.2.0] - 2026-05-15

### Added
- New feature X
- New feature Y

### Changed
- Modified behavior of endpoint Z

### Fixed
- Fixed bug in event indexing
- Fixed race condition in pool metrics

### Security
- Updated dependencies to patch CVE-XXXX-XXXXX
```

## Rollback

If a release needs to be rolled back:

1. Delete the GitHub release
2. Delete the git tag: `git push origin --delete v{VERSION}`
3. Revert the merge commit on main: `git revert -m 1 {COMMIT_SHA}`
4. Push the revert: `git push origin main`

## Questions?

For questions about the release process, please open an issue or contact the maintainers.
