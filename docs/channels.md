# Distribution channel acquisition

## Purpose

This document analyzes how bnck can acquire package, project, and executable-name data from two channel families:

- Distros: Linux distros, like Debian, Fedora, Arch Linux; non-native channels like Homebrew
- Language repositories: crates.io, npm, and PyPI

Homebrew is a package manager rather than an operating-system distribution, but it behaves like a distro channel for this analysis: its packages install finished commands into a managed prefix.

The goal is not to reproduce each channel. It is to produce auditable candidate mappings and package observations using the schema in [acquisition-format.md](acquisition-format.md). Candidate mappings have the form:

```text
(channel, package name, observed package version, executable name, project hints, evidence)
```

Project attribution remains curation. A matching homepage, repository URL, source package, or package name is useful evidence, but is not sufficient to merge packages into a canonical project automatically.

## Shared acquisition model

### Define the observed channel scope

A channel name alone rarely identifies one package universe. Every acquisition must declare its scope:

- Debian: Suite or snapshot, component, architecture, and binary versus source packages
- Fedora: Release, repository, architecture, and whether updates are included
- Arch Linux: Snapshot date, repository, and architecture
- Homebrew: Tap, formulae versus casks, platform, and architecture
- crates.io: Visible crate releases and the release-selection policy
- npm: Registry, visible package versions, and the release-selection policy
- PyPI: Index, visible distribution files, Python/platform compatibility policy, and the release-selection policy

The initial dataset should prefer official repositories, current supported releases, and primary architectures. Expanding scope later is safer than silently combining every historical release and architecture into one observation. Internally, observations should retain their scope even if dataset generation later collapses them into the channel-level provider relation defined in [design.md](design.md).

### Version channel data with a snapshot manifest

A mutable label such as `stable`, `latest`, `updates`, or a dist-tag is not a reproducible version. Each acquisition should emit a channel snapshot manifest containing:

- Channel and scope
- Channel-native boundary, such as a repository commit, archive date, change sequence, or signed repository root
- Retrieval time
- URI, media type, byte size, and cryptographic digest of every input object
- Signature identity and verification result where the channel signs metadata or artifacts
- Selected package versions and artifact digests
- Importer source revision and configuration
- Any failed, missing, or deliberately excluded inputs

The dataset release refers to the digest of this manifest. The manifest, in turn, content-addresses every local input and the candidate artifact. This gives each dataset release an exact meaning while those inputs remain retained.

Current importers emit the local input path, byte size, and SHA-256 digest, selected package releases and variants available from the parsed metadata, importer identity, scope, and completeness status. The just-based fetch layer does not yet record retrieval timestamps, original URI values in the manifest, or upstream signature-verification results. Those remain explicit collector gaps rather than implied guarantees.

A channel-native boundary is necessary but not always sufficient. For example, a PyPI serial or npm change sequence identifies a logical point in an update stream, but it does not guarantee that an arbitrary later HTTP request can reconstruct that point. The collector must retain the static metadata responses it actually used, or acquire them through a snapshot-capable mirror.

### Separate package identity from package release

The canonical package identity remains `(channel, package name)`. Acquisition should additionally record the observed release and variant:

```text
(package name, version, architecture, platform, repository scope, artifact digest)
```

This distinction matters because executable names can change between releases or variants. A provider relation can be generated from a selected current release, while the observation remains traceable to the exact artifact that supported it.

Release selection must be explicit. A reasonable default is the newest non-withdrawn stable release visible at the snapshot boundary, with distro repositories allowed to choose the version selected by repository priority. Yanks, deprecations, unpublishes, and repository removals must be recorded rather than treated as ordinary absence.

### Define an executable consistently

For distro-like channels, an executable candidate is an end-user command whose path is reported in a public command directory, normally `bin` or `sbin` under the package-manager prefix. Directories, manual pages, libraries, desktop applications, shell completions, and paths under private `libexec` directories do not count. Repository metadata does not always expose mode or file type, so some path-based candidates will need later verification.

