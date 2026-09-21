# Language registry acquisition

## Scope

Language channels follow the same ownership boundary as distro channels: acquisition discovers packages and executable declarations; only explicit package attributions in `curations/` produce canonical providers. A matching package name or repository URL never changes ownership automatically.

The collectors use the crates.io dump and explicit npm/PyPI package curations:

- crates.io: Download the full database dump, extract the crate and version tables, and stream them to select relevant packages and their newest non-yanked stable releases
- npm: Fetch the `latest` release manifest for each explicit npm package name in project curations and package overrides
- PyPI: Fetch Simple API project responses for explicit PyPI package names in project curations and package overrides, then one wheel per supported current release

npm release manifests contain `bin` declarations. PyPI executable extraction requires project responses and static wheel inspection; the global project-name index is not fetched. Neither npm tarballs nor crates.io package artifacts are downloaded. The crates.io dump supplies all required package and executable metadata; no per-package API requests are made. No packages are installed, built, imported, or executed.

## Discovery and attribution

crates.io discovery includes explicitly curated registry package names, project IDs, project names, repository basenames, and existing channel package names. Name hints ignore ASCII case and punctuation. The crates.io dump also permits discovery by normalized repository or homepage URL, including packages whose registry names differ from the project ID.

npm/PyPI acquisition refreshes known packages rather than discovering names. Only explicit package names in curations are selected, including scoped npm names and package overrides. Add verified native package names to curations before fetching. PyPI curations must use normalized names: lowercase, with each run of dots, underscores, or hyphens replaced by one hyphen. Non-normalized names fail planning rather than losing their exact curation attribution.

Discovery hints are not identity decisions. Unavailable npm/PyPI packages are recorded from their HTTP responses; neither channel performs registry-wide discovery.

Review unowned registry packages with strong or exact matching evidence:

```sh
cargo run -- suggest-registry-packages \
	--acquisitions var/acquired \
	--curations curations \
	--output var/registry-suggestions.json
```

The report includes project IDs, package names, executable declarations, matching evidence, and source URLs. It can contain competing suggestions. Check the upstream project, package identity, and commands before adding `[[package]]` entries; forks and republished packages can copy upstream URLs. The command does not edit curations or create projects. Regenerate providers after accepting attributions.

## Batch acquisition

Fetch and import all language channels without arguments:

```sh
just fetch-languages
just import-languages
just generate
```

Each fetch automatically creates a unique UTC-timestamped directory under `var/input/languages/` and prints its path. After every fetch stage succeeds, it atomically updates `var/input/languages/current`, a file containing that directory's absolute path. Imports read this selection once and use the same retained directory for all three channels. A failed fetch never replaces the previous selection; importing before the first completed fetch produces an explicit error. Earlier input directories are retained rather than overwritten.

The root `justfile` variable `languages_input` controls the storage location when a different input root is needed.

Bulk jobs use an optimized Rust build. Fetching is performed by curl through just recipes; Rust planning and import commands only read local files. The crates.io dump is large. The dump recipe extracts only the two required CSV tables and dump metadata, not download-history or other unrelated tables.

The batch imports replace `var/acquired/crates-io.jsonl`, `npm.jsonl`, and `pypi.jsonl` and their adjacent manifests. Existing distro acquisitions remain unchanged. Retain the original snapshot directories and selected wheels: acquisition validation verifies their content identities when generating providers or releases.

Each stage can also be run separately with explicit paths. These jobs do not change the automatic batch selection:

```sh
just fetch-crates-dump var/input/crates-next
just import-crates-dump var/input/crates-next

just fetch-npm var/input/npm-next
just import-npm var/input/npm-next

just fetch-pypi-metadata var/input/pypi-next
just fetch-pypi-wheels var/input/pypi-next
```

A metadata snapshot contains `metadata.json`, an explicit package-to-response plan, and `metadata.tsv`, its download list. Resume an interrupted metadata download without requesting completed responses again:

```sh
just resume-npm var/input/npm-next
just fetch-registry-plan pypi var/input/pypi-next/metadata.tsv
```

Use `import-npm --plan <snapshot>/metadata.json` or `import-pypi --plan <snapshot>/wheels.json` with the desired output and scope for individual snapshot imports. After switching from catalog-based PyPI discovery, create a fresh metadata plan to narrow acquisition to curated packages; existing digest-cached wheels remain reusable. Plans reference absolute local paths. Unplanned files in the response directory or wheel cache do not participate in import or its input digest.

