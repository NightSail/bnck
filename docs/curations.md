# Curation format

## Purpose

Curations connect acquired channel packages and executable names to canonical projects. They are the human-maintained source of truth for decisions that channel metadata cannot establish safely:

- Canonical project identities
- Channel packages belonging to each project
- Per-executable attribution for mixed packages
- Exclusions for false executable candidates
- Replacements for incorrectly reported executable names
- Additions that static acquisition cannot observe
- Editorial associations between executable names and reference or implementation projects

Acquired package metadata and executable lists stay out of curation files. Dataset generation combines immutable acquisition snapshots with a specific curation revision to produce provider records.

## Format

Curations are TOML: human-readable, comment-friendly, strictly parseable, and non-executable. Package attribution is project-centric, so one project and all its packages live in one file.

```text
curations/
├── manifest.toml
├── names.toml
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
schema = 1
```

This version is independent of the provider, release, acquisition, and browser schema versions. An incompatible change increments it and migrates affected files atomically. Changes to field names, types, required fields, or resolution semantics are incompatible; current readers reject unknown fields, so adding a field is also incompatible. Schema 1 adds name associations.

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

A package identity is the tuple `(channel, name)`. Listing it in a project file gives that project default ownership of every acquired executable from the package, subject to exceptional package curation.

A package may be listed by at most one project; multiple claims are a validation error.

A package can have an ordinary owner and exceptional executable rules. The remaining executables are attributed to the ordinary owner. A package without an ordinary owner is mixed: every acquired executable must be handled by an `attribute`, `replace`, or `exclude` rule. Packages with neither ordinary ownership nor mixed-package attribution produce no provider records.

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

The default project is the one claiming `(fedora, example)`. If no project claims the package, the exception must handle every acquired executable as a mixed package.

### Mixed packages

A mixed package has no ordinary owner. Its exception file explicitly attributes every acquired executable that is not excluded or replaced:

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

An addition also covers a command name that a channel creates at install time. Debian registers `/usr/bin/awk` and `/usr/bin/nawk` through `update-alternatives`, while `Contents` lists only the implementation binaries. Each package that registers the alternative receives its own addition:

```toml
[[add]]
executable = 'awk'
project = 'gawk'
reason = 'Debian declares Provides: awk and registers /usr/bin/awk through update-alternatives'
references = [
	'https://packages.debian.org/sid/gawk',
	'https://sources.debian.org/src/gawk/',
]
```

## Bundled and indirect commands

A provider record means the package exposes the command name, directly or through an installed alias. It does not mean the project implements, emulates, or is compatible with that command.

### Bundled commands

Packages that install several commands use ordinary package ownership. One project file with the package entries attributes every acquired executable. Do not list individual commands in curation; the acquired executable set stays authoritative. For example, `coreutils` owns `ls`, `cat`, `cp`, and the rest of its installed commands without executable rules.

### Alternative command names

A channel can create a command name that its file listings do not expose. Debian's `gawk`, `mawk`, and `original-awk` packages declare `Provides: awk` and register `/usr/bin/awk` and `/usr/bin/nawk` through `update-alternatives`. Each package that registers the alternative receives an `add` rule. Do not use `replace`: the implementation's own command remains installed. Add the name only for channels and packages where the mechanism is present; the same project may provide it on one channel and not another.

### Multicall binaries and applets

Multicall packages expose a dispatcher or an applet namespace, not every applet as a command. Record only the names the package installs or exposes through a documented channel mechanism. BusyBox and ToyBox applets are not separate provider records; uutils packages in Fedora and Arch install `uu_ls` and `uu-ls`, and its Debian package installs a `coreutils` multicall binary. A drop-in replacement is a project property, not a provider relation.

Shell builtins are also not provider records unless the package installs a command for the name.

## Name associations

`curations/names.toml` records editorial associations between executable names and projects. They supplement provider records; they never assert that a package installs the name.

