# Language registry acquisition

## Scope

Language channels follow the distro ownership boundary: acquisition discovers packages and executable declarations; only explicit package attributions in `curations/` produce canonical providers. A matching package name or repository URL never changes ownership.

Collectors use the crates.io dump and explicit npm/PyPI curations:

- crates.io: The full database dump; crate and version tables are streamed to select relevant packages and their newest non-yanked stable releases
- npm: The `latest` release manifest for each explicit package in project curations and package overrides
- PyPI: Simple API project responses for explicit package names, then one wheel per supported current release

npm manifests declare `bin` entries. PyPI extraction requires project responses and static wheel inspection; the global project-name index is not fetched. No npm tarballs or crates.io artifacts are downloaded, and no package is installed, built, imported, or executed.

Fetch jobs write `acquisition-context.tsv` files containing request URLs, response media types, retrieval times, and wheel URLs. Import commands read it with `--context`; snapshots therefore retain retrieval provenance without distributing raw responses or wheels. PyPI snapshots record the maximum Simple API `_last-serial` as a boundary. npm change sequences and crates.io index commits are not recorded.

## Discovery and attribution

crates.io discovery uses explicitly curated registry package names, project IDs, project names, repository basenames, existing channel package names, and normalized repository or homepage URLs. Name hints ignore ASCII case and punctuation, so packages whose registry names differ from the project ID can still be found.

npm/PyPI acquisition refreshes known packages rather than discovering names: only explicit package names in curations and overrides are selected, including scoped npm names. Add verified native package names to curations before fetching. PyPI names must be normalized (lowercase, each run of dots, underscores, or hyphens replaced by one hyphen); non-normalized names fail planning instead of losing their exact attribution.

Discovery hints are not identity decisions. Unavailable npm/PyPI packages are recorded from their HTTP responses; neither channel performs registry-wide discovery.

Review unowned registry packages with matching evidence:

```sh
cargo run -- suggest-registry-packages \
	--acquisitions var/acquired \
	--curations curations \
	--output var/registry-suggestions.json
```

The report lists project IDs, package names, executable declarations, matching evidence, and source URLs, and may contain competing suggestions. Check upstream identity, package provenance, and commands before adding `[[package]]` entries: forks and republished packages can copy upstream URLs. The command edits nothing; regenerate providers after accepting attributions.

## Batch acquisition

```sh
just fetch-languages
just import-languages
just generate
```

Both jobs run without arguments. Each fetch creates a unique UTC-timestamped directory under `var/input/languages/` (override with the `languages_input` justfile variable) and prints its path. After every stage succeeds, it atomically writes that absolute path to `var/input/languages/current`. Imports read the selection once and use one retained directory for all three channels. A failed fetch leaves the previous selection in place; importing before the first completed fetch is an explicit error. Earlier input directories are retained.

Bulk jobs use an optimized Rust build. curl fetches through just recipes; Rust planning and import commands only read local files. The crates.io dump recipe extracts only the two required CSV tables and dump metadata.

Batch imports replace `var/acquired/crates-io.jsonl`, `npm.jsonl`, and `pypi.jsonl` with their adjacent manifests; distro acquisitions remain unchanged. Retain snapshot directories and selected wheels: validation verifies their content identities when generating providers or releases.

Stages can also run separately with explicit paths, without changing the automatic batch selection:

```sh
just fetch-crates-dump var/input/crates-next
just import-crates-dump var/input/crates-next

just fetch-npm var/input/npm-next
just import-npm var/input/npm-next

just fetch-pypi-metadata var/input/pypi-next
just fetch-pypi-wheels var/input/pypi-next
```

A metadata snapshot contains `metadata.json` (the package-to-response plan), `metadata.tsv` (its download list), and `acquisition-context.tsv` (retrieval provenance). Resume interrupted downloads without repeating completed responses:

```sh
just resume-npm var/input/npm-next
just fetch-registry-plan pypi var/input/pypi-next/metadata.tsv
```