For language repositories, declared command entry points count even when the package manager installs them at a location outside common system-level PATH destinations.

Each executable observation should carry an evidence kind:

- Declared entry point: A crate target, npm `bin` key, or Python script entry point
- Repository file listing: A command path reported by distro repository metadata
- Static recipe action: A command inferred from a Homebrew formula or cask declaration
- Artifact metadata: A command declaration read from an archive without installing it

Current acquisition uses only static data: repository indexes, registry manifests, recipe definitions, and, where no compact equivalent exists, archive metadata. It does not install or build packages. Compact channel metadata should be preferred over downloading package payloads because payload inspection would substantially increase data transfer per run.

Acquisition should preserve disagreements and unknowns instead of choosing silently.

### Extract project information as hints

Useful project hints include:

- Upstream source repository URLs
- Homepage and documentation URLs
- Source archive URLs
- Package summary and description
- License and issue-tracker URLs
- Distro source package, RPM source name, Arch package base, Homebrew tap, or language-repository package name

URLs should be normalized for comparison while retaining their original spelling. Safe normalization includes lowercasing hostnames, removing a terminal `.git`, and recognizing equivalent repository web and clone URLs. It should not merge forks, mirrors, organization moves, monorepo members, or similarly named packages without curation.

One source package can build unrelated binary packages, one language package can expose commands from more than one project, and a single project can publish several packages. Per-executable curation therefore remains necessary.

### Do not install, build, or execute packages

Package archives, manifests, formulae, build backends, maintainer scripts, and lifecycle scripts are untrusted input. Current collectors only parse static metadata and supported archive structures. They do not evaluate formula code, invoke build backends, run maintainer or lifecycle scripts, or install packages.

When static data cannot determine an executable name, acquisition should record the result as unknown. Installation-based verification may be added as a separate, explicitly versioned acquisition mode later, but is outside the current scope.

### Retrieval anchors

These are discovery anchors, not snapshot identifiers. Mirror URLs and API responses remain mutable unless the channel-specific analysis says otherwise.

| Channel | Primary discovery object |
| --- | --- |
| Debian | `https://deb.debian.org/debian/dists/<suite>/InRelease` and its referenced indexes |
| Fedora | `<repository-base-url>/repodata/repomd.xml`, with base URLs discovered through Fedora metalinks |
| Arch Linux | `https://archive.archlinux.org/repos/<year>/<month>/<day>/<repo>/os/<arch>/` |
| Homebrew | A tap Git checkout, `https://formulae.brew.sh/api/formula.json`, and `cask.json` |
| crates.io | The crates.io index and `https://static.crates.io/db-dump.tar.gz` |
| npm | `https://replicate.npmjs.com/_changes` and `https://registry.npmjs.org/<package>` |
| PyPI | `https://pypi.org/simple/` and `https://pypi.org/simple/<normalized-project>/` |

## Distro channels

### Debian

#### Relevant data and retrieval

Debian repositories expose a signed metadata tree below `dists/` and package artifacts below `pool/`. Acquisition should:

1. Fetch `InRelease`, or `Release` plus `Release.gpg`, for a selected suite and verify it against the Debian archive keyring
2. Fetch the `Packages` index for every selected component and architecture using the checksums in the release file
3. Fetch `Sources` indexes for source-package and upstream metadata
4. Fetch both `Contents-<architecture>` and `Contents-all` indexes for a cheap package-to-path mapping that includes architecture-independent packages

`Packages` supplies binary package name, version, architecture, source package, description, homepage, dependencies, artifact path, and checksums. `Sources` supplies source package metadata, including `Homepage` and `Vcs-*` fields when maintainers provide them. `Contents` maps installed paths to binary packages and is an efficient first pass for command candidates.

Current acquisition reads both the configured primary suite, stable by default, and unstable/sid. Their normalized records remain in separate JSON Lines files but use the same `debian` package namespace, allowing unstable-only packages to participate without creating a second canonical distribution channel.

