# Contributing

Covers development, acquisition jobs, curations, release publication, and the website. See [README.md](README.md) for the project overview and artifact usage.

## Development checks

Run `just check` with Rust, just, and Bun installed. Install the TypeScript tooling once, then run the checks. They cover formatting, Clippy, Rust tests, TypeScript types, browser unit tests, acquisition-job tests, and browser/SQLite search comparisons:

```sh
bun install
just check
```

## Demo

```sh
just demo
```

Acquires curated npm packages and Black into a fresh `var/demo/` directory and prints its path. It contains `acquired/npm.jsonl`, `acquired/pypi.jsonl`, their manifests, `providers.jsonl`, and `names.jsonl`. Demo jobs never change production acquisitions, provider output, name associations, or default input selections.

Provider records and name associations are accepted only through the checked-in curations under `curations/`. Dataset generation requires adjacent snapshot manifests; rerun the import jobs after upgrading from candidate-only acquisition files.

## Acquisition jobs

Command-line tools in just recipes perform registry and artifact downloads. Rust importers only read local files and write normalized data: candidate JSON Lines plus an adjacent, content-identified [acquisition snapshot manifest](docs/acquisition-format.md).

List available jobs:

```sh
just
```

Fetch and import all curated npm packages:

```sh
just fetch-npm
just import-npm
just generate
```

`fetch-npm` creates a fresh planned snapshot under `var/input/npm/` and selects it only after all requests finish; `import-npm` imports that selection. For explicit snapshots, pass the path to both. Resume interrupted downloads with `just resume-npm <snapshot>`, then import that path explicitly. An optional second argument to `import-npm` sets the output path.

A focused PyPI run is also available:

```sh
just fetch-pypi black
just import-pypi
```

It produces a partial acquisition; use `just demo` to keep a focused run separate from production data.

Fetch and import distro metadata:

```sh
just fetch-distros
just import-distros
```

Each distro can be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the corresponding import job. Debian acquisition covers the configured primary suite, stable by default, and unstable/sid. Debian fetch requires `gpgv` and `gpg`, and downloads the active archive and release signing keys from `https://ftp-master.debian.org/keys/` into `var/cache/debian/archive-keyring.gpg`; set `DEBIAN_KEYRING` for another keyring. Import jobs verify Debian release checksums and Fedora `repomd.xml` metadata checksums. The Fedora base URL, Debian suites and architecture, and Arch mirror and architecture are set through environment variables in their justfiles.

Fetch jobs write `acquisition-context.tsv` files with retrieval URLs, media types, timestamps, signature results, and channel boundaries. Import recipes pass them with `--context`. To report package removals against a preceding acquisition or extracted release, pass `--previous <path>` to an import command; partial snapshots never infer removals.

Acquire all language channels:

```sh
just fetch-languages
just import-languages
just generate
```

Both jobs run without arguments; imports use the most recently completed fetch. crates.io uses its full database dump; npm and PyPI fetch metadata only for explicitly curated packages. See [language registry acquisition](docs/language-registries.md) for individual jobs, resuming downloads, release selection, and coverage limits.

The crates.io dump and distro metadata sets are large. Fetch jobs retain inputs under `var/input/`, selected PyPI wheels are cached by digest under `var/cache/pypi/wheels/`, and normalized acquisition output goes to `var/acquired/`.

Show acquisition and provider record counts:

```sh
just stats
```

Fetch jobs end with the on-disk size of the inputs they retain; import, generate, and release jobs end with the size of the artifacts they write. Each summary line names the path and its IEC size.

## Curation diagnostics

`just generate` and `just release` write acquisition diagnostics to `var/diagnostics/report.json` and compare against `var/diagnostics/baseline.json`. The first run creates the baseline. Warnings report new or removed executables, changed package sets, projects without observed packages, and project metadata conflicts; they do not block generation. Pass `--baseline` a previous acquisitions or release directory to compare against a fixed historical state without updating it.

## Curations

Project definitions, package ownership rules, and name associations live under `curations/`. The [curation format](docs/curations.md) documents their schema, exceptional executable rules, name associations, and validation requirements. The [curation guide](docs/curation-guide.md) defines inclusion criteria, evidence standards, correction rules, and the review checklist.

Curators can propose changes by editing the relevant files under `curations/` and opening a pull request; no build is required to start. Every new rule needs a specific reason and durable public evidence. Validate with `just generate` when acquisition data covers the affected channel, or state the missing validation in the pull request.

### Hint ingestion

Normalized Terminal Trove and tldr hints live under `var/hints/`. Preview automatic project creation and package extension from local acquisitions:

```sh
just ingest-hints
```

Apply the same plan and write an audit report:

```sh
just apply-hints
```

