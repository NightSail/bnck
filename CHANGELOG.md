# Changelog

Notable changes to bnck are documented here. Schema versions and their bump rules are defined in [docs/release-format.md](docs/release-format.md#schema-version-fields).

No public release has been made.

## Unreleased

### Added

- Initial development pipeline for channel acquisition, TOML curations, provider JSON Lines, SQLite artifacts, and the static website
- ISC licensing for code, curations, dataset artifacts, documentation, and website content
- Candidate JSON Lines artifacts in release provenance
- Release verification instructions and public key documentation
- Signed release packaging as `.tar.zst` for GitHub Releases
- Canonical dataset redirects in the generated `_redirects` file
- Release assembly rejects absolute and parent-relative input paths
- Candidate-only validation mode for split acquisition and release pipelines
- GitHub Actions release workflow with per-channel acquisition jobs and site deployment
- Daily scheduled release runs with automatic CalVer assignment from dataset tags
- Dataset release tags namespaced as `bnck-dataset-<CalVer>`
- Unchanged provider datasets skipped during scheduled publication
- Latest dataset redirects resolve to the current release tag
- Automatic curation ingestion from normalized Terminal Trove and tldr hints
- Persistent hint rejections for candidates that must not be re-ingested
- Expanded seed curations from Terminal Trove and tldr hint batches
- Separate website About and Dataset pages with source, issue, release, contributing, and license links
- Dataset page links to the latest release alias, latest tag, GitHub Releases, and current artifact downloads
- TypeScript static site generation and browser modules
- Hot-reloading website development server with no runtime dependencies

### Changed

- Website headings, navigation, controls, and labels use lowercase; prose keeps sentence case.
- Dataset details moved from `/about` to `/dataset`; `/about` now presents the project.
- SQLite schema version is 1 for the first public schema. Pre-publication development artifacts labeled 2 are not release-compatible.
- Release documentation now distinguishes release, provider, SQLite, acquisition, curation, and browser schema versions.
- The release workflow is now a single `just release` command that assembles, signs, verifies, and packages the release.
- Website generation and development serving moved from Rust to Bun and TypeScript. `just site-build` generates the static site and `just site-dev` runs the hot-reloading server.
