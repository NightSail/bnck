# bnck

bnck builds an executable-name database from distribution-channel metadata and checked-in curations.

## MVP

The MVP imports all initial channels:

- Debian from `Packages`, `Sources`, and `Contents` indexes
- Fedora from DNF primary and filelists metadata
- Arch Linux from repository file databases
- Homebrew from formula and cask API metadata
- crates.io from the database dump
- npm from package packuments
- PyPI from Simple API metadata and statically inspected wheels
- TOML curation resolution into canonical provider JSON Lines

It does not install, build, import, or execute packages.

## SQLite artifact

`bnck.sqlite.zst` is the compressed SQLite consumer artifact. It includes provider relations, curated project links, and release metadata.

The artifact supports the queries documented in [`docs/release-format.md`](docs/release-format.md). Decompress it before opening it with SQLite:

```sh
zstd -d var/bnck.sqlite.zst -o var/bnck.sqlite
sqlite3 var/bnck.sqlite \
	"SELECT project, channel, package FROM providers WHERE executable = 'rg' ORDER BY project, channel, package;"
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks, acquisition jobs, curation workflows, release generation and signing, website development and deployment, and implementation responsibilities.

## Acknowledgements

Project names for the initial seed curations were sourced from [Terminal Trove](https://terminaltrove.com/) and [tldr pages](https://tldr.sh/) as discovery hints. Projects were added after cross-checking multiple attributes and confirming that at least one distribution channel provides a package.

We thank both projects and their contributors for their prior work cataloguing and documenting terminal tools.