The command requires URL identity and package or executable evidence, skips packages already claimed by a curation, and does not rewrite existing project metadata. Review the preview before applying, especially when a hint batch is large. Add reviewed exceptions to `curations/rejections.toml`; see [hint rejections](docs/curations.md#hint-rejections).

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

Review the evidence and upstream identity before adding attributions. Name matches and copied repository URLs alone do not establish ownership. Suggestions rank results by canonical repository and homepage URLs, package and source-package names, executable names, and existing curation ownership, and report the matching evidence. The commands read only local acquisition and curation data.

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

This writes the release directory and `var/releases/bnck-2026-04-16.1.tar.zst`. It uses `minisign.key` and `minisign.pub` by default and requires `minisign` and `zstd`. The secret key is never committed. See the [README](README.md) for public release verification.

## Release automation

The `Dataset` workflow runs daily at 03:17 UTC and can also be dispatched manually. It runs one acquisition job per channel, generates the dataset, and publishes only when a canonical provider or name association digest differs from the latest dataset release. It does not build the site. When it publishes a new release, it dispatches the `Site` workflow with that release tag.

The `Site` workflow builds and deploys the static site from a released SQLite artifact. It runs on pushes that change site sources, when dispatched manually, and when the dataset workflow publishes a release. A manual dispatch may name a dataset release tag; otherwise it uses the latest `bnck-dataset-*` release. Site changes therefore do not trigger acquisition or a dataset release.

Job steps call the scripts in `.github/scripts/`; the workflows supply their inputs and secrets through step environment variables.

Manually dispatching the `Dataset` workflow with an empty release input assigns the next `YYYY-MM-DD.N` from the existing `bnck-dataset-*` tags. Supplying an identifier forces that release and bypasses the unchanged-dataset check. The tags are the sequence source of truth; deleting a tag allows its sequence to be reused.

Required `cloudflare-pages` deployment environment configuration:

- `MINISIGN_SECRET_KEY` secret: an unencrypted CI signing key, set from the complete `minisign.key` file. Generate it with `minisign -G -W`; an empty passphrase still encrypts the key. Remove an existing passphrase with `minisign -C -W -s minisign.key`.
- `CLOUDFLARE_API_TOKEN` secret with `Editor` access scoped to the site Worker
- `CLOUDFLARE_ACCOUNT_ID` secret
- A committed `minisign.pub` matching the CI signing key

The dataset build and site deployment jobs use this environment, so environment protection rules also apply to release signing and site deployment. The Worker deployment updates production directly; its custom domain is attached in the Cloudflare dashboard, so the deployment token needs no zone-level Workers Routes permission.

The dataset workflow tags releases `bnck-dataset-<CalVer>` so dataset releases remain distinct from other releases in the repository. It passes `BNCK_CANDIDATE_ONLY=1` to generation and release assembly because acquisition jobs transfer only candidate artifacts and manifests.

The unchanged check compares both canonical JSON Lines digests. Curation-only changes that do not alter either digest require a manual dispatch to force a new release.

## Website development

Run the website directly against the compressed or uncompressed SQLite artifact:

```sh
just site-dev
```

The dev server binds `127.0.0.1:3000` by default. It builds the site into a temporary directory, watches `web/` and the database artifact, and reloads connected pages after a successful rebuild. Pass a different address or database as `just site-dev 0.0.0.0:8080 var/other.sqlite.zst`.

Generate a static site and serve that exact output with Bun:

```sh
just site-build
just site-preview
```

The preview binds `127.0.0.1:3000` by default; pass `HOST:PORT`, as in `just site-preview 0.0.0.0:8080 var/site`, to change it. `site-build` clears its output directory before rebuilding. The generator itself refuses an existing output directory, so use a fresh path when invoking it directly:

```sh
bun web/generate.ts \
	--database var/bnck.sqlite.zst \
	--output var/site-next
bun web/preview.ts var/site-next 127.0.0.1:3000
```

Production needs no application server or browser database runtime. Publish the output at the origin root of a case-sensitive static host with directory index support: `/projects/helix` resolves to `projects/helix/index.html`, and hosts may redirect to a trailing slash. Configure the generated `404.html` as the error page rather than an SPA fallback, enable HTTP compression, and apply the generated `_headers` file or equivalents. Deploy pages and assets together and retain previous application directories and dataset files so open pages finish loading their matching versions. See [static deployment](docs/website.md#static-deployment) for details.

Page renderers live in `web/pages.ts`, shared markup in `web/components.ts`, and the stylesheet is `web/style.css`. Browser entry points are TypeScript modules under `web/`; the generator bundles them with Bun. Source edits automatically rebuild the dev site, while a static output needs `just site-build` again.

## Responsibilities

Just recipes handle directory creation, HTTP downloads, archive extraction, job ordering, and passing local paths between steps. They parse or reshape channel data only for decompression and archive extraction.

Rust code handles CSV, JSON, TOML, and wheel metadata parsing; acquisition snapshot manifests, package observations, and input digests; release selection; package and executable normalization; curation validation and resolution; and deterministic JSON Lines generation. It does not shell out or perform network requests.

TypeScript code in `web/` reads the SQLite artifact, renders the static site and JSON exports, and serves the development and preview interfaces.
