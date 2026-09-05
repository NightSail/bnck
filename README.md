# bnck

bnck builds an executable-name database from distribution-channel metadata and checked-in curations.

## MVP

The MVP imports all initial channels:

- Debian from `Packages`, `Sources`, and `Contents` indexes
- Fedora from DNF primary and filelists metadata
- Arch Linux from repository file databases
- Homebrew from formula and cask API metadata
- crates.io from the database dump or per-crate API responses
- npm from package packuments
- PyPI from Simple API metadata and statically inspected wheels
- TOML curation resolution into canonical provider JSON Lines

It does not install, build, import, or execute packages. Registry and artifact downloads are performed by command-line tools in just recipes. Rust importers only read local files and write normalized data. Each import writes candidate JSON Lines plus an adjacent, content-identified [acquisition snapshot manifest](docs/acquisition-format.md).

## Demo

Run a small live acquisition for ripgrep, ESLint, and Black:

```sh
just demo
```

The resulting files include:

```text
var/acquired/crates-io.jsonl
var/acquired/crates-io.jsonl.manifest.json
var/acquired/npm.jsonl
var/acquired/npm.jsonl.manifest.json
var/acquired/pypi.jsonl
var/acquired/pypi.jsonl.manifest.json
var/providers.jsonl
```

`var/providers.jsonl` contains the provider records accepted by the sample curations under `curations/`. Dataset generation requires the adjacent snapshot manifests; rerun the import jobs after upgrading from candidate-only acquisition files.

## Jobs

List available jobs:

```sh
just
```

Fetch and import individual packages:

```sh
just fetch-crate ripgrep
just fetch-npm eslint
just fetch-pypi black

just import-crates-api
just import-npm
just import-pypi
just generate
```

Fetch and import distro metadata:

```sh
just fetch-distros
just import-distros
```

Each distro can also be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the corresponding import job. Debian acquisition includes both the configured primary suite, stable by default, and unstable/sid. The Fedora base URL, Debian suites and architecture, and Arch mirror and architecture can be changed through the environment variables defined in their justfiles.

For a full crates.io import, download and process the crates.io database dump:

```sh
just fetch-crates-dump
just import-crates-dump
```

The crates.io database dump and distro metadata sets are large. Fetch jobs retain their inputs under `var/input/`, while normalized acquisition output is written under `var/acquired/`.

Show acquisition and provider record counts with:

```sh
just stats
```

## SQLite artifact

Generate the compressed SQLite consumer artifact from the canonical provider JSON Lines file:

```sh
just generate-sqlite 2026-04-16.1
```

This writes `var/bnck.sqlite.zst`. The command validates the canonical JSON Lines encoding, ordering, and uniqueness before building the database. It records the SHA-256 of the uncompressed JSON Lines content, provider count, release identifier, generation time, generator version, and Git revision in the SQLite metadata table. It also reads project metadata from `curations/` and includes curated homepages, repositories, documentation, issue trackers, and additional links in the derived SQLite artifact.

The artifact supports the queries documented in [`docs/release-format.md`](docs/release-format.md). Decompress it before opening it with SQLite:

```sh
zstd -d var/bnck.sqlite.zst -o var/bnck.sqlite
sqlite3 var/bnck.sqlite \
	"SELECT project, channel, package FROM providers WHERE executable = 'rg' ORDER BY project, channel, package;"
```

Assemble the release directory, including compressed provider and SQLite artifacts, acquisition manifests, `manifest.json`, and `SHA256SUMS`:

```sh
just release 2026-04-16.1
```

Release assembly does not expose a signing key to the generator. Produce `SHA256SUMS.minisig` with the separately managed release signing key before publication:

```sh
just sign-release var/releases/bnck-2026-04-16.1 /path/to/minisign.key
```

## Website

Start the local search and browsing interface with the generated SQLite artifact:

```sh
just serve
```

The site listens on `http://127.0.0.1:3000` by default. Search executable names and canonical project IDs, or leave the name empty to browse. Combined results group executables by owning project, with matching commands first and expandable executable lists. Lookup-type filters retain flat executable and project indexes, and distribution-channel filters narrow the results. Paginated results link to exact-name detail pages showing every accepted channel and package relation. All navigation works without JavaScript.

The dataset page at `/about` defines result fields, explains coverage limits, and lists channel counts and the release date. The [website design](docs/website.md) documents the views and search contract.

HTML templates live under `web/templates/`, and the stylesheet is `web/style.css`. Debug builds reload these files from disk on every request, so edits appear after refreshing the browser without restarting the server. Release builds embed the same files in the executable.

The server accepts either the compressed release artifact or an uncompressed SQLite database. A compressed artifact is decompressed to a temporary file, then every connection is opened read-only:

```sh
cargo run -- serve \
	--database var/bnck.sqlite.zst \
	--address 127.0.0.1:3000
```

## Curation suggestions

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

Suggestions are ranked using canonical repository and homepage URLs, package and source-package names, executable names, and existing curation ownership. Each result reports its matching evidence. The commands read only local acquisition and curation data and do not modify curations.

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

## Acknowledgements

Project names for the initial seed curations were sourced from [Terminal Trove](https://terminaltrove.com/) and [tldr pages](https://tldr.sh/) as discovery hints. Projects were added after cross-checking multiple attributes and confirming that at least one distribution channel provides a package.

We thank both projects and their contributors for their prior work cataloguing and documenting terminal tools.
