# Distribution channel acquisition

## Purpose

bnck acquires package observations and executable candidates from distribution channels. Project attribution remains curation: a matching package name, URL, source package, or repository is evidence, never automatic ownership.

This document describes the implemented acquisition pipeline. The snapshot schema is defined in [acquisition-format.md](acquisition-format.md); dataset release behavior is defined in [release-format.md](release-format.md).

## Shared behavior

### Scope

Every acquisition declares a channel scope:

- Debian: Suite, component, architecture
- Fedora: Release, repository, architecture
- Arch Linux: Repositories, architecture
- Homebrew: Tap, package kinds
- crates.io: Registry, release policy, package selection
- npm: Registry, release policy, package selection, catalog policy
- PyPI: Index, release policy, wheel policy, package selection, catalog policy

The scope is stored in the snapshot manifest. Distro acquisitions currently use one architecture and one component or repository selection per manifest.

### Snapshot output

Each import writes candidate JSON Lines and an adjacent snapshot manifest named `<candidates>.manifest.json`.

The manifest records:

- Channel, status, and scope
- Input paths, sizes, and SHA-256 digests
- Candidate artifact path, size, digest, and record count
- Package observations with observed or failed status; the schema also defines removed
- Importer version and optional source revision

Candidate records include the package release, executable name, evidence kind, variant, artifact digest when known, and project hints.

Current manifests do not record retrieval timestamps, original URIs, media types, signature results, or a channel-native snapshot boundary. The schema has an optional `uri` field, but current import commands leave it empty.

Distro and bulk crates.io imports produce complete snapshots on success. Planned npm and PyPI imports produce complete snapshots unless a package is unavailable or unsupported, in which case the snapshot is partial and the package carries a failure reason. Focused PyPI directory imports are always partial. A hard import failure aborts without writing a failed snapshot.

Retained inputs stay under `var/input/` by default; normalized candidates are written under `var/acquired/`. Dataset releases copy channel manifests but not candidate or input artifacts.

### Package identity and release selection

The canonical package identity is `(channel, package name)`. Each observation records the selected release and, when available, its architecture or other variant and artifact digest.

Current release selection:

- Debian: The selected stable and unstable suite indexes
- Fedora: The selected release's Everything repository
- Arch Linux: The current core and extra mirrors
- Homebrew: The Formula and Cask API's stable releases
- crates.io: The newest non-yanked, non-prerelease semantic version
- npm: The registry's `latest` dist-tag
- PyPI: The newest non-yanked stable release, followed by a supported wheel requirement

Yanked and prerelease releases are excluded. Deprecations, unpublishes, and repository removals are not emitted as package observations.

### Executable extraction

Distro-like channels treat paths in public command directories, normally `bin` and `sbin`, as executable candidates. Paths under `libexec`, manual pages, libraries, desktop files, and shell completions are excluded. Repository metadata does not reliably expose file type or mode, so a public path is evidence rather than proof that the path is executable.

Language repositories use declared entry points. Each observation carries one evidence kind:

- `declared-entry-point`: A crate target, npm `bin` key, or Python entry point
- `repository-file-listing`: A command path reported by distro repository metadata
- `static-recipe-action`: A command inferred from a Homebrew formula or cask declaration
- `artifact-metadata`: A command declaration read from an archive without installing it

Acquisition uses static data only. It does not install, build, import, or execute packages, evaluate formula code, run lifecycle scripts, or invoke build backends.

### Project hints

Current candidates carry a subset of available project information:

- `source_package`: Debian source package, Fedora source RPM, or Arch package base
- `homepage`
- `repository`
- `description`

License URLs, issue-tracker URLs, documentation URLs, and source archive URLs are not acquired. Hints support curator review but establish no ownership.

## Channels

### Debian

`just fetch-debian` fetches `InRelease`, `Packages.xz`, `Sources.xz`, `Contents-<architecture>.gz`, and `Contents-all.gz` for the configured stable suite and unstable suite. The indexes are decompressed before import.

Acquisition does not verify `InRelease` signatures or compare indexes against release-file checksums. The base URL, suite, component, and architecture come from the Debian justfile variables.

`import-debian` parses:

- `Packages` for package name, version, architecture, source package, description, homepage, and SHA-256 digest
- `Sources` for source-package homepage hints
- `Contents` for public command paths

The package identity is the binary package name. Stable and unstable records use the same `debian` channel and are stored in separate candidate files and manifests. `.deb` archive inspection is deferred.

### Fedora

`just fetch-fedora` downloads the selected repository's `repomd.xml`, plans the `primary` and `filelists` metadata objects, and downloads them. The default scope is one Fedora release's Everything repository.

`import-fedora` reads:

- `primary` for package name, version, architecture, summary or description, URL, source RPM, and checksum
- `filelists` for public command paths

