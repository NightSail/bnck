# bnck

bnck builds an executable-name database from distribution-channel metadata and manual curations.

A searchable web interface is at <https://bnck.dev/>. Datasets are published under <https://bnck.dev/data/>, with the latest always at <https://bnck.dev/data/latest>.

## Dataset

Each release contains:

- `providers.jsonl.zst`: Canonical provider relations
- `bnck.sqlite.zst`: SQLite artifact with provider relations, curated project links, and release metadata
- `LICENSE`, `SHA256SUMS`, and `SHA256SUMS.minisig`
- Channel manifests and candidate JSON Lines files

GitHub Releases provides each release as a `.tar.zst` archive.

The provider relation is `(project, executable, channel, package)`. The JSON Lines artifact defines the logical dataset identity; SQLite is the recommended artifact for applications.

Coverage is curated and channel-scoped. npm and PyPI include only explicitly curated packages. Channel counts describe accepted dataset records, not the complete contents of each channel. A missing record means the relation was not observed in that release, not that it is unavailable. The dataset does not specify package versions, platform support, or installation instructions.

## Verify a release

Releases are signed with minisign public key `RWQSlyUkDwCyJF9mBUymWcUWbOu1dLyAHKSm/W0PVqCSPKB1091ZzVeO`.

Download a release from the canonical release page and run the following from the extracted release directory:

```sh
sha256sum -c SHA256SUMS
minisign -Vm SHA256SUMS -P RWQSlyUkDwCyJF9mBUymWcUWbOu1dLyAHKSm/W0PVqCSPKB1091ZzVeO
```

## Query

Decompress the SQLite artifact then run queries on it:

```sh
zstd -d bnck.sqlite.zst -o bnck.sqlite
sqlite3 bnck.sqlite \
	"SELECT project, channel, package FROM providers WHERE executable = 'rg' ORDER BY project, channel, package;"
```

See [`docs/release-format.md`](docs/release-format.md) for the schema and supported queries.

## Channels

Imported distribution channels:

- Distros and OSes:
	- Debian, from `Packages`, `Sources`, and `Contents` indexes
	- Fedora, from DNF primary and filelists metadata
	- Arch Linux, from repository file databases
	- Homebrew, from formula and cask API metadata
- Language registries:
	- crates.io, from the database dump
	- npm, from the `latest` release manifests of curated packages
	- PyPI, from Simple API metadata and statically inspected wheels of curated packages

TOML curations resolve these acquisitions into canonical provider JSON Lines. bnck does not install, build, import, or execute packages.

## Documentation

- [Core design](docs/design.md)
- [Acquisition snapshot format](docs/acquisition-format.md)
- [Distribution channel acquisition](docs/channels.md)
- [Curation format](docs/curations.md)
- [Language registry acquisition](docs/language-registries.md)
- [Dataset release format](docs/release-format.md)
- [Website](docs/website.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks, acquisition jobs, curation workflows, release generation and signing, and website deployment. See [CHANGELOG.md](CHANGELOG.md) for notable changes and schema transitions.

## Acknowledgements

Initial seed curations used [Terminal Trove](https://terminaltrove.com/) and [tldr pages](https://tldr.sh/) as discovery hints. Each project was added after cross-checking multiple attributes and confirming that at least one channel packages it.

## License

bnck source code, build tooling, curations, dataset artifacts, documentation, and website content are licensed under the [ISC License](LICENSE).

Upstream package metadata remains subject to its source terms. The acknowledgements above record discovery sources and do not impose additional license conditions.
