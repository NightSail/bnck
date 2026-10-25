# Contributing

Covers curations, development, acquisition jobs, release publication, and the website. See [README.md](README.md) for the project overview and artifact usage.

## Curations

Project definitions, package ownership rules, and name associations live under `curations/`. The [curation format](docs/curations.md) documents their schema, exceptional executable rules, name associations, and validation requirements. The [curation guide](docs/curation-guide.md) defines inclusion criteria, evidence standards, correction rules, and submission requirements.

Curators can propose changes by editing the relevant files under `curations/` and opening a pull request; no build is required to start. Every new rule needs a specific reason and durable public evidence. Validate with `just generate` when acquisition data covers the affected channel, or state the missing validation in the pull request.

### Hint ingestion

Normalized Terminal Trove and tldr hints live under `var/hints/`. Preview automatic project creation and package extension:

```sh
just ingest-hints
```

Apply the same plan and write an audit report:

```sh
just apply-hints
```

Ingestion requires URL identity and package or executable evidence, skips claimed packages, and does not rewrite existing project metadata. Review the preview before applying. Add rejected candidates to `curations/rejections.toml`; see [hint rejections](docs/curations.md#hint-rejections).

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

Report unowned language-registry packages related to existing projects:

```sh
cargo run -- suggest-registry-packages --output var/registry-suggestions.json
```

Review the evidence and upstream identity before adding attributions. Suggestions are not ownership decisions.

### Diagnostics

`just generate` and `just release` write acquisition diagnostics to `var/diagnostics/report.json` and compare against `var/diagnostics/baseline.json`. The first run creates the baseline. Warnings cover changed package and executable sets, removed packages, projects without observed packages, and project metadata conflicts; they do not block generation. See [curation diagnostics](docs/channels.md#curation-diagnostics) for the comparison rules.

## Development checks

Run `just check` with Rust, just, and Bun installed. Install the TypeScript tooling once:

```sh
bun install
just check
```

The checks cover formatting, Clippy, Rust tests, TypeScript types, browser tests, acquisition-job tests, and browser/SQLite search comparisons.

## Demo

```sh
just demo
```

Acquires curated npm packages and Black into a fresh `var/demo/` directory and prints its path. The directory contains `acquired/npm.jsonl`, `acquired/pypi.jsonl`, their manifests, `providers.jsonl`, and `names.jsonl`. Demo jobs do not change production acquisitions, provider output, name associations, or default input selections.

## Acquisition jobs

Command-line tools in just recipes perform registry and artifact downloads. Rust importers read local files and write candidate JSON Lines plus an adjacent, content-identified [acquisition snapshot manifest](docs/acquisition-format.md).

Fetch and import all curated npm packages:

```sh
just fetch-npm
just import-npm
just generate
```

`fetch-npm` creates a fresh planned snapshot under `var/input/npm/` and selects it only after all requests finish; `import-npm` imports that selection. Pass an explicit snapshot path to both to use another input. Resume with `just resume-npm <snapshot>`, then import that path. An optional second argument to `import-npm` sets the output path.

A focused PyPI run is available as `just fetch-pypi black` followed by `just import-pypi`. It produces a partial acquisition; use `just demo` to keep focused runs separate from production data.

Fetch and import distro metadata:

```sh
just fetch-distros
just import-distros
```

Each distro can be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the matching import job. Debian fetch requires `gpgv` and `gpg`, and verifies `InRelease` before downloading indexes. Set `DEBIAN_KEYRING` to use a non-default keyring. Fetch jobs write `acquisition-context.tsv`; import recipes pass it with `--context`. Pass `--previous <path>` to report removals against a preceding acquisition or extracted release. See [distribution channel acquisition](docs/channels.md) for channel scope, release selection, verification, and path variables.

Acquire all language channels:

```sh
just fetch-languages
just import-languages
just generate
```

Both jobs run without arguments; imports use the most recently completed fetch. crates.io uses its full database dump; npm and PyPI fetch metadata only for explicitly curated packages. See [language registry acquisition](docs/language-registries.md) for individual jobs, resuming downloads, release selection, and coverage limits.

Fetch jobs retain inputs under `var/input/`, selected PyPI wheels are cached by digest under `var/cache/pypi/wheels/`, and normalized acquisition output goes to `var/acquired/`.

Show acquisition and provider record counts:

```sh
just stats
```

Fetch jobs end with the on-disk size of retained inputs; import, generate, and release jobs end with the size of written artifacts.

## Release artifacts

Generate the compressed SQLite consumer artifact from the canonical provider JSON Lines:

```sh
just generate-sqlite 2026-04-16.1
```

This writes `var/bnck.sqlite.zst` after validating the canonical provider and name association JSON Lines encoding, ordering, and uniqueness. The SQLite metadata table records the SHA-256 and count of both uncompressed JSON Lines artifacts, release identifier, generation time, generator version, and Git revision. Curated homepages, repositories, documentation, issue trackers, additional links, and name associations are included.

Assemble, sign, verify, and package a release:

```sh
just release 2026-04-16.1
```

This writes the release directory and `var/releases/bnck-2026-04-16.1.tar.zst`. It uses `minisign.key` and `minisign.pub` by default and requires `minisign` and `zstd`. The secret key is never committed. See the [README](README.md) for public release verification and [dataset release format](docs/release-format.md) for artifact contents.

## Release automation

The `Dataset` workflow runs daily at 03:17 UTC and can also be dispatched manually. It runs one acquisition job per channel, generates the dataset, and publishes only when a canonical provider or name association digest differs from the latest dataset release. It does not build the site. After publishing a release, it dispatches the `Site` workflow with that release tag.

The `Site` workflow builds and deploys the static site from a released SQLite artifact. It runs on pushes that change site sources, manual dispatch, and dataset publication. A manual dispatch may name a release tag; otherwise it uses the latest `bnck-dataset-*` release.

The workflow scripts live in `.github/scripts/`. The `cloudflare-pages` deployment environment requires:

- `MINISIGN_SECRET_KEY`: an unencrypted CI signing key, set from the complete `minisign.key` file. Generate it with `minisign -G -W`; remove an existing passphrase with `minisign -C -W -s minisign.key`.
- `CLOUDFLARE_API_TOKEN`: an `Editor`-scoped token for the site Worker
- `CLOUDFLARE_ACCOUNT_ID`
- A committed `minisign.pub` matching the CI signing key

The dataset build and site deployment jobs use this environment, so its protection rules also apply to release signing and site deployment. The Worker updates production directly; its custom domain is attached in the Cloudflare dashboard.

Manual dispatch with an empty release input assigns the next `YYYY-MM-DD.N` from existing `bnck-dataset-*` tags. Supplying an identifier forces that release and bypasses the unchanged-dataset check. Tags are the sequence source of truth; deleting a tag allows its sequence to be reused.

The workflow passes `BNCK_CANDIDATE_ONLY=1` to generation and release assembly because acquisition jobs transfer only candidate artifacts and manifests. Curation-only changes that do not alter either canonical digest require a forced manual dispatch to publish a new release.

## Website development

Run the site against a compressed or uncompressed SQLite artifact:

```sh
just site-dev
```

The dev server binds `127.0.0.1:3000` by default, watches `web/` and the database, and reloads connected pages after a successful rebuild. Pass an address or database as `just site-dev 0.0.0.0:8080 var/other.sqlite.zst`.

Build and preview the static site:

```sh
just site-build
just site-preview
```

`site-preview` binds `127.0.0.1:3000` by default; pass `HOST:PORT` and a directory to change both. `site-build` clears its output directory.

Publish the output at the origin root of a case-sensitive host. The Cloudflare Worker in `web/worker.ts` runs first for `/projects/*` and `/executables/*`, serving `/identity.html` without changing the URL and redirecting trailing slashes and old export paths. Configure `404.html` as the error page rather than an SPA fallback, enable HTTP compression, and apply the generated `_headers` and `_redirects` files or equivalents. Deploy pages, the identity shell, and assets together and retain previous release assets so open pages finish loading their matching versions. See [website](docs/website.md) for routes, JSON actions, and deployment details.

## Responsibilities

Just recipes handle directory creation, HTTP downloads, archive extraction, job ordering, and paths between steps. They parse or reshape channel data only for decompression and archive extraction.

Rust code handles CSV, JSON, TOML, and wheel metadata parsing; acquisition snapshots and input digests; release selection; package and executable normalization; curation validation and resolution; and deterministic JSON Lines generation. It does not shell out or perform network requests.

TypeScript code in `web/` reads the SQLite artifact, renders the static pages and browser dataset, renders identity pages and JSON exports in the browser, and serves the development and preview interfaces.
