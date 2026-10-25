# Curation guide

Curations are reviewed facts. The dataset contains only the relations a curator has accepted; omission does not mean a package or command is unavailable. This guide defines what belongs in `curations/`, the evidence required, and how to propose additions or corrections. The [curation format](curations.md) defines the schema and resolution semantics.

## Scope

Curations decide:

- Which canonical project a channel package belongs to
- Which executable names a package provides
- Which project is editorially associated with a name

They do not record package versions, dependencies, platforms, installation instructions, categories, popularity, or compatibility. Provider records describe package availability. A provider record means the package exposes the command name, not that the project implements or is compatible with its usual semantics. Name associations carry that editorial distinction.

Coverage is deliberate, not exhaustive. A package, project, or command that is absent may simply not have been reviewed.

## Before contributing

Check the current dataset and the existing files:

- `curations/projects/` for project definitions and ordinary package ownership
- `curations/packages/<channel>/` for exceptional package rules
- `curations/names.toml` for name associations
- `curations/rejections.toml` for hint candidates that must not be submitted again
- [bnck.dev](https://bnck.dev/) or a current release for accepted relations

If you have local acquisition data, the suggestion commands in [CONTRIBUTING](../CONTRIBUTING.md#suggestions) can find candidate packages. Treat suggestions as evidence to investigate, not as decisions.

Open an issue before:

- Creating a project whose upstream identity is unclear
- Moving a package between projects, or merging or splitting projects
- Changing many packages or executables at once
- Changing the schema, adding a channel, or changing resolution semantics

Small additions and corrections can go directly to a pull request.

## Evidence

Every fact that is not directly observed in acquisition data needs durable, public evidence. Channel metadata can establish that a package exists and reports an executable. It does not by itself establish which project owns the package.

Prefer primary sources:

- Upstream repository files: README, manual, changelog, release notes, man pages, source that installs the command, and build definitions such as `Cargo.toml`, `package.json`, or `pyproject.toml`
- Channel package sources: Debian package pages and `Sources`, Fedora specs, Arch `PKGBUILD` files, Homebrew formulae and casks, and registry manifests
- Official standards, specifications, and project documentation for name semantics and alternative implementations

Secondary sources such as distribution bug trackers, packaging mailing lists, and distribution documentation can establish packaging behavior that upstream does not document. Cite the source that demonstrates the claim.

These are not sufficient on their own:

- Similar package, project, or executable names
- A copied homepage or repository URL
- Search result snippets, third-party indexes, or generated summaries
- Forks, mirrors, or repackagers that do not establish upstream identity
- A command mentioned in documentation but not installed or exposed by the package
- Popularity, personal preference, or availability in another channel

References must be public, stable, and relevant. Prefer unversioned documentation or channel source pages; use a tag or commit permalink when the referenced lines change between releases. Do not cite local paths, temporary search URLs, or login-protected pages.

State the conclusion and the evidence in the reason. "The package installs foo" is weak. "Debian's package definition links the source script as /usr/bin/foo" is reviewable. Provide at least one durable reference for every new rule that overrides or extends acquisition, even where the schema makes it optional.

## Projects

Add a project when it is the canonical identity for at least one accepted package or name association. A project with no package and no association contributes no dataset relation; do not add it.

- Use the upstream project's identity and release boundary. Do not create a project per package, repository, or command, and do not group unrelated tools merely because they share an owner or repository.
- Treat an independently maintained fork with its own identity as a separate project. A read-only mirror or a copy that does not have its own release identity is not separate.
- Keep the project ID stable when a project is renamed or moves. Update `name` and the primary URLs instead. Add former locations to `links` only when package metadata may still use them for matching.
- Use official upstream pages for project metadata. `links` may record additional official or ecosystem pages, but not distribution packaging repositories, forks, or mirrors.
- Use the upstream display name without versions, taglines, or package-name conventions.

## Packages

List a package under a project only when:

- The package distributes that project's software rather than merely depending on or invoking it as a separate tool
- The package exposes at least one executable attributed to the project, or an `add` rule supplies one
- The exact `(channel, name)` is present in acquisition, or the channel selects packages from curation and the package exists upstream
- Ordinary ownership covers every acquired executable, or a mixed package exception handles the rest

Distro-maintained packages qualify even when upstream does not publish them. An installer or wrapper that installs the project's command and has no separate software identity of its own may qualify. A package that adds its own commands or behavior is a separate project.

Do not list meta-packages, dependency-only packages, libraries, data-only packages, debug packages, or other packages that expose no executable. They produce no provider records.

For distros, use binary package names, not source package names. Each binary package is a separate key, even when several come from one source package. For npm and PyPI, adding a package entry also selects it for acquisition, so verify the native package name first. A package on one channel does not prove that another channel distributes it.

A package that bundles components from multiple independently maintained projects is a mixed package. Attribute its executables to their projects instead of assigning the whole package to one. If no project has ordinary ownership, every acquired executable must be handled by an `attribute`, `replace`, or `exclude` rule.

## Executables

A provider executable is an exact command name that the package installs or exposes for use. Installed programs qualify even when they are not primarily used from a terminal.

Include public entry points, installed aliases, and commands created by documented install-time mechanisms through `add`. Do not include:

- Test fixtures, fuzz targets, examples, benchmarks, build helpers, or internal helpers not installed for users
- File paths, library names, man page names, subcommands, applet names not installed as commands, or shell builtins, functions, and aliases not installed by the package
- Names that are planned, merely documented, or available only through another package
- Names inferred from branding, the package name, or a likely alias
- Versioned names unless the package installs that exact name

Names are case-sensitive and exact. Do not normalize case, punctuation, extensions, or common aliases. Name collisions are expected: multiple projects may provide the same executable.

Ordinary package ownership covers the acquired executable set. Do not enumerate every command in a multi-command package in a rule, and do not use `add` for a name that acquisition already observes.

## Exceptional rules

Use ordinary ownership whenever it expresses the truth. Add an exception only for a specific package and exact executable, and choose the narrowest rule that fixes the problem.

- `exclude`: The acquired candidate is not a real user-facing command or is not installed as an executable. Examples include test fixtures, internal build tools, and non-executable files under a package binary directory. Do not exclude real commands because they are obscure, deprecated, duplicated, or not your preferred implementation.
- `replace`: The acquired name is an implementation artifact rather than the user-facing command, and the package exposes the command under a different name. Replacement suppresses the acquired name, so use it only when that name is not itself a valid entry point. If the package exposes both names, keep the acquired name and use `add` for the missing one.
- `attribute`: The package contains executables from more than one independently maintained project. Attribute only the commands from the other project. Do not use it to split commands within one project.
- `add`: The package installs or exposes a command that static acquisition cannot observe. Alternatives, post-install symlinks or wrappers, and parser limitations are typical cases. The command must not already be acquired, must be shown by the package or installation mechanism, and needs a reason and durable reference. Do not add documented-but-uninstalled commands, subcommands, applets, shell builtins, or aliases the package does not create.

Rules must remain valid against the current acquired executable set. Update or remove a rule when acquisition changes; a rule that refers to an absent executable is a validation error.

## Name associations

Associations are editorial relations between an exact executable name and a project. They never add provider records and never establish ownership.

- `reference`: The project most strongly associated with the name. This is usually the project that defines the standard, the original implementation, or the de facto reference. At most one reference is allowed per executable. Omit it when there is no defensible choice; do not default to the first or most popular provider. The reference project need not provide the command on a current channel.
- `implementation`: A project known to implement the command semantics under another name, through an applet namespace, or through a multicall binary. Do not add an implementation merely because the project provides the same exact command; the provider record already states that.

Both roles need a reason and durable evidence that explains the relationship. Add associations that answer a likely lookup, not an exhaustive list of every compatible tool. Duplicate `(executable, project)` pairs and multiple references for one executable are invalid.

## Corrections

Corrections are as important as additions. Fix the source fact rather than editing generated output.

- Wrong owner: Move the package entry to the correct project and update any package exception that refers to project IDs. If the old project has no packages and no associations, remove its file.
- Wrong project identity: Keep the stable ID for a rename or move and update metadata. Merge files only when they describe the same upstream project. Split a project only when it contains independently maintained projects.
- Wrong executable: Use `exclude`, `replace`, or `add` according to the actual installed command. If acquisition is correct and the curation is stale, update the curation instead of adding an exception.
- Missing package or executable: Add it only with evidence. Do not infer a channel package from another channel or a command from project branding.
- Removed package: A recorded removal allows the entry to remain, but it contributes no provider records. Remove the package entry and exception when the removal is permanent; do not add rules for a package that is no longer distributed.
- Wrong association: Remove or correct it, and preserve at most one reference per executable. Do not add a duplicate implementation under a new reason.

Do not treat absence from a partial acquisition as a removal. Staleness is determined by a complete channel snapshot. Do not revert an evidence-backed change without new evidence; discuss it on the pull request or issue first.

## Submitting changes

1. Edit only the relevant files under `curations/`. Use the existing TOML layout and keep each file focused on one project or package.
2. Give every new or changed rule a specific reason and durable references. Do not hand-edit `providers.jsonl`, `names.jsonl`, acquisition snapshots, or other generated files.
3. Keep one topic per pull request. Separate formatting-only changes from factual changes.
4. Run `just check` if you have the development environment. It covers repository checks but does not resolve curations against acquisition data.
5. Run `just generate` with acquisition data that covers every affected channel, if you can. This validates stale packages, missing executables, duplicate ownership, incomplete mixed packages, and the other conditions listed in [curation validation](curations.md#validation). A focused acquisition for the affected channel is enough when it is complete for that channel and includes every package the change touches.
6. If you cannot run acquisition locally, say so in the pull request. Maintainers can validate the change against a full snapshot before merging.

The pull request should state:

- The relations added, corrected, or removed
- The evidence for each decision, including the sources that establish project identity and package ownership
- The acquisition snapshot or date used for validation, if any
- The checks you ran
- Any remaining uncertainty or alternatives you considered

Do not change the schema version, add fields, or add channels in a curation pull request. Propose those changes separately.

Hint rejections belong in `curations/rejections.toml` only for reviewed automatic candidates that must not be submitted again. A rejection affects hint ingestion only; it does not remove an existing provider or correct a curation. Remove the rejection when the underlying hint or identity issue is resolved.

## Review criteria

A curation change is ready when:

- Project identity is established from upstream or channel evidence, not from names alone
- Package ownership is complete for the acquired executable set, with exceptions used only where necessary
- Executable names are exact, installed, and intended for use
- Every rule is minimal, non-contradictory, and valid against the current acquired executable set
- Reasons are specific, and references are public, durable, and directly support the claim
- Project IDs are stable, package names are channel-native, and exception files are under the correct channel directory
- Name associations use valid roles, have at most one reference, and are not used as a substitute for package ownership
- The pull request contains no unrelated changes or generated artifacts
- The affected channel has been validated against complete acquisition data, or the pull request states why it was not

Curation is editorial. Meeting these criteria makes a change reviewable; it does not guarantee inclusion.
