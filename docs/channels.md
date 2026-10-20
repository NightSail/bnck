# Distribution channel acquisition

## Purpose

This document analyzes how bnck acquires package, project, and executable-name data from two channel families:

- Distros: Linux distributions such as Debian, Fedora, and Arch Linux, plus non-native channels such as Homebrew
- Language repositories: crates.io, npm, and PyPI

Homebrew behaves like a distro channel here: its packages install finished commands into a managed prefix.

The goal is not to reproduce each channel but to produce auditable candidate mappings and package observations for the [snapshot schema](acquisition-format.md):

```text
(channel, package name, observed package version, executable name, project hints, evidence)
```

Project attribution remains curation. A matching homepage, repository URL, source package, or package name is evidence, never automatic ownership.

## Shared acquisition model

### Define the observed channel scope

A channel name alone rarely identifies one package universe, so every acquisition declares its scope:

- Debian: Suite or snapshot, component, architecture, and binary versus source packages
- Fedora: Release, repository, architecture, and whether updates are included
- Arch Linux: Snapshot date, repository, and architecture
- Homebrew: Tap, formulae versus casks, platform, and architecture
- crates.io: Visible crate releases and the release-selection policy
- npm: Registry, visible package versions, and the release-selection policy
- PyPI: Index, visible distribution files, Python/platform compatibility policy, and the release-selection policy

Prefer official repositories, current supported releases, and primary architectures. Observations retain their scope even if dataset generation later collapses them into the channel-level provider relation defined in [design.md](design.md).

### Version channel data with a snapshot manifest

A mutable label such as `stable`, `latest`, `updates`, or a dist-tag is not a reproducible version. Each acquisition emits a channel snapshot manifest with:

- Channel and scope
- Channel-native boundary, such as a repository commit, archive date, change sequence, or signed repository root
- Retrieval time
- URI, media type, byte size, and cryptographic digest of every input object
- Signature identity and verification result where the channel signs metadata or artifacts
- Selected package versions and artifact digests
- Importer source revision and configuration
- Failed, missing, or deliberately excluded inputs

The dataset release refers to the digest of this manifest, which content-addresses every local input and the candidate artifact.

Current importers emit input path, size, and SHA-256 digest, selected package releases and variants, importer identity, scope, and completeness status. The just-based fetch layer does not yet record retrieval timestamps, original URIs, or upstream signature results; these are explicit collector gaps, not implied guarantees.

A channel-native boundary is necessary but not always sufficient. A PyPI serial or npm change sequence identifies a logical point in an update stream but cannot reconstruct it for an arbitrary later request; retain the static metadata responses actually used, or acquire them through a snapshot-capable mirror.

### Separate package identity from package release

The canonical package identity is `(channel, package name)`. Acquisition additionally records the observed release and variant:

```text
(package name, version, architecture, platform, repository scope, artifact digest)
```

Executable names can change between releases or variants, so provider relations derive from a selected current release while observations stay traceable to the exact artifact.

Release selection must be explicit. The default is the newest non-withdrawn stable release visible at the snapshot boundary, with distro repositories choosing the version selected by repository priority. Yanks, deprecations, unpublishes, and repository removals are recorded, not treated as ordinary absence.

### Define an executable consistently

For distro-like channels, an executable candidate is an end-user command whose path is reported in a public command directory, normally `bin` or `sbin` under the package-manager prefix. Directories, manual pages, libraries, desktop applications, shell completions, and paths under private `libexec` directories do not count. Repository metadata does not always expose mode or file type, so some path-based candidates need later verification.

For language repositories, declared command entry points count even when installed outside common system-level PATH destinations.

Each observation carries an evidence kind:

- Declared entry point: A crate target, npm `bin` key, or Python script entry point
- Repository file listing: A command path reported by distro repository metadata
- Static recipe action: A command inferred from a Homebrew formula or cask declaration
- Artifact metadata: A command declaration read from an archive without installing it

Acquisition uses only static data: repository indexes, registry manifests, recipe definitions, and, where no compact equivalent exists, archive metadata. Compact channel metadata is preferred over package payloads, which would multiply transfer per run. Disagreements and unknowns are preserved, not silently resolved.

### Extract project information as hints

Useful hints:

