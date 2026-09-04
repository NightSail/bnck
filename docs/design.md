# Core design

## Purpose

bnck is an executable name database that maps executable names to projects and distribution packages.

It answers two questions:

- Given an executable name, which projects provide it, and through which distribution channels?
- Given a project, which executable names does it provide, and through which distribution channels?

## Concepts

- Project: The canonical identity of the software project to which an executable is attributed
- Executable name: The end-user-facing command used to invoke an executable, with the exact spelling
- Distribution channel: The package namespace and source through which a set of software is distributed; examples include Debian, Homebrew, and crates.io
- Package: A channel-native distribution unit that provides one or more executables, identified by a `(distribution channel, package name)` tuple
- Provider record: A relation stating that a package from a distribution channel provides an executable attributed to a project
- Acquisition: Automated collection and normalization of package-to-executable mappings from distribution channel data
- Curation: Checked-in, human-maintained data that resolves acquired mappings into canonical project attribution


## Core data model

The canonical fact is a provider relation:

```text
(project, executable name, distribution channel, package name)
```

For example:

```text
helix | hx | crates.io | helix-term
helix | hx | Debian    | helix
```

Provider records are the canonical queryable data.

Multiple projects may provide the same executable name, and a package may contain executables attributed to multiple projects.

## Acquisition and curation

Importers perform acquisition and produce candidate mappings. These candidates describe what each channel reports, but do not establish which packages across channels belong to the same project.

Curations define:

- The canonical projects represented in the dataset
- The project to which each package is attributed
- Corrections or replacements for executable names reported by importers
- Per-executable attribution when a package contains executables from multiple projects

Curations are the source of truth for connecting packages of the same project. Dataset generation combines them with acquired candidate mappings to produce provider records.

## Dataset semantics

A given version of the dataset is a catalog of accepted provider relations at generation time. It does not provide historical observations. Absence of a specific provider record only means it was not observed at the time.

## Design rule

Add a field or entity only when required by a supported query, necessary curation, or ambiguity in real data.
