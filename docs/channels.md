# Distribution channel acquisition

bnck acquires package observations and executable candidates from distribution channels. Project attribution remains curation: a matching package name, URL, source package, or repository is evidence, never automatic ownership.

The snapshot schema is defined in [acquisition-format.md](acquisition-format.md); dataset release behavior is defined in [release-format.md](release-format.md).

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

The scope is stored in the snapshot manifest. Distro acquisitions use one architecture and one component or repository selection per manifest.

### Snapshot output

Package identity is `(channel, package name)`. Each observation records the selected release and, when available, its architecture or other variant and artifact digest.

Each import writes candidate JSON Lines and `<candidates>.manifest.json`. Fetch recipes write `acquisition-context.tsv` beside their inputs and pass it to import commands with `--context`; derived files omit the URI. Inputs omitted from the context fall back to local modification times. Paths under the repository root are recorded relative to it.

Distro and bulk crates.io imports produce complete snapshots on success. Planned npm and PyPI imports are partial when a package is unavailable or unsupported, with a failure reason in the package observation. Focused PyPI directory imports are always partial.

A hard import failure writes a `failed` snapshot with an empty candidate artifact and the error summary. Passing `--previous` a preceding acquisitions or release directory adds `removed` observations for complete snapshots with the same channel and scope. Partial snapshots never infer removals.

Retained inputs stay under `var/input/`; normalized candidates are written under `var/acquired/`. Dataset releases copy channel manifests and candidate files, but not raw channel inputs.

### Executable extraction

Distro-like channels treat paths in public command directories, normally `bin` and `sbin`, as executable candidates. Paths under `libexec`, manual pages, libraries, desktop files, and shell completions are excluded. Repository metadata does not reliably expose file type or mode, so a public path is evidence rather than proof that the path is executable.

Names created only by install-time mechanisms, such as alternatives or post-install symlinks, are not observed and require reviewed curation additions. Multicall applets and shell builtins are not candidates unless the channel lists them as public command paths.

Language repositories use declared entry points. Each observation carries one evidence kind:

- `declared-entry-point`: A crate target, npm `bin` key, or Python entry point
- `repository-file-listing`: A command path reported by distro repository metadata
- `static-recipe-action`: A command inferred from a Homebrew formula or cask declaration
- `artifact-metadata`: A command declaration read from an archive without installing it

Acquisition uses static data only. It does not install, build, import, or execute packages, evaluate formula code, run lifecycle scripts, or invoke build backends.

### Project hints

Candidates carry `source_package` (Debian source package, Fedora source RPM, or Arch package base), `homepage`, `repository`, and `description`. Hints support curator review but establish no ownership.

## Channels

### Debian

`just fetch-debian` fetches `InRelease`, `Packages.xz`, `Sources.xz`, `Contents-<architecture>.gz`, and `Contents-all.gz` for the configured stable and unstable suites. It downloads the active archive and release signing keys from `https://ftp-master.debian.org/keys/` into `var/cache/debian/archive-keyring.gpg`, then verifies `InRelease` with `gpgv` before downloading indexes. Set `DEBIAN_KEYRING` to use another keyring, and update the key list in the Debian justfile when Debian publishes new active keys.

`import-debian` verifies every downloaded index against the SHA-256 checksums and sizes in `InRelease`. The snapshot boundary records the `InRelease` release date, and its input signature records the verifying key fingerprint. The base URL, suite, component, and architecture come from justfile variables. Signature verification does not cover package archives because they are not downloaded.

`import-debian` parses:

- `Packages` for package name, version, architecture, source package, description, homepage, and SHA-256 digest
- `Sources` for source-package homepage hints
- `Contents` for public command paths

The package identity is the binary package name. Stable and unstable records use the same `debian` channel and are stored in separate candidate files and manifests.

### Fedora

`just fetch-fedora` downloads the selected repository's `repomd.xml`, plans the `primary` and `filelists` metadata objects, and downloads them. The default scope is one Fedora release's Everything repository.

`import-fedora` reads package name, version, architecture, summary or description, URL, source RPM, and checksum from `primary`, and public command paths from `filelists`. It verifies both metadata objects against the sizes and SHA-256 checksums declared by `repomd.xml`. The snapshot boundary records the `repomd.xml` revision and metadata checksums. Fedora mirrors do not publish a `repomd.xml` signature at the configured endpoint, so repository XML is trusted through HTTPS and verified by its declared checksums.