```toml
[[association]]
executable = 'ls'
project = 'coreutils'
role = 'reference'
reason = 'GNU coreutils is the reference implementation of the POSIX ls utility'
references = [
	'https://pubs.opengroup.org/onlinepubs/9699919799/utilities/ls.html',
	'https://www.gnu.org/software/coreutils/manual/html_node/ls-invocation.html',
]

[[association]]
executable = 'ls'
project = 'uutils-coreutils'
role = 'implementation'
reason = 'uutils implements ls under uu-ls, uu_ls, and the coreutils multicall binary'
references = ['https://github.com/uutils/coreutils']
```

Each association requires:

- `executable`: Exact command name
- `project`: Existing project ID
- `role`: Either `reference` or `implementation`
- `reason`: Non-empty explanation
- `references`: At least one durable reference

`reference` identifies the project most strongly associated with the name. At most one reference is allowed per executable. `implementation` identifies a project known to implement the name under another command or through an applet namespace. An association may name a project with no current provider record for that executable; that is the purpose of the separate relation.

Associations do not affect provider resolution. Duplicate `(executable, project)` pairs and multiple references for one executable are validation errors.

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

Curations for channels outside a generation run's selected acquisition inputs are not resolved. A curated package missing from a complete selected channel snapshot is an error. Absence from a partial acquisition is skipped, not treated as stale curation.

Name associations are resolved independently of package resolution. They are emitted separately from provider records.

Release provenance identifies the pinned acquisition snapshots, the source revision containing the curations and generator, and the applicable schema versions.

## Scale

The format stores `O(P + M)` facts for `P` projects and `M` ordinary project-to-package relationships, but uses `O(P + E)` files for `E` exceptional packages. Ordinary projects need one file regardless of channel count. Exceptional package files can be sharded by directory without changing package identity. Name associations add `O(A)` facts in one file.

## Automatic hint ingestion

Terminal Trove and tldr pages are discovery hints, not ownership decisions. Normalized hint records are JSON Lines with `source`, `source_id`, `name`, and optional `executable`, `repository`, `homepage`, and `references` fields. Provenance for a batch is recorded beside the hint files.

`bnck ingest-hints` reads a hint directory, cross-checks every candidate against acquired packages, and adds only packages with URL identity plus package, source-package, or executable-name evidence. It skips packages already claimed by a curation, drops ambiguous package attributions, and never removes or rewrites existing project metadata. A candidate for an existing project URL appends its unclaimed packages; otherwise a new project file is written. The command previews by default, applies with `--apply`, and can record an audit report.

Ingestion is idempotent. Re-running the same or a newer hint batch does not duplicate project files or package declarations.

## Hint rejections

`curations/rejections.toml` records automatic candidates that must not be submitted again. Rejections affect hint ingestion only; they do not change dataset generation.

Reject a whole candidate by its normalized project URL:

```toml
[[rejection]]
key = 'github.com/example/tool'
reason = 'The hint identifies the wrong upstream project'
```

Hint records without a URL use a source-prefixed key such as `terminaltrove:tool` or `tldr:common/tool`. Add a `source` to keep the rejection effective if the URL changes:

```toml
[[rejection]]
key = 'github.com/example/tool'
source = 'terminaltrove:tool'
reason = 'The hint identifies the wrong upstream project'
```

The key and source are both matched; either one rejects the candidate. Reject one package association while keeping the candidate's other packages by adding both `channel` and `package`:

```toml
[[rejection]]
key = 'github.com/example/tool'
source = 'terminaltrove:tool'
channel = 'debian'
package = 'tool-extra'
reason = 'The Debian package is a different project'
```

Each rejection requires a non-empty reason. Remove the entry to allow the candidate to be considered again. When rejecting a whole automatically created project, also delete its project curation; otherwise the existing curation remains in the dataset.

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
- An association referencing an unknown project
- An unknown association role
- An association without a reason or reference
- A duplicate association for one executable and project
- Multiple reference associations for one executable

Validation must distinguish stale curation from incomplete acquisition: a failed or partial importer run must not make valid curation appear stale.

Validation is strict: detected issues fail generation rather than producing non-blocking warnings.
