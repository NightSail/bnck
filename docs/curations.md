# Curation format

## Purpose

Curations connect acquired channel packages and executable names to canonical projects. They are the human-maintained source of truth for decisions that channel metadata cannot establish safely.

Curations define:

- Canonical project identities
- Channel packages belonging to each project
- Per-executable attribution for mixed packages
- Exclusions for false executable candidates
- Replacements for incorrectly reported executable names
- Additions that static acquisition cannot observe

Acquired package metadata and executable lists remain outside curation files. Dataset generation combines immutable acquisition snapshots with a specific curation revision to produce provider records.

## Format

Curations use TOML because it is human-readable, comment-friendly, strictly parseable, and non-executable. Ordinary package attribution is project-centric so that one project and all its channel packages can be maintained in one file.

The directory layout is:

```text
curations/
├── manifest.toml
├── projects/
│   ├── helix.toml
│   └── ripgrep.toml
└── packages/
    ├── debian/
    │   └── example-tool-bundle.toml
    └── fedora/
        └── example.toml
```

Files under `projects/` define projects and ordinary package ownership. Files under `packages/<channel>/` are needed only for exceptional package-level executable rules. The package directory must agree with the exception file's `channel` field.

Base filenames are organizational and do not establish identity. Exact project and package identities come from fields inside each file. This avoids filename encoding rules for scoped npm packages, Homebrew taps, and other package names containing `/`.

## Schema version

`curations/manifest.toml` declares the curation schema version:

```toml
schema = 0
```

The curation schema version is independent of the provider and release manifest schema versions. An incompatible curation format change increments this value and migrates affected files atomically.

## Project definitions

Each canonical project has one file under `curations/projects/`. The file contains project metadata and every ordinary channel package attributed to it.

`curations/projects/ripgrep.toml`:

```toml
id = 'ripgrep'
name = 'ripgrep'

homepage = 'https://burntsushi.net/ripgrep/'
repository = 'https://github.com/BurntSushi/ripgrep'

[[package]]
channel = 'arch'
name = 'ripgrep'

[[package]]
channel = 'crates.io'
name = 'ripgrep'

[[package]]
channel = 'debian'
name = 'ripgrep'

[[package]]
channel = 'fedora'
name = 'ripgrep'

[[package]]
channel = 'homebrew'
name = 'homebrew/core/ripgrep'
```

Required project fields are:

- `id`: Stable machine-readable curation identity
- `name`: Human-readable project name

Optional project hints include:

- `homepage`
- `repository`
- `documentation`
- `issue_tracker`

Each `package` entry requires:

- `channel`: Stable channel key
- `name`: Exact channel-native package identity

Project IDs must be stable, lowercase, human-readable, and independent of repository location. Repository moves and branding changes do not normally change the project ID.

Project metadata helps curators assess package relationships. It does not establish a relationship automatically and does not need to be included in the canonical provider release.

## Package ownership

A package identity is the tuple `(channel, name)`. A package listed by one project gives that project default ownership of every acquired executable from the package, subject to an exceptional package curation.

A package may be listed by at most one project. Multiple project files claiming the same package are a validation error.

Packages containing executables from multiple projects are not listed as ordinary packages in any project file. Their attribution is defined entirely by an exceptional package curation.

Packages without ordinary ownership or exceptional mixed-package attribution do not produce provider records.

## Exceptional package curations

Files under `curations/packages/<channel>/` contain only package-level exceptions. Most packages do not need one.

An exception file identifies the package but does not repeat its default project:

```toml
channel = 'fedora'
name = 'example'

[[exclude]]
executable = 'example-data'
reason = 'The path is non-executable despite being installed under /usr/bin'
references = [
	'https://src.fedoraproject.org/rpms/example',
]
```

The default project comes from the project file that claims `(fedora, example)`.

### Mixed packages

A mixed package is not claimed by a project file. Its exception file explicitly attributes every acquired executable that is not excluded or replaced:

```toml
channel = 'debian'
name = 'example-tool-bundle'

[[attribute]]
executable = 'foo'
project = 'foo'
reason = 'Built from the bundled Foo project'

[[attribute]]
executable = 'bar'
project = 'bar'
reason = 'Built from the bundled Bar project'
```

Without ordinary project ownership, every acquired executable must be handled by an `attribute`, `replace`, or `exclude` rule. An unhandled executable is a validation error.