PyPI's per-package fetch and directory import remain available for focused checks. Their imports mark the channel snapshot partial with `package-selection=explicit-packages`; they do not claim to cover all curated packages. `just demo` acquires curated npm packages and Black into a fresh directory under `var/demo/`, without replacing production inputs or output.

## npm refresh jobs

`just fetch-npm` and `just import-npm` form a standalone pair. Fetching creates a fresh UTC-timestamped directory under `var/input/npm/` and atomically writes its absolute path to `var/input/npm/current` only after downloading every planned response. Imports require that completed snapshot. Neither job reads the old per-package JSON files directly under `var/input/npm/`.

With an explicit snapshot path, the same recipes are used by `fetch-languages` and `import-languages`. Explicit fetches create a new directory and never change the standalone or language-batch selection. Use `just resume-npm <snapshot>` for interrupted downloads, then `just import-npm <snapshot>`. Resuming does not regenerate the plan or change the default selection. An optional second argument to `import-npm` specifies the acquisition output path.

Planning deduplicates explicit package names and requests `https://registry.npmjs.org/<encoded-package>/latest` once per package. The registry resolves the `latest` dist-tag and returns one release manifest, not the full packument or version history. No `_all_docs` scan, package-name search, or tarball download is performed. Fresh snapshots request each selected release again because tags and metadata can change; resumed snapshots reuse completed responses. Requests are sequential, compressed, and separated by a 0.1-second pause, with curl retry backoff and `Retry-After` handling.

Old catalog/packument snapshots are not compatible with release-manifest imports. Run a fresh `fetch-npm` or `fetch-languages` before importing after this change; existing retained inputs are not deleted.

## Release and wheel selection

crates.io uses semantic version ordering, excluding yanked and prerelease versions. A selected library-only release is observed without executable candidates; commands from an older release are not retained. Omit `--curations` when invoking `import-crates-dump` directly to import the whole dump instead of narrowing acquisition to curation hints.

npm follows the registry's `latest` dist-tag, not the numerically highest version. Imports record the exact version in the retained release manifest and verify its package name against the plan. Scoped identities retain their scope. Unsupported `directories.bin` declarations are recorded as failed extraction rather than library-only observations. A missing package or `latest` release is recorded as failed, not inferred to be unpublished. A release with no executable declarations is observed without commands, replacing any older commands.

PyPI uses PEP 440 ordering over non-yanked stable releases with distribution files. The order of the Simple API's `versions` and `files` arrays is irrelevant. Wheel selection prefers a platform-independent wheel, then smaller declared size, then filename for deterministic ties. A newer source-only release is recorded as unsupported; acquisition does not fall back to an older wheel's commands. All-wheel-variant coverage and source-distribution inspection are not implemented.

Wheel downloads are reused by SHA-256 under `var/cache/pypi/wheels/`. Imports verify the expected digest before reading the ZIP, then verify the normalized package name and selected version against its core metadata. Only the explicitly selected wheels are imported, so an old cached wheel cannot preserve removed commands. A corrupt cached wheel causes an explicit import failure; remove that file and rerun the wheel download job to repair it.

## Completeness and failures

npm release responses and PyPI project responses constitute retained, bounded, non-atomic scans, not point-in-time mirrors. npm publishes a fetch completion marker only after every planned request finishes; this includes retained HTTP 404/410 responses, which make the imported snapshot partial. PyPI journal reconciliation is not implemented.

HTTP 404 and 410 responses are retained with status sidecars and become failed package observations. Other HTTP errors, interrupted downloads, malformed metadata, missing planned responses, and digest mismatches fail the job. Unsupported PyPI releases and npm executable declarations carry a `reason` in the package observation's variant map. These failures make the snapshot partial; they are never represented as successful packages with no commands.

A complete npm/PyPI snapshot uses `package-selection=curated-packages` and means every package in the retained plan was observed. A complete crates.io curation-discovery snapshot means all selected inputs were processed, not that the entire registry or every possible relationship to a curated project has been acquired. Published providers remain limited to reviewed curations. Acquisition manifests retain scope and failure details outside the canonical four-field provider relation.