Metadata checksums declared by `repomd.xml` are not verified. Metalink discovery, `updateinfo`, updates repositories, modular repositories, repository priority, and duplicate NEVRA handling are not implemented. Downloading source RPMs or inspecting spec files is deferred.

The package identity is the binary RPM name. The source RPM name is preserved as a hint.

### Arch Linux

`just fetch-arch` downloads the current `core` and `extra` repository databases and file databases from `ARCH_MIRROR`. It extracts the file databases; the importer reads the extracted `desc` and `files` data.

`import-arch` reads package name, version, architecture, package base, URL, description, and SHA-256 digest from `desc`, and public command paths from `files`. Database signatures, Arch Linux Archive dates, and `.db` metadata import are not implemented. Checking out packaging repositories or `PKGBUILD` files is deferred, as is package-archive inspection.

The package identity is the binary package name, not the package base. The package base is preserved as a hint.

### Homebrew

`just fetch-homebrew` downloads the Formula API and Cask API responses. `import-homebrew` reads:

- Formula name, tap, stable version, homepage, repository, description, and `executables` list
- Cask token, version, homepage, description, and declared `binary` artifacts

When `--core` points at a Homebrew tap checkout and the Formula API has no `executables` list, the importer statically parses direct `bin.install`, `sbin.install`, and install-symlink actions. The standard just recipes do not fetch a tap checkout.

Homebrew package identities are fully qualified as `tap/name`. Bottle metadata, bottle digests, cask artifact digests, and tap commit pinning are not recorded. Bottle archive inspection and `brew install` are deferred.

### crates.io

`just fetch-crates-dump` downloads the crates.io database dump and extracts `crates.csv`, `versions.csv`, and dump metadata. `import-crates-dump` reads the two CSV tables.

When curations are supplied, the importer narrows crates by curated package names, project names, repository basenames, and normalized repository or homepage URLs. For each selected crate it chooses the newest non-yanked, non-prerelease semantic version, records the release, and reads `bin_names` as declared executable candidates.

The crates.io index is not acquired. Dump timestamps, index commits, license, readme, features, `rust_version`, edition, and `has_lib` are not imported. Crate archives are not downloaded.

### npm

`just fetch-npm` plans one request per explicitly curated npm package and fetches `https://registry.npmjs.org/<encoded-package>/latest`. Responses and HTTP status sidecars are retained. `just import-npm` reads the selected release manifests.

The importer records the exact release version, package name, project fields, and `bin` declarations. An object-valued `bin` supplies command keys; a string-valued `bin` supplies the unscoped package basename. A `directories.bin` declaration without a resolved `bin` mapping is recorded as an unsupported extraction failure.

No packument, full version history, `_all_docs` scan, name search, or tarball download is performed. Lifecycle scripts, dependencies, and platform constraints are not used to infer commands.

### PyPI

Acquisition fetches the PEP 691 JSON response from `https://pypi.org/simple/<normalized-project>/` for each explicitly curated PyPI package. It selects the newest non-yanked stable release, then prefers a platform-independent wheel, smaller declared size, and filename. One wheel per selected release is downloaded and cached by SHA-256.

`import-pypi` reads `.dist-info/METADATA`, `.dist-info/entry_points.txt`, and `.data/scripts/` from the selected wheel. Commands come from `console_scripts` and `gui_scripts` entry points and from script files directly under `.data/scripts/`.

The global project index is not fetched. PEP 658 metadata sidecars, source distributions, and multiple wheel variants are not inspected. A current release without a wheel is recorded as unsupported; acquisition does not fall back to an older release.

## Current limitations

- No signature verification
- No retrieval timestamps, original URIs, media types, or channel-native snapshot boundaries
- No incremental acquisition or tombstones; imports replace the preceding snapshot
- No `removed` package observations, and hard failures do not produce a `failed` snapshot
- No archive inspection beyond selected PyPI wheel metadata
- No acquisition metrics
- Release artifacts include channel manifests but not the candidate or input files they reference

## Curation boundary

Importers never decide that packages from different channels belong to the same project from names or URLs alone. They emit hints and package observations; curations accept or reject project attribution and assign individual executables when a package mixes projects.

## Further specifications

- Debian repository format: [Debian Repository Format](https://wiki.debian.org/DebianRepository/Format)
- Fedora repository metadata: [DNF repository metadata](https://dnf.readthedocs.io/en/latest/repo_metadata.html)
- Homebrew metadata: [Formula API](https://formulae.brew.sh/docs/api/)
- crates.io dump: [Data access](https://crates.io/data-access)
- npm command declarations: [`package.json` `bin`](https://docs.npmjs.com/cli/configuring-npm/package-json#bin)
- PyPI Simple API: [Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/)
- Python entry points: [Entry points specification](https://packaging.python.org/en/latest/specifications/entry-points/)
