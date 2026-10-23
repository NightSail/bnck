# Core design

## Purpose

bnck maps executable names to projects and distribution packages. It answers:

- Given an executable name, which projects provide it, through which distribution channels?
- Given a project, which executable names does it provide, through which distribution channels?
- Which projects are editorially associated with a name as its reference or a known implementation?

## Concepts

- Project: The canonical identity of the software project to which an executable is attributed
- Executable name: The end-user-facing command used to invoke an executable, with its exact spelling
- Distribution channel: The package namespace and source through which software is distributed; for example Debian, Homebrew, or crates.io
- Package: A channel-native distribution unit providing one or more executables, identified by `(distribution channel, package name)`
- Provider record: A relation stating that a package from a distribution channel provides an executable attributed to a project
- Name association: A curated, channel-independent link between an executable name and a project, with role `reference` or `implementation`
- Acquisition: Automated collection and normalization of package-to-executable mappings from channel data
- Curation: Checked-in, human-maintained data that resolves acquired mappings into canonical project attribution

## Core data model

The canonical fact is a provider relation:

```text
(project, executable name, distribution channel, package name)
```

```text
ripgrep | rg | arch | ripgrep
ripgrep | rg | crates.io | ripgrep
```

Multiple projects may provide the same executable name, and a package may contain executables attributed to multiple projects. A provider record means the package exposes the command name, directly or through an installed alias; it does not mean the project implements, emulates, or is compatible with that command.

Name associations record that editorial distinction:

```text
(name, project, role)
```

`reference` identifies the project most strongly associated with a name. `implementation` identifies a project known to implement the name under another command or through an applet namespace. An association may exist without any current provider record for the pair.

## Acquisition and curation

Importers produce candidate mappings: what each channel reports, without establishing which packages across channels belong to the same project.

Curations define:

- The canonical projects represented in the dataset
- The project to which each package is attributed
- Corrections or replacements for executable names reported by importers
- Per-executable attribution when a package contains executables from multiple projects
- Reference and implementation associations for executable names

Dataset generation combines curations with acquired candidates to produce provider records and emits curated name associations separately.

## Dataset semantics

A dataset version catalogs the provider relations and name associations accepted at generation time. It carries no historical package observations, so a missing provider record only means the relation was not observed then. Name associations are editorial claims about meaning, not historical availability.

## Design rule

Add a field or entity only when required by a supported query, necessary curation, or ambiguity in real data.