Use `import-npm --plan <snapshot>/metadata.json` or `import-pypi --plan <snapshot>/wheels.json` with the desired output, scope, and `--context <snapshot>/acquisition-context.tsv` for individual snapshot imports. Create the metadata plan from current curations before fetching; digest-cached wheels remain reusable. Plans reference absolute local paths. Unplanned files in the response directory or wheel cache do not participate in import or its input digest.

PyPI's per-package fetch and directory import produce partial snapshots with `package-selection=explicit-packages`; they do not claim to cover all curated packages. `just demo` acquires curated npm packages and Black into a fresh directory under `var/demo/` without touching production inputs or output.

## npm refresh jobs

`just fetch-npm` and `just import-npm` form a standalone pair. Fetching creates a fresh UTC-timestamped directory under `var/input/npm/` and writes its absolute path to `var/input/npm/current` only after downloading every planned response; imports require that completed snapshot. Imports read only the selected snapshot; unrelated files under `var/input/npm/` are ignored.

With an explicit snapshot path, the same recipes serve `fetch-languages` and `import-languages`. Explicit fetches create a new directory and never change the standalone or batch selection. Resume with `just resume-npm <snapshot>`, which neither regenerates the plan nor changes the selection, then `just import-npm <snapshot>`. An optional second argument to `import-npm` sets the acquisition output path.

Planning deduplicates package names and requests `https://registry.npmjs.org/<encoded-package>/latest` once per package: one release manifest from the `latest` dist-tag, not a packument or version history. No `_all_docs` scan, name search, or tarball download is performed. Fresh snapshots re-request each release because tags and metadata can change; resumed snapshots reuse completed responses. Requests are sequential and compressed, with a 0.1-second pause, curl retry backoff, and `Retry-After` handling.

## Release and wheel selection

crates.io uses semantic version ordering, excluding yanked and prerelease versions. A selected library-only release is observed without executable candidates; commands from older releases are not retained. Omit `--curations` when invoking `import-crates-dump` directly to import the whole dump instead of narrowing to curation hints.

npm follows the registry's `latest` dist-tag, not the numerically highest version. Imports record the exact version in the retained manifest and verify its package name against the plan; scoped identities retain their scope. Unsupported `directories.bin` declarations are recorded as failed extraction, not library-only observations. A missing package or `latest` release is recorded as failed, not inferred to be unpublished. A release with no executable declarations is observed without commands, replacing any older commands.

PyPI uses PEP 440 ordering over non-yanked stable releases with distribution files; the order of the Simple API's `versions` and `files` arrays is irrelevant. Wheel selection prefers a platform-independent wheel, then smaller declared size, then filename. A newer source-only release is recorded as unsupported; acquisition does not fall back to an older wheel. All-wheel-variant coverage and source-distribution inspection are not implemented.

Wheels are cached by SHA-256 under `var/cache/pypi/wheels/`. Imports verify the expected digest before reading the ZIP, then verify the normalized package name and selected version against its core metadata. Only explicitly selected wheels are imported, so an old cached wheel cannot preserve removed commands. A corrupt cached wheel causes an explicit import failure; remove it and rerun the wheel download job.

## Completeness and failures

npm release responses and PyPI project responses are retained, bounded, non-atomic scans, not point-in-time mirrors. npm writes a completion marker only after every planned request finishes, including retained HTTP 404/410 responses, which make the imported snapshot partial. PyPI journal reconciliation is not implemented.

HTTP 404/410 responses are retained with status sidecars and become failed package observations. Other HTTP errors, interrupted downloads, malformed metadata, missing planned responses, and digest mismatches fail the job. Unsupported PyPI releases and npm executable declarations carry a `reason` in the package observation's variant map. These failures make the snapshot partial; they are never recorded as successful packages with no commands.

A complete npm/PyPI snapshot uses `package-selection=curated-packages` and means every package in the plan was observed. A complete crates.io curation-discovery snapshot means all selected inputs were processed, not that the whole registry was scanned. Published providers remain limited to reviewed curations. Acquisition manifests retain scope and failure details outside the canonical four-field provider relation.

Import commands write `failed` snapshots when parsing or validation fails. Pass `--previous` a preceding acquisitions or release directory to add `removed` package observations for packages absent from the current complete snapshot with the same channel and scope.