Repository labels such as `stable` move over time. For reproducibility, prefer a timestamped [snapshot.debian.org](https://snapshot.debian.org/) view, or retain the verified release tree and every referenced index. `by-hash` URLs avoid mirror-update races but do not replace historical retention.

#### Channel-data version

The snapshot manifest should record:

- Snapshot timestamp or archive URI
- Suite or codename, component, and architecture set
- Digest and signing key of `InRelease`
- Digest of each `Packages`, `Sources`, and `Contents` object
- Version, architecture, filename, and SHA-256 digest of each observed package

A Debian package version is not the channel-data version. It identifies one package release inside a repository snapshot; the signed release root plus its indexes identifies the observed channel state.

#### Project extraction

Start with the binary package's `Source` field and join it to the matching source stanza. Candidate project information comes from `Homepage`, `Vcs-Git`, `Vcs-Browser`, source archive locations, and optional upstream metadata in the Debian source package. The Debian source package name is a strong grouping hint but not a canonical project identity: one source can produce multiple projects or bundle third-party tools.

Binary packages, not source packages, are the channel package identities because users install binary packages and file ownership is assigned to them. Architecture-specific observations should be retained even when their command sets are later unioned.

Still, source package names are preserved and referenced, as a way to establish more stable identities, as binary package names are not expected to be durable.

#### Executable extraction

Filter `Contents` entries to public command directories such as `/usr/bin`, `/usr/sbin`, `/bin`, and `/sbin`. Account for /usr-merge; treat `/usr/bin` and `/bin` as the same source, likewise for `/usr/sbin` and `/sbin`.

Inspecting `.deb`s is deferred until there is genuine need.

### Fedora

#### Relevant data and retrieval

A Fedora DNF repository is rooted at `repodata/repomd.xml`. The root references compressed metadata objects and includes their checksums. Acquisition should fetch and verify:

- `primary` metadata for package name, epoch-version-release, architecture, summary, URL, source RPM, and payload location/checksum
- `filelists` metadata for installed paths
- `updateinfo` when withdrawal, replacement, and advisory context is needed

Repository URLs should be resolved from Fedora's metalink configuration, but the acquired object should be identified by digest rather than by the transient mirror that served it.

The Fedora package universe is split across release and updates repositories, and may include modular or alternative repositories. Repository priority and duplicate NEVRA handling must match an explicitly documented DNF view.

#### Channel-data version

Record the Fedora release, repository ID, base architecture, `repomd.xml` digest and revision, checksums of every referenced metadata object, and each selected RPM's NEVRA, location, and repository-reported payload digest. A complete set of `repomd.xml` roots is the boundary for the snapshot.

A mirror timestamp or Fedora release number alone is insufficient because the updates repository changes continuously.

#### Project extraction

Use RPM `URL`, summary, description, and `SOURCERPM` fields from `primary` metadata as initial hints. The source RPM name is analogous to a Debian source package: it is useful for grouping but can represent a fork, a bundle, or several upstream projects.

Fedora's static package-maintenance, dist-git, and build-system metadata can supplement provenance when it can be pinned to the repository state. Downloading source RPMs merely to inspect their spec files is deferred.

#### Executable extraction

Use `filelists` to find paths in `/usr/bin`, `/usr/sbin`, `/bin`, and `/sbin`. Exclude entries explicitly marked as directories or ghost files. As with Debian, account for /usr-merge aliases rather than emitting duplicate command names.

Filelists do not reliably establish executable mode or distinguish every regular file from a symlink. The public path is therefore the executable evidence for now. Commands created only by RPM scriptlets or alternatives frameworks are not represented and remain a known gap; no RPM transaction is run to discover them.

The package identity is the binary RPM name. Keep epoch, version, release, architecture, source RPM, and repository as observation fields.

### Arch Linux

#### Relevant data and retrieval

Official Arch mirrors publish repository databases such as `core.db` and richer file databases such as `core.files`. Acquisition should:

1. Select an official [Arch Linux Archive](https://archive.archlinux.org/) date rather than a moving mirror
2. Fetch each selected repository's `.db` and `.files` objects and their signatures where published
3. Read package metadata, including name, version, package base, URL, filename, architecture, and checksums
4. Use the `.files` records to locate public command paths

The packaging repositories and `PKGBUILD` files are valuable for source URLs and split-package relationships, but they should be checked out at a commit corresponding to the repository snapshot rather than read from the current default branch.

#### Channel-data version

Record the archive date, repository, architecture, database and file-database digests, signature results, and every selected package's full version, filename, and repository-reported digest. The archive date is a convenient channel-native boundary; content hashes protect the repository metadata from damage or unexpected changes.

Arch's rolling release has no stable release number that can identify repository state. A current mirror URL or package version list is not enough.

#### Project extraction

The package URL, package base, source URLs from `PKGBUILD`, and packaging repository are project hints. The package base connects split packages built from one `PKGBUILD`, but it must not automatically become the canonical project because split packages can expose distinct upstream components.

#### Executable extraction

Filter `.files` entries to `usr/bin/` and other supported public command directories. The file database does not provide enough mode and type information to classify every entry definitively, so the public path itself is the current evidence.

Package-archive inspection and installation are deferred. Commands created only by install scripts are therefore not observed. The binary package name, not package base, is the channel package identity.

### Homebrew

#### Relevant data and retrieval

Homebrew has two relevant package kinds:

- Formulae: Build recipes that normally install Unix commands and libraries
- Casks: macOS application installers that may optionally expose command-line binaries

The authoritative recipe history is the Git history of a tap such as `Homebrew/homebrew-core`. The [Formula API](https://formulae.brew.sh/docs/api/) is a convenient generated view containing names, tap, description, homepage, source URLs and checksums, versions, bottle variants, executable names, and often `tap_git_head`. Acquisition should pin the tap commit first, parse that checkout statically, and accept an API response only when all records agree on the intended tap commit.

For formulae, use the API and formula source as static inputs. Bottle metadata identifies supported platform and architecture variants, but bottles are not downloaded merely to enumerate their files. Formula files are Ruby programs and must be parsed conservatively without evaluating them.

Casks should be a separate scope. The Cask API and cask definitions describe artifacts such as `binary`, `app`, `pkg`, and `suite`. Only a declared or observed command-line artifact belongs in bnck; an application bundle by itself does not.

#### Channel-data version

Record the fully qualified tap, tap Git commit, formula or cask name, resolved version and revision, source checksum, and repository-reported bottle or cask artifact digest for each platform. If the API is used, retain and hash the API response and record `tap_git_head`. A Homebrew interpreter version is needed only if formula evaluation is introduced later.

A formula's upstream version is not sufficient. Homebrew revisions and bottle rebuilds can change the installed artifact without changing that version, and platform bottles can differ.

#### Project extraction

Use the formula or cask homepage, stable source URL, head repository, description, and livecheck information as hints. Resource URLs may identify bundled dependencies and should not normally be interpreted as the formula's project. A tap and formula rename history can establish package continuity but does not establish project identity.

Use the fully qualified `tap/name` as package identity internally. Short names and aliases are lookup conveniences and can collide across taps.

#### Executable extraction

Use the Formula API's `executables` list when present. Otherwise, conservatively parse formula definitions for recognizable DSL actions such as `bin.install`, `sbin.install`, public symlink creation, and explicit destination renames. The installed basename is the command name. Do not infer paths from arbitrary Ruby, `system` build commands, or generic `make install` calls. Formulae whose commands cannot be determined statically remain unknown.

Bottle archive inspection and `brew install` are deferred. Consequently, platform-specific bottle differences are retained as metadata but do not produce additional executable observations.

For casks, a declared `binary` artifact directly supplies a command candidate; use its explicit target basename when one is present. Do not infer commands from `app`, `pkg`, `suite`, installer, or postflight declarations. Those artifacts may create commands at installation time, but installation is outside the current acquisition mode.

## Language repositories

### crates.io

#### Relevant data and retrieval

crates.io provides two complementary static sources:

- The crates.io index: Release records, dependency metadata, yank state, and SHA-256 checksum for each `.crate` artifact
- The crates.io database dump: Bulk crate, release, owner, project, and target metadata without requiring an API crawl

For a full acquisition, use the [database dump](https://crates.io/data-access) and a checkout of the [crates.io index](https://doc.rust-lang.org/cargo/reference/registry-index.html). Avoid crawling the per-crate HTTP API. The dump's release records include binary target names, so `.crate` files do not need to be downloaded for executable discovery.

The package identity is the crate name as assigned by crates.io. Each semantic version is a release of that package, not a separate package.

#### Channel-data version

Use the crates.io index Git commit as the primary visibility and yank-state boundary. Record the database-dump timestamp, digest, and internal metadata as a supplemental boundary, and retain its relationship to the index state observed during collection. For every selected crate release, record version, yank state, index checksum, and the crate size reported by the dump.

The index commit is preferable to “latest at retrieval time” because yank flags can change and new versions are appended continuously. The artifact checksum remains useful release identity even though the artifact is not downloaded.

#### Project extraction

Read `repository`, `homepage`, `documentation`, `description`, `license`, and `readme` fields from the database dump. Repository and homepage URLs are especially useful, but a repository match is ambiguous for monorepos.

Do not assume a crate is the canonical project of the same name. Sys crates, bindings, forks, command wrappers, and organization-owned namespace crates routinely violate that assumption.

#### Executable extraction

Read `bin_names` from each selected release in the crates.io database dump. Each value is a binary target name extracted by crates.io from the published package and is therefore a declared command candidate. `has_lib` can distinguish library-only and mixed packages but does not affect command extraction.

The dump does not preserve all target conditions, such as `required-features`, with each binary name. Retain release-level `features`, `rust_version`, and edition metadata where available, but treat installability under a particular feature set or target as unknown. Do not run Cargo or download and inspect `.crate` files in the current acquisition mode.

### npm

#### Relevant data and retrieval

The npm registry exposes CouchDB-style package documents, commonly called packuments. A packument contains package-level metadata, dist-tags, time data, and a manifest for each visible version. Each version's `dist` object identifies its tarball and integrity digest.

For bulk acquisition, consume the registry replication/change feed rather than attempting to discover names through search. Process changes in sequence, fetch the corresponding document revision, and retain it locally. For focused refreshes, `https://registry.npmjs.org/<package>` is sufficient. The replication service is operational infrastructure rather than a promise that old revisions will remain fetchable forever, so responses must be captured as they are processed.

Each version manifest embedded in the packument includes project fields, command declarations, platform constraints, and a `dist` object. Record `dist.integrity`, or `dist.shasum` when a stronger Subresource Integrity value is unavailable, without downloading the tarball.

The package identity is the exact registry name, including npm scope. Dist-tags are mutable aliases and are never package versions.

#### Channel-data version

Record the registry origin, opaque replication `last_seq`, package document ID and `_rev`, captured packument digest, selected semantic version, tarball URL, and `dist.integrity`. A snapshot-capable registry replica should retain all document revisions processed through the chosen sequence.

A sequence alone is not reproducible if old revisions have been compacted. Retained, content-addressed packuments make the metadata boundary durable; the upstream-reported tarball integrity value identifies the release artifact without requiring it to be stored. Unpublished versions, deprecated versions, changed dist-tags, and replaced document revisions should remain represented in acquisition status.

#### Project extraction

Use `repository`, `homepage`, `bugs`, `description`, license, funding, and author/maintainer fields from the version-specific manifest embedded in the packument. Prefer version-specific metadata over the packument's package-level summary because project URLs can change between releases.

Repository fields can include a `directory` for monorepos. Preserve it. Do not merge packages merely because they share a repository: JavaScript monorepos frequently publish many independently meaningful projects.

#### Executable extraction

Read the normalized `bin` mapping from the selected release manifest embedded in the packument. Object keys are command names and values are package-relative script paths. A string-valued `bin` denotes the package's default command name according to npm's package-manifest normalization. Legacy `directories.bin` declarations should be expanded using npm-compatible normalization rather than ad hoc rules.

Treat the normalized `bin` key as the command because npm-compatible installers create links or platform wrappers from that declaration. Without downloading the tarball, acquisition cannot confirm that the target exists, is a regular file, or has a valid shebang; preserve that limitation rather than rejecting the declaration.

Lifecycle scripts can download or generate additional commands, but they are nondeterministic and outside static acquisition. Do not infer commands from `scripts` or dependencies. Platform constraints in `os`, `cpu`, `libc`, and `engines` should be retained with the observation.

### PyPI

#### Relevant data and retrieval

PyPI's standards-based discovery interface is the [Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/):

- The root project list from PEP 503 HTML or PEP 691 JSON
- A per-project response listing distribution files, hashes, upload times, yanked state, Python requirements, and optional core-metadata hashes
- Wheel and source-distribution artifacts

Use the PEP 691 JSON media type when available. Prefer PEP 658 core-metadata sidecars for project information so full distributions do not need to be downloaded. The PyPI JSON project API can supplement descriptions and project URLs, but it is not a complete, snapshot-consistent bulk interface.

For large-scale or incremental collection, use a PEP 381-compatible mirror implementation such as bandersnatch. PyPI exposes `X-PyPI-Last-Serial` and per-response `_last-serial` values so a mirror can determine whether its local project state is current.

The package identity is the distribution project's normalized name for lookup, while the original core-metadata `Name` should be retained for display and provenance.

#### Channel-data version

Use a PyPI journal serial as the logical snapshot boundary. Retain the root and project responses, their reported serials, and their content digests, plus every selected file's filename, upload time, yanked state, and SHA-256 digest.

A crawler that starts at one serial and finishes at another does not have a coherent snapshot merely because it records the final header. It must process journal changes through a fixed serial with a mirror, or retain enough per-project serial information to describe the acquisition as a bounded but non-atomic scan. PyPI file hashes identify artifacts, but removed files and changed metadata make the serial and captured responses necessary.

#### Project extraction

Read core metadata from the PEP 658 metadata sidecar when available. Otherwise, use static `.dist-info/METADATA` from a wheel or `PKG-INFO` from a source distribution only when that artifact is already needed. Useful fields include `Name`, `Version`, `Summary`, `Home-page`, `Project-URL`, `Author`, `License`, and classifiers. Repository, source, issue-tracker, and documentation URLs are generally encoded as labeled `Project-URL` values.

Metadata can differ between files for the same release, especially across old or platform-specific uploads. Preserve those differences and prefer metadata attached to the artifact that supplied the executable evidence.

#### Executable extraction

PyPI's index and core-metadata sidecars do not expose entry-point groups, so wheel metadata is the one case where executable discovery requires selected package artifacts. Read `.dist-info/entry_points.txt` according to the [entry points specification](https://packaging.python.org/en/latest/specifications/entry-points/) and collect keys in the `console_scripts` and `gui_scripts` groups as logical command names. Also inspect the wheel's `.data/scripts/` area for scripts that installers copy directly, preserving the installed basename. This is static ZIP inspection; the wheel is never installed or imported.

Inspect selected wheel variants because entry points can differ by platform or build. To control data transfer, fetch only one platform-independent wheel first when available, reuse wheels by SHA-256 digest, and inspect additional variants only when policy requires them. Generated Windows launchers should not add `.exe`, `-script.py`, or other installer-specific wrappers to the logical command name unless bnck later models physical platform filenames separately.

A source distribution may declare scripts statically in `pyproject.toml` or `setup.cfg`. A `setup.py` can compute metadata dynamically and must not be executed. Static declarations are candidate evidence only. If no wheel or supported static declaration exists, mark executable extraction as unknown; absence of a recognizable declaration is not evidence that the project installs no commands.

## Comparison and recommended order

| Channel | Best snapshot boundary | Best project source | Best executable source | Main complication |
| --- | --- | --- | --- | --- |
| Debian | Signed release root at a snapshot timestamp | Source stanza and source package | `Contents` | Alternatives and missing file types |
| Fedora | Set of `repomd.xml` digests | Primary metadata and source RPM name | `filelists` | Multiple repos and scriptlets |
| Arch Linux | Archive date plus repository digests | Package URL, package base, and `PKGBUILD` | `.files` | Rolling state and missing file types |
| Homebrew | Tap commit plus API response digests | Formula/cask metadata | API executable lists and static declarations | Arbitrary Ruby and implicit install paths |
| crates.io | Index commit plus database-dump digest | Database-dump project fields | Database-dump `bin_names` | Missing per-target conditions |
| npm | Replication sequence plus captured revisions | Version manifest in packument | Normalized `bin` mapping | Unpublishes and lifecycle scripts |
| PyPI | Journal serial plus captured project pages | Core metadata sidecars | Static wheel entry points and scripts | Dynamic source metadata and wheel variants |

A practical implementation order is:

1. crates.io, because binary target names are present in the database dump
2. npm, because `bin` is explicit once reliable bulk replication is in place
3. PyPI wheels, while deferring source-only projects that lack supported static declarations
4. Debian, Fedora, and Arch Linux using repository file indexes only
5. Homebrew using conservatively recognized formula and cask declarations

This order exercises the common manifest, metadata cache, URL normalization, and evidence model while minimizing package-artifact downloads. Installation-based acquisition is not part of this plan.

## Operational requirements

### Incremental updates

Keep the previous channel snapshot and process only new repository roots, index commits, replication changes, or journal serials. Reuse artifacts by digest. Generate a fresh immutable snapshot manifest even when most objects are shared with the previous acquisition.

The acquisition schema supports explicit `removed` and `failed` package statuses. Current full-snapshot importers emit observed packages and replace the preceding snapshot; incremental tombstone production is not implemented. Published releases retain their snapshot manifests and are not mutated.

### Validation

Every importer should report, at minimum:

- Metadata or artifact digest mismatches
- Invalid signatures
- Referenced artifacts that cannot be fetched
- Malformed paths, archive traversal attempts, and duplicate archive members
- Declared commands with malformed or unsafe target paths
- Conflicts between static metadata sources
- Unsupported package variants or dynamic build metadata

Failures should be visible in snapshot status and metrics. Silently treating a failed package as a package with no executables would create false negative provider records.

### Storage and licensing

Metadata and small manifests should be retained directly when redistribution terms permit. Current acquisition avoids package payloads except where static metadata is unavailable, notably selected PyPI wheels. Retained artifacts should use a content-addressed cache and be reused across runs. A dataset release remains reproducible only while its required inputs are available either in that cache or from an immutable upstream archive.

The collector must respect channel rate limits, mirroring policies, signatures, and redistribution terms. Bulk feeds and official mirrors should be preferred over parallel API crawling.

### Curation boundary

Importers should never decide that packages from different channels are the same project solely from names or URLs. They should emit ranked, explainable hints and retain the exact supporting fields. Curation then accepts a project attribution, rejects it, or assigns individual executables to different projects when a package is mixed.

This boundary keeps acquisition repeatable and mechanical while allowing project identity to remain conservative and correctable.

## Further specifications

- Debian repository format: [Debian Repository Format](https://wiki.debian.org/DebianRepository/Format)
- Fedora repository metadata: [DNF repository metadata](https://dnf.readthedocs.io/en/latest/repo_metadata.html)
- Arch package archives: [Creating packages](https://wiki.archlinux.org/title/Creating_packages)
- Homebrew bottles: [Bottles](https://docs.brew.sh/Bottles)
- Cargo registry index: [Registry index format](https://doc.rust-lang.org/cargo/reference/registry-index.html)
- npm command declarations: [`package.json` `bin`](https://docs.npmjs.com/cli/configuring-npm/package-json#bin)
- Python core metadata: [Core metadata specifications](https://packaging.python.org/en/latest/specifications/core-metadata/)
