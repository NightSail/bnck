# Contributing

This guide covers development, acquisition jobs, curations, release publication, and website maintenance. See [README.md](README.md) for the project overview and artifact usage.

## Development checks

Run `just check` with Rust, just, and Bun installed. It includes formatting checks, Clippy, Rust tests, browser unit tests, acquisition-job tests, and comparisons of the JavaScript search results against SQLite:

```sh
just check
```

## Demo

Run a small live acquisition for all curated npm packages and Black:

```sh
just demo
```

Each demo creates a fresh directory under `var/demo/` and prints its path. It contains `acquired/npm.jsonl`, `acquired/pypi.jsonl`, their adjacent manifests, and `providers.jsonl`. Demo jobs do not change production acquisitions, provider output, or default input selections.

Provider records are accepted through the checked-in curations under `curations/`. Dataset generation requires the adjacent snapshot manifests; rerun the import jobs after upgrading from candidate-only acquisition files.

## Acquisition jobs

Registry and artifact downloads are performed by command-line tools in just recipes. Rust importers only read local files and write normalized data. Each import writes candidate JSON Lines plus an adjacent, content-identified [acquisition snapshot manifest](docs/acquisition-format.md).

List available jobs:

```sh
just
```

Fetch and import all curated npm packages, without arguments:

```sh
just fetch-npm
just import-npm
just generate
```

`fetch-npm` creates a fresh planned snapshot under `var/input/npm/` and selects it only after all requests finish. `import-npm` imports that selection, not the old per-package cache. For explicit snapshots, use `just fetch-npm <snapshot>` and `just import-npm <snapshot>`. Resume interrupted downloads with `just resume-npm <snapshot>`, then import that path explicitly. An optional second argument to `import-npm` selects an output path.

The older PyPI focused workflow remains available as `just fetch-pypi black` followed by `just import-pypi`. It produces a partial acquisition; use `just demo` to keep a focused run separate from production data.

Fetch and import distro metadata:

```sh
just fetch-distros
just import-distros
```

Each distro can also be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the corresponding import job. Debian acquisition includes both the configured primary suite, stable by default, and unstable/sid. The Fedora base URL, Debian suites and architecture, and Arch mirror and architecture can be changed through the environment variables defined in their justfiles.

Acquire all language channels, using broad crates.io/PyPI catalogs and explicit npm package curations:

```sh
just fetch-languages
just import-languages
just generate
```

Both jobs run without arguments. Each fetch creates a fresh directory under `var/input/languages/`; imports use the most recently completed fetch. Failed refreshes leave the previous completed inputs selected, and earlier input directories are retained.

crates.io uses its full database dump without per-package API requests; npm fetches one small `latest` release manifest per explicitly curated package, without a catalog scan, full version histories, or package archives; PyPI uses its full project index followed by selected Simple API responses and statically inspected wheels. Only explicitly attributed packages enter the provider dataset. See [language registry acquisition](docs/language-registries.md) for individual jobs, resuming downloads, release selection, and coverage limitations.

The crates.io database dump and distro metadata sets are large. Fetch jobs retain their inputs under `var/input/`, selected PyPI wheels are cached by digest under `var/cache/pypi/wheels/`, and normalized acquisition output is written under `var/acquired/`.

Show acquisition and provider record counts with:

```sh
just stats
```

## Curations

Project definitions and package ownership rules live under `curations/`. The [curation format](docs/curations.md) documents their schema, exceptional executable rules, and validation requirements.

### Suggestions

Find acquired packages that may belong to a project:

```sh
cargo run -- suggest-packages \
	--name ripgrep \
	--homepage https://burntsushi.net/ripgrep/ \
	--repository https://github.com/BurntSushi/ripgrep
```

Find curated or metadata-derived projects that may own a package:

```sh
cargo run -- suggest-projects --channel debian --package ripgrep
```

Generate a batch report of unowned language-registry packages related to existing projects:

```sh
cargo run -- suggest-registry-packages --output var/registry-suggestions.json
```

Review the evidence and upstream identity before adding package attributions. Name matches and copied repository URLs alone do not establish ownership.

Suggestions are ranked using canonical repository and homepage URLs, package and source-package names, executable names, and existing curation ownership. Each result reports its matching evidence. The commands read only local acquisition and curation data and do not modify curations.

## Release artifacts

Generate the compressed SQLite consumer artifact from the canonical provider JSON Lines file:

```sh
just generate-sqlite 2026-04-16.1
```

This writes `var/bnck.sqlite.zst`. The command validates the canonical JSON Lines encoding, ordering, and uniqueness before building the database. It records the SHA-256 of the uncompressed JSON Lines content, provider count, release identifier, generation time, generator version, and Git revision in the SQLite metadata table. It also reads project metadata from `curations/` and includes curated homepages, repositories, documentation, issue trackers, and additional links in the derived SQLite artifact.

Assemble the release directory, including compressed provider and SQLite artifacts, acquisition manifests, `manifest.json`, and `SHA256SUMS`:

```sh
just release 2026-04-16.1
```

Release assembly does not expose a signing key to the generator. Produce `SHA256SUMS.minisig` with the separately managed release signing key before publication:

```sh
just sign-release var/releases/bnck-2026-04-16.1 /path/to/minisign.key
```

See the [release format](docs/release-format.md) for artifact schemas, provenance, integrity, and compatibility rules.

## Website development

Generate a fully static website from the compressed or uncompressed SQLite artifact, then preview it locally with Bun:

```sh
just generate-site
just preview
```

The preview listens on `http://127.0.0.1:3000` by default. The generator writes `var/site/` and refuses to overwrite an existing output directory. Use a fresh output path for each build. No application server or browser database runtime is required in production:

```sh
cargo run -- generate-site \
	--database var/bnck.sqlite.zst \
	--output var/site-next
bun web/preview.js var/site-next 3000
```

Publish the output directory at the origin root on a case-sensitive static host with directory index support. Paths such as `/projects/helix` resolve to `projects/helix/index.html`; hosts may redirect to a trailing slash. Configure the generated `404.html` as the error page, not an SPA fallback. Enable HTTP Brotli or gzip compression, serve `.json` as `application/json` and `.js` as JavaScript, and apply the generated `_headers` file or equivalent host settings. HTML and exports revalidate; the content-addressed `/assets/` files cache immutably. Application assets and the dataset have independent hashes, so a dataset update does not invalidate unchanged scripts or stylesheets. Deploy pages and assets together and retain previous application directories and dataset files so already-open pages can finish loading their matching versions. See [static deployment](docs/website.md#static-deployment) for details.

HTML templates live under `web/templates/`, and the stylesheet is `web/style.css`. Debug builds read these files and browser modules from disk during generation; release builds embed them. Rebuild the static output after edits. The existing SQLite-backed interface remains available through `just serve` for server-rendered development and query comparisons.

## Responsibilities

Just recipes handle:

- Directory creation
- HTTP downloads
- Archive extraction
- Job ordering
- Passing local paths between steps

Rust code handles:

- CSV, JSON, TOML, and wheel metadata parsing
- Acquisition snapshot manifests, package observations, and input digests
- Release selection
- Package and executable normalization
- Curation validation and resolution
- Deterministic JSON Lines generation

Rust code does not shell out or perform network requests. Just recipes do not parse or reshape channel data beyond decompression and archive extraction.