## Executable rules

Rules match exact executable names. They do not use patterns, inferred aliases, or normalized names.

### Attribute

An `attribute` rule assigns an acquired executable to a project instead of the package's ordinary owner:

```toml
[[attribute]]
executable = 'vendor-helper'
project = 'vendor-helper'
reason = 'Provided by a separately maintained bundled project'
references = [
	'https://example.org/packaging-notes',
]
```

The executable must be present in acquisition data for the selected package.

### Exclude

An `exclude` rule suppresses a false or unsuitable acquired candidate:

```toml
[[exclude]]
executable = 'example-data'
reason = 'The path is non-executable despite being installed under /usr/bin'
references = [
	'https://src.fedoraproject.org/rpms/example',
]
```

Exclusions require a reason because they override acquired channel data.

### Replace

A `replace` rule substitutes the user-facing command name for an acquired name:

```toml
[[replace]]
acquired = 'example.py'
executable = 'example'
project = 'example'
reason = 'The package links the source script under the command name example'
references = [
	'https://example.org/package-definition',
]
```

Replacement has exact semantics:

1. Suppress the executable named by `acquired`
2. Add the executable named by `executable`
3. Attribute the replacement to `project`

The acquired name must exist. Replacement does not perform fuzzy matching or general name normalization.

### Add

An `add` rule supplies an executable that static acquisition cannot observe:

```toml
[[add]]
executable = 'example-admin'
project = 'example'
reason = 'The static formula parser cannot resolve the expression that creates this wrapper'
references = [
	'https://example.org/package-definition',
]
```

Additions require a reason and at least one durable reference. They do not claim that the executable was observed by the importer.

## Resolution

Dataset generation resolves each curated package in this order:

1. Find ordinary ownership from project package declarations
2. Find the optional exceptional package curation
3. Load the acquired executable set
4. Validate all exceptional rules against the unmodified acquired set
5. Apply exact exclusions
6. Apply exact replacements
7. Apply explicit per-executable attributions
8. Attribute remaining executables to the ordinary owner
9. Apply explicit additions
10. Validate that each resulting executable has exactly one project attribution

Rule ordering inside a TOML file does not change these semantics.

Package curations for channels outside a generation run's selected acquisition inputs are not resolved. A curated package missing from a selected channel remains an error.

Dataset generation resolves curations against pinned acquisition snapshots. Release provenance identifies those snapshots, the source revision containing the curations and generator, and the applicable schema versions.

## Scale

Let:

- `P` be the number of curated projects
- `M` be the number of ordinary project-to-package relationships
- `E` be the number of exceptional packages

The format stores `O(P + M)` facts because every package relationship must be represented, but it uses only `O(P + E)` files. Ordinary projects use one file regardless of how many channels distribute them.

If exceptional package files become numerous enough to create filesystem problems, their directory layout can be sharded without changing package identity.

## Deliberately unsupported rules

The initial schema does not support:

- Regular expressions
- Globs
- Package-name prefixes
- Executable-name prefixes
- Arbitrary scripts or expressions
- Package-version ranges
- Architecture-dependent conditions
- Channel-specific embedded rule languages

Pattern rules could silently match new upstream data that no curator reviewed. Version ranges would also move the curation model toward historical observations, which the dataset does not provide.

Architecture- or release-specific rules should be added only after concrete cases demonstrate that the current channel-level model cannot represent necessary attribution.

## Validation

Curation validation fails on:

- Unknown project IDs
- Unknown channel keys
- Duplicate project IDs
- A package claimed by multiple projects
- Duplicate exceptional package curations
- Duplicate or contradictory executable rules
- Empty project, package, or executable names
- Unsafe executable names containing `/` or NUL
- An executable that is both excluded and attributed
- A replacement whose acquired name is independently excluded or attributed
- An addition that duplicates an acquired or replaced executable
- A mixed package with unhandled acquired executables
- A rule referring to an acquired executable that no longer exists
- A curated package absent from the selected complete channel snapshot

Validation must distinguish stale curation from incomplete acquisition. A failed or partial importer run must not make valid curation appear stale.

Curation validation reports warnings for:

- Project metadata disagreeing with current channel hints
- Repository URLs that redirect
- New executables appearing under an ordinarily owned package
- Acquired executable sets changing between snapshots
- Projects with no currently observed packages

Warnings do not block dataset generation.
