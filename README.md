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

It does not install, build, import, or execute packages. Registry and artifact downloads are performed by command-line tools in just recipes. Rust importers only read local files and write normalized data.

## Demo

Run a small live acquisition for ripgrep, ESLint, and Black:

```sh
just demo
```

The resulting files are:

```text
var/acquired/crates-io.jsonl
var/acquired/npm.jsonl
var/acquired/pypi.jsonl
var/providers.jsonl
```

`var/providers.jsonl` contains the provider records accepted by the sample curations under `curations/`.

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

Each distro can also be processed independently with `fetch-debian`, `fetch-fedora`, `fetch-arch`, `fetch-homebrew`, and the corresponding import job. The Fedora base URL, Debian suite and architecture, and Arch mirror and architecture can be changed through the environment variables defined in their justfiles.

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
- Release selection
- Package and executable normalization
- Curation validation and resolution
- Deterministic JSON Lines generation

Rust code does not shell out or perform network requests. Just recipes do not parse or reshape channel data beyond decompression and archive extraction.
