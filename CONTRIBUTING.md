# Contributing

Covers development, acquisition jobs, curations, release publication, and the website. See [README.md](README.md) for the project overview and artifact usage.

## Development checks

Run `just check` with Rust, just, and Bun installed. It runs formatting checks, Clippy, Rust tests, browser unit tests, acquisition-job tests, and JavaScript/SQLite search comparisons:

```sh
just check
```

## Demo

```sh
just demo
```

Acquires curated npm packages and Black into a fresh `var/demo/` directory and prints its path. It contains `acquired/npm.jsonl`, `acquired/pypi.jsonl`, their manifests, and `providers.jsonl`. Demo jobs never change production acquisitions, provider output, or default input selections.

Provider records are accepted only through the checked-in curations under `curations/`. Dataset generation requires adjacent snapshot manifests; rerun the import jobs after upgrading from candidate-only acquisition files.

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

Each distro can be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the corresponding import job. Debian acquisition covers the configured primary suite, stable by default, and unstable/sid. The Fedora base URL, Debian suites and architecture, and Arch mirror and architecture are set through environment variables in their justfiles.

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

This writes `var/bnck.sqlite.zst` after validating the canonical JSON Lines encoding, ordering, and uniqueness. The SQLite metadata table records the SHA-256 of the uncompressed JSON Lines, provider count, release identifier, generation time, generator version, and Git revision. Curated homepages, repositories, documentation, issue trackers, and additional links are included as project metadata.

Assemble, sign, verify, and package a release:

```sh
just release 2026-04-16.1
```

This writes the release directory and `var/releases/bnck-2026-04-16.1.tar.zst`. It uses `minisign.key` and `minisign.pub` by default and requires `minisign` and `zstd`. The secret key is never committed. See the [README](README.md) for public release verification.

## Release automation

The `Release` workflow runs daily at 03:17 UTC and can also be dispatched manually. It runs one acquisition job per channel, generates the dataset, and publishes only when the canonical provider digest differs from the latest dataset release. The site is rebuilt and deployed from the published SQLite artifact even when the dataset is unchanged.

Job steps call the scripts in `.github/scripts/`; the workflow supplies their inputs and secrets through step environment variables.

Manually dispatching with an empty release input assigns the next `YYYY-MM-DD.N` from the existing `bnck-dataset-*` tags. Supplying an identifier forces that release and bypasses the unchanged-dataset check. The tags are the sequence source of truth; deleting a tag allows its sequence to be reused.

Required `cloudflare-pages` deployment environment configuration:

- `MINISIGN_SECRET_KEY` secret: an unencrypted CI signing key. Generate it with `minisign -G -W`; an empty passphrase still encrypts the key. Remove an existing passphrase with `minisign -C -W -s minisign.key`.
- `CLOUDFLARE_API_TOKEN` secret with account-level `Cloudflare Pages` edit permission
- `CLOUDFLARE_ACCOUNT_ID` secret
- `CLOUDFLARE_PROJECT_NAME` environment variable
- A committed `minisign.pub` matching the CI signing key

The build and deployment jobs use this environment, so environment protection rules also apply to release signing. The Pages deployment targets the repository default branch; the Pages project's production branch must match it.

The workflow tags releases `bnck-dataset-<CalVer>` so dataset releases remain distinct from other releases in the repository. It passes `BNCK_CANDIDATE_ONLY=1` to generation and release assembly because acquisition jobs transfer only candidate artifacts and manifests.

The unchanged check compares the canonical provider digest. Curation-only changes that do not alter provider relations require a manual dispatch to force a new release.

## Website development

Generate a fully static website from the compressed or uncompressed SQLite artifact, then preview it locally with Bun:

```sh
just generate-site
just preview
```

The preview binds `127.0.0.1:3000` by default; pass `HOST:PORT`, as in `just preview var/site 0.0.0.0:8080`, to change it. `generate-site` clears its output directory before rebuilding. The generator itself refuses an existing output directory, so use a fresh path when invoking it directly:

```sh
cargo run -- generate-site \
	--database var/bnck.sqlite.zst \
	--output var/site-next
bun web/preview.js var/site-next 127.0.0.1:3000
```

Production needs no application server or browser database runtime. Publish the output at the origin root of a case-sensitive static host with directory index support: `/projects/helix` resolves to `projects/helix/index.html`, and hosts may redirect to a trailing slash. Configure the generated `404.html` as the error page rather than an SPA fallback, enable HTTP compression, and apply the generated `_headers` file or equivalents. Deploy pages and assets together and retain previous application directories and dataset files so open pages finish loading their matching versions. See [static deployment](docs/website.md#static-deployment) for details.

HTML templates live under `web/templates/` and the stylesheet is `web/style.css`. Debug builds read templates, stylesheets, and browser modules from disk during generation; release builds embed them. Rebuild the static output after edits. `just serve` keeps the SQLite-backed server-rendered interface for development and query comparisons.

## Responsibilities

Just recipes handle directory creation, HTTP downloads, archive extraction, job ordering, and passing local paths between steps. They parse or reshape channel data only for decompression and archive extraction.

Rust code handles CSV, JSON, TOML, and wheel metadata parsing; acquisition snapshot manifests, package observations, and input digests; release selection; package and executable normalization; curation validation and resolution; and deterministic JSON Lines generation. It does not shell out or perform network requests.
