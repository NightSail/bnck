# Curation format

## Purpose

Curations connect acquired channel packages and executable names to canonical projects. They are the human-maintained source of truth for decisions that channel metadata cannot establish safely.

Curations define:

- Canonical project identities
- Accepted channel packages
- Default project attribution for a package
- Per-executable attribution for mixed packages
- Exclusions for false executable candidates
- Replacements for incorrectly reported executable names
- Additions that static acquisition cannot observe

Acquired package metadata and executable lists remain outside curation files. Dataset generation combines an immutable acquisition snapshot with a specific curation revision to produce provider records.

## Format

Curations use TOML because it is human-readable, comment-friendly, strictly parseable, and non-executable. Files should remain small and independently reviewable.

The directory layout is:

```text
curations/
├── manifest.toml
├── projects/
│   ├── helix.toml
│   └── ripgrep.toml
└── packages/
    ├── arch/
    │   └── ripgrep.toml
    ├── crates.io/
    │   ├── helix-term.toml
    │   └── ripgrep.toml
    ├── debian/
    │   ├── helix.toml
    │   └── ripgrep.toml
    └── npm/
        └── example-cli.toml
```

Directory location determines the file schema: files under `projects/` are project definitions, while files under `packages/<channel>/` are package curations. The package directory must agree with the file's `channel` field. Base filenames are organizational and do not establish identity; exact project and package identities come from fields inside each file. This avoids filename encoding rules for scoped npm packages, Homebrew taps, and other package names containing `/`.

## Schema version

`curations/manifest.toml` declares the curation schema version:

```toml
schema = 1
```

The curation schema version is independent of the provider and release manifest schema versions. An incompatible curation format change increments this value and migrates affected files atomically.

## Project definitions

Each canonical project has one file under `curations/projects/`.

`curations/projects/helix.toml`:

```toml
id = 'helix'
name = 'Helix'

homepage = 'https://helix-editor.com/'
repository = 'https://github.com/helix-editor/helix'
```

Required fields are:

- `id`: Stable machine-readable curation identity
- `name`: Human-readable project name

Optional project hints include:

- `homepage`
- `repository`
- `documentation`
- `issue_tracker`

Project IDs must be stable, lowercase, human-readable, and independent of repository location. Repository moves and branding changes do not normally change the project ID.

Project metadata helps curators assess package relationships. It does not establish a relationship automatically and does not need to be included in the canonical provider release.

## Package curations

Each accepted channel package has one file under `curations/packages/<channel>/`.

The package identity is the tuple formed by `channel` and `name`. Two files defining the same tuple are invalid regardless of their file paths.

### Default package attribution

A package whose executable candidates all belong to one project uses a default attribution.

`curations/packages/crates.io/helix-term.toml`:

```toml
channel = 'crates.io'
name = 'helix-term'
project = 'helix'
```

This attributes every acquired executable from `(crates.io, helix-term)` to `helix`, except where an explicit rule overrides or excludes it.

Default attribution keeps common package curations small and avoids copying acquired executable lists into Git.

### Mixed-package attribution

A package containing commands from multiple projects omits the default `project` and attributes each executable explicitly:

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

Without a default project, every acquired executable must be handled by an `attribute`, `replace`, or `exclude` rule. An unhandled executable is a validation error.

A package with a default project may also override individual executable attribution:

```toml
channel = 'debian'
name = 'example-tools'
project = 'example'

[[attribute]]
executable = 'vendor-helper'
project = 'vendor-helper'
reason = 'Built from a separately maintained bundled project'
references = [
	'https://example.org/packaging-notes',
]
```

## Executable rules

Rules match exact executable names. They do not use patterns, inferred aliases, or normalized names.

### Attribute

An `attribute` rule assigns an acquired executable to a project instead of the package default:

```toml
[[attribute]]
executable = 'vendor-helper'
project = 'vendor-helper'
reason = 'Provided by a separately maintained bundled project'
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

Dataset generation resolves one package curation against one acquired package in this order:

1. Load the acquired executable set
2. Validate all rules against the unmodified acquired set
3. Apply exact exclusions
4. Apply exact replacements
5. Apply explicit per-executable attributions
6. Attribute remaining executables to the default project
7. Apply explicit additions
8. Validate that each resulting executable has exactly one project attribution

Rule ordering inside a TOML file does not change these semantics.

Packages without an accepted curation do not produce provider records. Curations therefore establish the boundary between acquired candidates and released data. Package curations for channels outside a generation run's selected acquisition inputs are not resolved; a curated package missing from a selected channel remains an error.

Dataset generation resolves curations against pinned acquisition snapshots. Release provenance identifies those snapshots, the source revision containing the curations and generator, and the applicable schema versions.

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
- Duplicate package identities
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
- New executables appearing under a package with a default project
- Acquired executable sets changing between snapshots
- Projects with no currently observed packages

Warnings do not block dataset generation.
