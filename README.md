# bnck

bnck builds an executable-name database from distribution-channel metadata and checked-in curations.

## MVP

Imported channels:

- Debian from `Packages`, `Sources`, and `Contents` indexes
- Fedora from DNF primary and filelists metadata
- Arch Linux from repository file databases
- Homebrew from formula and cask API metadata
- crates.io from the database dump
- npm from the `latest` release manifests of curated packages
- PyPI from Simple API metadata and statically inspected wheels of curated packages

TOML curations resolve these acquisitions into canonical provider JSON Lines. bnck does not install, build, import, or execute packages.

## SQLite artifact

`bnck.sqlite.zst` is the compressed SQLite consumer artifact: provider relations, curated project links, and release metadata.

It supports the queries in [`docs/release-format.md`](docs/release-format.md). Decompress before opening it with SQLite:

```sh
zstd -d var/bnck.sqlite.zst -o var/bnck.sqlite
sqlite3 var/bnck.sqlite \
	"SELECT project, channel, package FROM providers WHERE executable = 'rg' ORDER BY project, channel, package;"
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks, acquisition jobs, curation workflows, release generation and signing, and website deployment.

## Acknowledgements

Initial seed curations used [Terminal Trove](https://terminaltrove.com/) and [tldr pages](https://tldr.sh/) as discovery hints. Each project was added after cross-checking multiple attributes and confirming that at least one channel packages it.

## License

bnck source code, build tooling, curations, dataset artifacts, documentation, and website content are licensed under the [ISC License](LICENSE).

Upstream package metadata remains subject to its source terms. The acknowledgements above record discovery sources and do not impose additional license conditions.