The package identity is the binary RPM name. The source RPM name is preserved as a hint.

### Arch Linux

`just fetch-arch` downloads the current `core` and `extra` repository databases and file databases from `ARCH_MIRROR`. It extracts the file databases; the importer reads the extracted `desc` and `files` data.

`import-arch` reads package name, version, architecture, package base, URL, description, and SHA-256 digest from `desc`, and public command paths from `files`. The snapshot boundary records the HTTP `Last-Modified` value of each downloaded database. The configured mirrors do not publish database signatures, so verification is limited to HTTPS transport and archive extraction.

The package identity is the binary package name, not the package base. The package base is preserved as a hint.

### Homebrew

`just fetch-homebrew` downloads the Formula API and Cask API responses. `import-homebrew` reads:

- Formula name, tap, stable version, homepage, repository, description, and `executables` list
- Cask token, version, homepage, description, and declared `binary` artifacts

When `--core` points at a Homebrew tap checkout and the Formula API has no `executables` list, the importer statically parses direct `bin.install`, `sbin.install`, and install-symlink actions. The standard recipes do not fetch a tap checkout.

Package identities are fully qualified as `tap/name`. Snapshots record the Formula and Cask API `tap_git_head` values as boundaries and reject disagreement within either response.

### crates.io

`just fetch-crates-dump` downloads the crates.io database dump and extracts `crates.csv`, `versions.csv`, and dump metadata. `import-crates-dump` reads the two CSV tables.

With curations, the importer narrows crates by curated package names, project names, repository basenames, and normalized repository or homepage URLs. For each selected crate it chooses the newest non-yanked, non-prerelease semantic version, records the release, and reads `bin_names` as declared executable candidates. The crates.io index and crate archives are not acquired.

### npm

`just fetch-npm` plans one request per explicitly curated npm package and fetches `https://registry.npmjs.org/<encoded-package>/latest`. Responses and HTTP status sidecars are retained. `just import-npm` reads the selected release manifests.

The importer records the exact release version, package name, project fields, and `bin` declarations. An object-valued `bin` supplies command keys; a string-valued `bin` supplies the unscoped package basename. A `directories.bin` declaration without a resolved `bin` mapping is recorded as an unsupported extraction failure. No packument, full version history, name search, or tarball download is performed, and lifecycle scripts are not evaluated.

### PyPI

Acquisition fetches the PEP 691 JSON response from `https://pypi.org/simple/<normalized-project>/` for each explicitly curated PyPI package. It selects the newest non-yanked stable release, then prefers a platform-independent wheel, smaller declared size, and filename. One wheel per selected release is downloaded and cached by SHA-256.

`import-pypi` reads `.dist-info/METADATA`, `.dist-info/entry_points.txt`, and `.data/scripts/` from the selected wheel. Commands come from `console_scripts` and `gui_scripts` entry points and from script files directly under `.data/scripts/`.

The snapshot boundary records the maximum PyPI Simple API `_last-serial` across selected project responses. The global project index is not fetched. A current release without a wheel is recorded as unsupported; acquisition does not fall back to an older release.

## Curation diagnostics

`just generate` and `just release` write a non-blocking acquisition diagnostics report and compare selected acquisitions with a baseline. Warnings go to standard error and never fail generation.

The default paths are `var/diagnostics/report.json` and `var/diagnostics/baseline.json`. When the baseline file does not exist, the first successful run writes the current acquisition state as the baseline. Later runs compare package and executable sets only for channels complete in both states. A file baseline is replaced after a successful run; a directory baseline is read without modification.

Diagnostics cover new or removed executables under ordinarily owned packages, changed executable sets, new or removed packages, projects without observed packages, and project metadata that disagrees with package hints. Package change warnings skip partial or failed snapshots. The report uses stable JSON with a `format` of `bnck-acquisition-diagnostics`. Repository redirect warnings are not part of generation because they require network requests.

## Further specifications

- [Debian Repository Format](https://wiki.debian.org/DebianRepository/Format)
- [DNF repository metadata](https://dnf.readthedocs.io/en/latest/repo_metadata.html)
- [Homebrew Formula API](https://formulae.brew.sh/docs/api/)
- [crates.io data access](https://crates.io/data-access)
- [npm `package.json` `bin`](https://docs.npmjs.com/cli/configuring-npm/package-json#bin)
- [PyPI Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/)
- [Python entry points specification](https://packaging.python.org/en/latest/specifications/entry-points/)