- Upstream source repository URLs
- Homepage and documentation URLs
- Source archive URLs
- Package summary and description
- License and issue-tracker URLs
- Distro source package, RPM source name, Arch package base, Homebrew tap, or language-repository package name

Normalize URLs for comparison while retaining original spelling: lowercase hostnames, remove a terminal `.git`, and recognize equivalent repository web and clone URLs. Never merge forks, mirrors, organization moves, monorepo members, or similarly named packages without curation.

One source package can build unrelated binary packages, one language package can expose commands from several projects, and one project can publish several packages, so per-executable curation remains necessary.

### Do not install, build, or execute packages

Package archives, manifests, formulae, build backends, maintainer scripts, and lifecycle scripts are untrusted input. Collectors parse static metadata and supported archive structures only: they do not evaluate formula code, invoke build backends, run maintainer or lifecycle scripts, or install packages.

When static data cannot determine an executable name, record the result as unknown. Installation-based verification may later become a separate, explicitly versioned acquisition mode.

### Retrieval anchors

Discovery anchors, not snapshot identifiers. Mirror URLs and API responses are mutable unless the channel analysis says otherwise.

| Channel | Primary discovery object |
| --- | --- |
| Debian | `https://deb.debian.org/debian/dists/<suite>/InRelease` and its referenced indexes |
| Fedora | `<repository-base-url>/repodata/repomd.xml`, with base URLs discovered through Fedora metalinks |
| Arch Linux | `https://archive.archlinux.org/repos/<year>/<month>/<day>/<repo>/os/<arch>/` |
| Homebrew | A tap Git checkout, `https://formulae.brew.sh/api/formula.json`, and `cask.json` |
| crates.io | The crates.io index and `https://static.crates.io/db-dump.tar.gz` |
| npm | `https://registry.npmjs.org/<encoded-package>/latest` for curated packages |
| PyPI | `https://pypi.org/simple/<normalized-project>/` for curated packages |

## Distro channels

### Debian

#### Relevant data and retrieval

Debian repositories expose a signed metadata tree below `dists/` and package artifacts below `pool/`. Acquisition:

1. Fetches `InRelease`, or `Release` plus `Release.gpg`, for a selected suite and verifies it against the Debian archive keyring
2. Fetches the `Packages` index for every selected component and architecture using the release-file checksums
3. Fetches `Sources` indexes for source-package and upstream metadata
4. Fetches `Contents-<architecture>` and `Contents-all` for a cheap package-to-path mapping that includes architecture-independent packages

`Packages` supplies binary package name, version, architecture, source package, description, homepage, dependencies, artifact path, and checksums. `Sources` supplies source-package metadata including `Homepage` and `Vcs-*` fields. `Contents` maps installed paths to binary packages and is an efficient first pass for command candidates.

Current acquisition reads the configured primary suite, stable by default, and unstable/sid. Their normalized records use the same `debian` package namespace in separate JSON Lines files, so unstable-only packages participate without a second canonical channel.

