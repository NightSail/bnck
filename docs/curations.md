# Curation format

## Purpose

Curations connect acquired channel packages and executable names to canonical projects. They are the human-maintained source of truth for decisions that channel metadata cannot establish safely:

- Canonical project identities
- Channel packages belonging to each project
- Per-executable attribution for mixed packages
- Exclusions for false executable candidates
- Replacements for incorrectly reported executable names
- Additions that static acquisition cannot observe

Acquired package metadata and executable lists stay out of curation files. Dataset generation combines immutable acquisition snapshots with a specific curation revision to produce provider records.

## Format

Curations are TOML: human-readable, comment-friendly, strictly parseable, and non-executable. Package attribution is project-centric, so one project and all its packages live in one file.

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

Files under `projects/` define projects and ordinary package ownership. Files under `packages/<channel>/` hold only exceptional package-level executable rules; the directory must agree with the exception file's `channel` field.

Base filenames are organizational and do not establish identity. Exact project and package identities come from fields inside each file, which avoids filename encoding rules for scoped npm packages, Homebrew taps, and names containing `/`.

## Schema version

`curations/manifest.toml` declares the schema version:

```toml
schema = 0
```

This version is independent of the provider, release, acquisition, and browser schema versions. An incompatible change increments it and migrates affected files atomically. Changes to field names, types, required fields, or resolution semantics are incompatible; current readers reject unknown fields, so adding a field is also incompatible.

## Project definitions

Each canonical project has one file under `curations/projects/`, containing project metadata and every ordinary channel package attributed to it.

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

Required fields:

- `id`: Stable machine-readable curation identity
- `name`: Human-readable project name

Optional project hints: `homepage`, `repository`, `documentation`, `issue_tracker`, and `links`.

The first four describe the project's primary URLs. `links` records additional official or ecosystem URLs that package metadata may use for the same project: registry pages, former locations, and further project or documentation pages. For example, hledger packages may refer to either of two Hackage pages:

```toml
links = [
	'https://hackage.haskell.org/package/hledger',
	'https://hackage.haskell.org/package/hledger-ui',
]
```

Links are matching evidence only; they never establish ownership. Do not add distribution packaging repositories, forks, or mirrors merely because a package record mentions them.

Each `package` entry requires:

- `channel`: Stable channel key
- `name`: Exact channel-native package identity

Project IDs use lowercase ASCII letters, digits, hyphens, periods, and underscores. They must be stable and independent of repository location or branding.

Project metadata aids curator review but establishes no relationship and need not appear in the canonical provider release.

## Package ownership

A package identity is the tuple `(channel, name)`. Listing it in a project file gives that project default ownership of every acquired executable from the package, subject to an exceptional package curation.

A package may be listed by at most one project; multiple claims are a validation error.

Packages containing executables from multiple projects are not listed as ordinary packages. Their attribution comes entirely from an exceptional package curation. Packages with neither ordinary ownership nor mixed-package attribution produce no provider records.

## Exceptional package curations

Files under `curations/packages/<channel>/` contain only package-level exceptions. Most packages need none. An exception file identifies the package without repeating its default project:

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

The default project is the one claiming `(fedora, example)`.

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

Every acquired executable must be handled by an `attribute`, `replace`, or `exclude` rule; an unhandled one is a validation error.

## Executable rules

Rules match exact executable names: no patterns, inferred aliases, or name normalization.

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

The executable must exist in acquisition data for the selected package.

### Exclude

An `exclude` rule suppresses a false or unsuitable acquired candidate, as in the exception-file example above. A reason is required because the rule overrides acquired channel data.

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

Replacement suppresses `acquired`, adds `executable`, and attributes it to `project`. The acquired name must exist. No fuzzy matching or general name normalization is performed.

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

Additions require a reason and at least one durable reference, and do not claim the executable was observed by the importer.

## Resolution

Dataset generation resolves each curated package in this order:

1. Find ordinary ownership from project package declarations
2. Find the optional exceptional package curation
3. Load the acquired executable set
4. Validate exceptional rules against the unmodified acquired set
5. Apply exclusions
6. Apply replacements
7. Apply per-executable attributions
8. Attribute remaining executables to the ordinary owner
9. Apply additions
10. Validate that each resulting executable has exactly one project attribution

Rule order inside a TOML file does not change these semantics.

Curations for channels outside a generation run's selected acquisition inputs are not resolved. A curated package missing from a selected channel is an error.

Release provenance identifies the pinned acquisition snapshots, the source revision containing the curations and generator, and the applicable schema versions.

## Scale

The format stores `O(P + M)` facts for `P` projects and `M` ordinary project-to-package relationships, but uses `O(P + E)` files for `E` exceptional packages. Ordinary projects need one file regardless of channel count. Exceptional package files can be sharded by directory without changing package identity.

## Deliberately unsupported rules

- Regular expressions
- Globs
- Package-name prefixes
- Executable-name prefixes
- Arbitrary scripts or expressions
- Package-version ranges
- Architecture-dependent conditions
- Channel-specific embedded rule languages

Pattern rules could silently match upstream data no curator reviewed. Version ranges would move curation toward historical observations, which the dataset does not provide.

Add architecture- or release-specific rules only after concrete cases show the channel-level model cannot represent necessary attribution.

## Validation

Validation fails on:

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
- An addition duplicating an acquired or replaced executable
- A mixed package with unhandled acquired executables
- A rule referring to an acquired executable that no longer exists
- A curated package absent from the selected complete channel snapshot

Validation must distinguish stale curation from incomplete acquisition: a failed or partial importer run must not make valid curation appear stale.

Validation warns on:

- Project metadata disagreeing with current channel hints
- Repository URLs that redirect
- New executables under an ordinarily owned package
- Acquired executable sets changing between snapshots
- Projects with no currently observed packages

Warnings do not block dataset generation.