Prefer a timestamped [snapshot.debian.org](https://snapshot.debian.org/) view, or retain the verified release tree and every referenced index; `by-hash` URLs avoid mirror-update races but do not replace historical retention.

#### Channel-data version

Record:

- Snapshot timestamp or archive URI
- Suite or codename, component, and architecture set
- Digest and signing key of `InRelease`
- Digest of each `Packages`, `Sources`, and `Contents` object
- Version, architecture, filename, and SHA-256 digest of each observed package

A package version identifies one release inside a repository snapshot; the signed release root plus its indexes identifies the observed channel state.

#### Project extraction

Start from the binary package's `Source` field joined to the matching source stanza. Candidate hints come from `Homepage`, `Vcs-Git`, `Vcs-Browser`, source archive locations, and optional upstream metadata. The source package name groups packages but is not a canonical project identity: one source can produce multiple projects or bundle third-party tools.

Channel package identities are binary packages because users install them and file ownership is assigned to them. Architecture-specific observations are retained even when command sets are later unioned. Source package names are preserved as more stable grouping hints, since binary package names are not expected to be durable.

#### Executable extraction

Filter `Contents` entries to public command directories such as `/usr/bin`, `/usr/sbin`, `/bin`, and `/sbin`. Account for /usr-merge: treat `/usr/bin` and `/bin` as one source, likewise `/usr/sbin` and `/sbin`.

Inspecting `.deb` files is deferred until there is genuine need.

### Fedora

#### Relevant data and retrieval

A Fedora DNF repository is rooted at `repodata/repomd.xml`, which references compressed metadata objects with checksums. Fetch and verify:

- `primary` for package name, epoch-version-release, architecture, summary, URL, source RPM, and payload location/checksum
- `filelists` for installed paths
- `updateinfo` when withdrawal, replacement, and advisory context is needed

Resolve repository URLs from Fedora metalinks, but identify acquired objects by digest rather than the transient mirror that served them.

The Fedora universe spans release and updates repositories and may include modular or alternative repositories. Repository priority and duplicate NEVRA handling must match an explicitly documented DNF view.

#### Channel-data version

Record the Fedora release, repository ID, base architecture, `repomd.xml` digest and revision, checksums of every referenced metadata object, and each selected RPM's NEVRA, location, and repository-reported payload digest. The complete set of `repomd.xml` roots is the snapshot boundary; a mirror timestamp or release number alone is insufficient because updates change continuously.

#### Project extraction

Use RPM `URL`, summary, description, and `SOURCERPM` from `primary` as initial hints. The source RPM name is analogous to a Debian source package: useful for grouping, but it can represent a fork, a bundle, or several upstream projects.

Fedora's package-maintenance, dist-git, and build-system metadata can supplement provenance when pinned to the repository state. Downloading source RPMs to inspect spec files is deferred.

#### Executable extraction

Use `filelists` to find paths in `/usr/bin`, `/usr/sbin`, `/bin`, and `/sbin`, excluding entries marked as directories or ghost files. Account for /usr-merge aliases rather than emitting duplicate command names.

Filelists do not reliably establish executable mode or distinguish regular files from symlinks, so the public path is the current evidence. Commands created only by RPM scriptlets or alternatives frameworks remain an unobserved gap; no RPM transaction is run.

Package identity is the binary RPM name. Keep epoch, version, release, architecture, source RPM, and repository as observation fields.

### Arch Linux

#### Relevant data and retrieval

Official Arch mirrors publish repository databases such as `core.db` and file databases such as `core.files`. Acquisition:

1. Selects an official [Arch Linux Archive](https://archive.archlinux.org/) date rather than a moving mirror
2. Fetches each selected repository's `.db` and `.files` objects and their signatures where published
3. Reads name, version, package base, URL, filename, architecture, and checksums
4. Uses `.files` records to locate public command paths

Check out packaging repositories and `PKGBUILD` files at the commit matching the repository snapshot, not from the current default branch.

#### Channel-data version

Record the archive date, repository, architecture, database and file-database digests, signature results, and each selected package's full version, filename, and repository-reported digest. The archive date is a convenient channel-native boundary; content hashes protect the metadata. Arch's rolling release has no stable release number, so a mirror URL or version list cannot identify repository state.

#### Project extraction

The package URL, package base, `PKGBUILD` source URLs, and packaging repository are hints. The package base connects split packages built from one `PKGBUILD` but must not become the canonical project, because split packages can expose distinct upstream components.

#### Executable extraction

Filter `.files` entries to `usr/bin/` and other supported public command directories. The file database lacks enough mode and type information to classify every entry, so the public path is the current evidence. Package-archive inspection and installation are deferred, so commands created only by install scripts are not observed. Package identity is the binary package name, not the package base.

### Homebrew

#### Relevant data and retrieval

Homebrew has two package kinds:

- Formulae: Build recipes that normally install Unix commands and libraries
- Casks: macOS application installers that may optionally expose command-line binaries

The authoritative recipe history is the Git history of a tap such as `Homebrew/homebrew-core`. The [Formula API](https://formulae.brew.sh/docs/api/) is a generated view with names, tap, description, homepage, source URLs and checksums, versions, bottle variants, executable names, and often `tap_git_head`. Pin the tap commit, parse that checkout statically, and accept an API response only when all records agree on the intended commit.

For formulae, use the API and formula source as static inputs. Bottle metadata identifies supported platform and architecture variants; bottles are not downloaded to enumerate files. Formula files are Ruby programs and must be parsed conservatively without evaluation.

Casks are a separate scope. The Cask API and cask definitions describe artifacts such as `binary`, `app`, `pkg`, and `suite`; only declared or observed command-line artifacts belong in bnck.

#### Channel-data version

Record the fully qualified tap, tap Git commit, formula or cask name, resolved version and revision, source checksum, and repository-reported bottle or cask artifact digest per platform. If the API is used, retain and hash the response and record `tap_git_head`. A Homebrew interpreter version is needed only if formula evaluation is introduced later.

An upstream version is insufficient: Homebrew revisions and bottle rebuilds can change the installed artifact without changing the version, and platform bottles can differ.

#### Project extraction

Use the formula or cask homepage, stable source URL, head repository, description, and livecheck information as hints. Resource URLs may identify bundled dependencies and normally are not the formula's project. Tap and formula rename history establishes package continuity, not project identity.

Use the fully qualified `tap/name` as package identity internally; short names and aliases are lookup conveniences that can collide across taps.

#### Executable extraction

Use the Formula API's `executables` list when present. Otherwise, conservatively parse formula definitions for recognizable DSL actions such as `bin.install`, `sbin.install`, public symlink creation, and explicit destination renames; the installed basename is the command name. Do not infer paths from arbitrary Ruby, `system` build commands, or generic `make install`. Formulae with statically undeterminable commands remain unknown.

Bottle archive inspection and `brew install` are deferred, so platform-specific bottle differences remain metadata without additional executable observations.

For casks, a declared `binary` artifact supplies a command candidate, using its explicit target basename when present. Do not infer commands from `app`, `pkg`, `suite`, installer, or postflight declarations.

## Language repositories

The implemented [language registry workflow](language-registries.md) uses the full crates.io database dump to discover relevant packages. npm and PyPI fetch metadata only for explicitly curated packages: npm uses `latest` release manifests; PyPI uses per-project Simple API responses and selected wheels. npm/PyPI collection is a retained non-atomic scan, not an incremental point-in-time mirror. The analyses below also describe stronger snapshot boundaries and optional metadata that are not yet implemented.

### crates.io

#### Relevant data and retrieval

crates.io provides two complementary static sources:

- The crates.io index: Release records, dependency metadata, yank state, and SHA-256 checksums for each `.crate`
- The crates.io database dump: Bulk crate, release, owner, project, and target metadata without an API crawl

For a full acquisition, use the [database dump](https://crates.io/data-access) and a checkout of the [crates.io index](https://doc.rust-lang.org/cargo/reference/registry-index.html); avoid crawling the per-crate HTTP API. Dump release records include binary target names, so `.crate` files need not be downloaded for executable discovery.

The package identity is the crate name assigned by crates.io; each semantic version is a release of that package, not a separate package.

#### Channel-data version

Use the crates.io index Git commit as the primary visibility and yank-state boundary. Record the dump timestamp, digest, and internal metadata as a supplemental boundary, and its relationship to the observed index state. For each selected release, record version, yank state, index checksum, and dump-reported crate size.

The index commit is preferable to "latest at retrieval time" because yank flags change and versions are appended continuously. The artifact checksum remains useful release identity even though the artifact is not downloaded.

#### Project extraction

Read `repository`, `homepage`, `documentation`, `description`, `license`, and `readme` from the dump. Repository and homepage URLs are useful, but a repository match is ambiguous for monorepos.

Do not assume a crate is the canonical project of the same name: sys crates, bindings, forks, command wrappers, and organization-owned namespace crates routinely break that assumption.

#### Executable extraction

Read `bin_names` from each selected release. Each value is a binary target name extracted by crates.io from the published package, hence a declared command candidate. `has_lib` distinguishes library-only and mixed packages but does not affect extraction.

The dump does not preserve all target conditions, such as `required-features`, per binary. Retain release-level `features`, `rust_version`, and edition metadata, but treat installability under a feature set or target as unknown. Do not run Cargo or inspect `.crate` files.

### npm

#### Relevant data and retrieval

Request `https://registry.npmjs.org/<encoded-package>/latest` for each explicit npm package name in project curations and package overrides. The endpoint resolves the current `latest` dist-tag and returns that release's manifest with its version, project fields, and `bin` declarations. Scoped names are URL-encoded without changing identity.

A refresh costs one small response per curated package, excluding retries. Do not enumerate `_all_docs` or fetch full packuments: neither the complete name catalog nor historical versions are needed. No package archives are downloaded.

Package identity is the exact registry name, including npm scope. Dist-tags are mutable aliases, never package versions.

#### Channel-data version

Retain the package plan, release response bodies, and HTTP status sidecars. The acquisition manifest records their content identities, the registry origin, and each observed release version. This is a bounded non-atomic scan, not a replication checkpoint.

Fresh snapshots re-request each package because dist-tags and metadata change; resuming downloads only unfinished responses. HTTP 404/410 means the requested latest release is unavailable, not that the package was unpublished.

#### Project extraction

Use `repository`, `homepage`, and `description` from the selected release manifest. Ownership comes from explicit curations, not matching fields. Do not merge packages that share a repository: JavaScript monorepos frequently publish independently meaningful projects.

#### Executable extraction

Read the normalized `bin` mapping from the selected release manifest: object keys are command names and values are package-relative script paths. A string-valued `bin` denotes the default command name, the unscoped basename. Legacy `directories.bin` without a resolved `bin` mapping is unsupported and recorded as such rather than inferred from a directory name.

The normalized `bin` key is the command because npm-compatible installers create links or wrappers from it. Without the tarball, acquisition cannot confirm that the target exists, is a regular file, or has a valid shebang; preserve that limitation rather than rejecting the declaration.

Lifecycle scripts can download or generate commands but are nondeterministic and outside static acquisition. Do not infer commands from `scripts` or dependencies. Platform constraints remain in the retained manifest but are not evaluated.

### PyPI

#### Relevant data and retrieval

Fetch `https://pypi.org/simple/<normalized-project>/` for each explicit PyPI package name in project curations and package overrides, using the PEP 691 JSON media type of the [Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/). Each response lists distribution files, hashes, upload times, yanked state, Python requirements, and optional core-metadata hashes.

Do not fetch the root project list or infer names from project names and repository URLs. Select each package's newest non-yanked stable release, then prefer a platform-independent wheel and smaller declared size. Cache selected wheels by SHA-256 and inspect them statically; a current release without a wheel is unsupported, not a reason to fall back to an older release.

Package identity is the normalized distribution name for lookup; retain the original core-metadata `Name` for display and provenance.

#### Channel-data version

Retain the explicit package plan, per-project responses, HTTP status sidecars, wheel plan, and selected wheels; their content identities define the acquisition inputs. The responses are a bounded non-atomic scan, not a journal checkpoint.

PyPI file hashes identify artifacts, but releases can be added, removed, or yanked. Fresh snapshots re-request curated project responses; existing wheels are reused when their digests match selected artifacts.

#### Project extraction

Read core metadata from the PEP 658 metadata sidecar when available. Otherwise use static `.dist-info/METADATA` from a wheel or `PKG-INFO` from a source distribution only when that artifact is already needed. Useful fields include `Name`, `Version`, `Summary`, `Home-page`, `Project-URL`, `Author`, `License`, and classifiers; repository, source, issue-tracker, and documentation URLs usually appear as labeled `Project-URL` values.

Metadata can differ between files for the same release, especially across old or platform-specific uploads. Preserve those differences and prefer metadata attached to the artifact that supplied the executable evidence.

#### Executable extraction

PyPI's index and core-metadata sidecars do not expose entry-point groups, so wheel metadata is the one case where executable discovery requires package artifacts. Read `.dist-info/entry_points.txt` per the [entry points specification](https://packaging.python.org/en/latest/specifications/entry-points/) and collect keys in the `console_scripts` and `gui_scripts` groups as logical command names. Also inspect the wheel's `.data/scripts/` area for scripts installers copy directly, preserving the installed basename. This is static ZIP inspection; the wheel is never installed or imported.

Inspect selected wheel variants because entry points can differ by platform or build. To control transfer, fetch a platform-independent wheel first when available, reuse wheels by SHA-256, and inspect further variants only when policy requires it. Do not add `.exe`, `-script.py`, or other installer-specific wrappers to logical command names unless bnck later models physical platform filenames.

A source distribution may declare scripts statically in `pyproject.toml` or `setup.cfg`; such declarations are candidate evidence only. A `setup.py` can compute metadata dynamically and must not be executed. If no wheel or supported static declaration exists, mark executable extraction as unknown; absence of a recognizable declaration is not evidence that the project installs no commands.

## Comparison and recommended order

| Channel | Best snapshot boundary | Best project source | Best executable source | Main complication |
| --- | --- | --- | --- | --- |
| Debian | Signed release root at a snapshot timestamp | Source stanza and source package | `Contents` | Alternatives and missing file types |
| Fedora | Set of `repomd.xml` digests | Primary metadata and source RPM name | `filelists` | Multiple repos and scriptlets |
| Arch Linux | Archive date plus repository digests | Package URL, package base, and `PKGBUILD` | `.files` | Rolling state and missing file types |
| Homebrew | Tap commit plus API response digests | Formula/cask metadata | API executable lists and static declarations | Arbitrary Ruby and implicit install paths |
| crates.io | Index commit plus database-dump digest | Database-dump project fields | Database-dump `bin_names` | Missing per-target conditions |
| npm | Package plan plus release response digests | Curated ownership and release manifest | Normalized `bin` mapping | Curated packages only; lifecycle scripts |
| PyPI | Package and wheel plans plus retained input digests | Selected wheel core metadata | Static wheel entry points and scripts | Curated packages only; source-only releases and wheel variants |

Implementation order, chosen to exercise the shared manifest, cache, URL normalization, and evidence model while minimizing artifact downloads:

1. crates.io, whose dump carries binary target names
2. npm, whose curated `latest` manifests declare `bin`
3. PyPI wheels, deferring source-only projects without supported static declarations
4. Debian, Fedora, and Arch Linux from repository file indexes
5. Homebrew from conservatively recognized formula and cask declarations

## Operational requirements

### Incremental updates

Keep the previous channel snapshot and process only new repository roots, index commits, replication changes, or journal serials. Reuse artifacts by digest. Generate a fresh immutable snapshot manifest even when most objects are shared.

The schema supports explicit `removed` and `failed` package statuses, and imports replace the preceding snapshot. Planned npm/PyPI imports record unavailable metadata and unsupported extraction as failures; an unavailable latest release does not prove unpublishing. Incremental tombstone production is not implemented. Published releases retain their manifests and are never mutated.

### Validation

Every importer reports at least:

- Metadata or artifact digest mismatches
- Invalid signatures
- Referenced artifacts that cannot be fetched
- Malformed paths, archive traversal attempts, and duplicate archive members
- Declared commands with malformed or unsafe target paths
- Conflicts between static metadata sources
- Unsupported package variants or dynamic build metadata

Failures belong in snapshot status and metrics. Treating a failed package as a package with no executables creates false negative provider records.

### Storage and licensing

Retain metadata and small manifests directly when redistribution terms permit. Acquisition avoids package payloads except where static metadata is unavailable, notably selected PyPI wheels. Retain artifacts in a content-addressed cache reused across runs; a release stays reproducible only while its inputs remain in that cache or an immutable upstream archive.

Respect channel rate limits, mirroring policies, signatures, and redistribution terms. Prefer bulk feeds and official mirrors over parallel API crawling.

### Curation boundary

Importers never decide that packages from different channels are the same project solely from names or URLs. They emit ranked, explainable hints and retain the exact supporting fields. Curation accepts or rejects a project attribution, or assigns individual executables when a package is mixed.

## Further specifications

- Debian repository format: [Debian Repository Format](https://wiki.debian.org/DebianRepository/Format)
- Fedora repository metadata: [DNF repository metadata](https://dnf.readthedocs.io/en/latest/repo_metadata.html)
- Arch package archives: [Creating packages](https://wiki.archlinux.org/title/Creating_packages)
- Homebrew bottles: [Bottles](https://docs.brew.sh/Bottles)
- Cargo registry index: [Registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- npm command declarations: [`package.json` `bin`](https://docs.npmjs.com/cli/configuring-npm/package-json#bin)
- Python core metadata: [Core metadata specifications](https://packaging.python.org/en/latest/specifications/core-metadata/)
