# Language registry acquisition

## Scope

Language channels follow the same ownership boundary as distro channels: acquisition discovers packages and executable declarations; only explicit package attributions in `curations/` produce canonical providers. A matching package name or repository URL never changes ownership automatically.

The collectors read broad registry catalogs and retain metadata for packages plausibly related to existing curated projects:

- crates.io: Download the full database dump, extract the crate and version tables, and stream them to select relevant packages and their newest non-yanked stable releases
- npm: Scan the replication service's complete `_all_docs` catalog with inclusive keyset pagination, then download selected package packuments
- PyPI: Download the complete Simple API project list, then selected project responses and one wheel per supported current release

npm and PyPI do not provide a crates.io-style dump containing every executable declaration. Their bulk catalogs provide package names; npm packuments contain `bin` declarations, while PyPI requires static wheel inspection. Neither npm tarballs nor crates.io package artifacts are downloaded. No packages are installed, built, imported, or executed.

## Discovery and attribution

Discovery includes explicitly curated registry package names, project IDs, project names, repository basenames, and existing channel package names. Name hints ignore ASCII case and punctuation. Scoped npm discovery requires a scope corresponding to the project ID or repository owner, and a related package basename or `cli`; a matching basename under an unrelated scope is insufficient. Explicitly curated scoped packages are always included. The crates.io dump also permits discovery by normalized repository or homepage URL, including packages whose registry names differ from the project ID.

These are acquisition hints, not identity decisions. Name-only npm/PyPI catalogs cannot discover every differently named package from its repository URL. Add a verified native package name to the project's curations when necessary, then generate a new metadata plan. Planning fails if an explicitly curated npm/PyPI package is absent from the catalog rather than silently ignoring it.

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

Bulk jobs use an optimized Rust build. Fetching is performed by curl through just recipes; Rust planning and import commands only read local files. The crates.io dump and npm catalog are large. The dump recipe extracts only the two required CSV tables and dump metadata, not download-history or other unrelated tables.

The batch imports replace `var/acquired/crates-io.jsonl`, `npm.jsonl`, and `pypi.jsonl` and their adjacent manifests. Existing distro acquisitions remain unchanged. Retain the original snapshot directories and selected wheels: acquisition validation verifies their content identities when generating providers or releases.

Each stage can also be run separately with explicit paths. These jobs do not change the automatic batch selection:

```sh
just fetch-crates-dump var/input/crates-next
just import-crates-dump var/input/crates-next

just fetch-npm-index var/input/npm-index-next
just fetch-registry-metadata npm var/input/npm-index-next var/input/npm-next

just fetch-pypi-index var/input/pypi-index-next.json
just fetch-registry-metadata pypi var/input/pypi-index-next.json var/input/pypi-next
just fetch-pypi-wheels var/input/pypi-next
```

A metadata snapshot contains `metadata.json`, an explicit package-to-response plan, and `metadata.tsv`, its download list. Resume an interrupted metadata download without requesting completed responses again:

```sh
just fetch-registry-plan npm var/input/npm-next/metadata.tsv
just fetch-registry-plan pypi var/input/pypi-next/metadata.tsv
```

Use `import-npm --plan <snapshot>/metadata.json` or `import-pypi --plan <snapshot>/wheels.json` with the desired output and scope for individual snapshot imports. Plans reference absolute local paths. Unplanned files in the response directory or wheel cache do not participate in import or its input digest.

The original per-package fetch commands remain available for focused checks and `just demo`. Their directory/API imports mark the channel snapshot partial with `package-selection=explicit-packages`; they do not claim to cover all curated packages.

## Release and wheel selection

crates.io uses semantic version ordering, excluding yanked and prerelease versions. A selected library-only release is observed without executable candidates; commands from an older release are not retained. Omit `--curations` when invoking `import-crates-dump` directly to import the whole dump instead of narrowing acquisition to curation hints.

npm follows the packument's `latest` dist-tag, not the numerically highest version. Scoped identities retain their scope. Unsupported `directories.bin` declarations are recorded as failed extraction rather than library-only observations. Explicit unpublish metadata produces a removed package observation.

PyPI uses PEP 440 ordering over non-yanked stable releases with distribution files. The order of the Simple API's `versions` and `files` arrays is irrelevant. Wheel selection prefers a platform-independent wheel, then smaller declared size, then filename for deterministic ties. A newer source-only release is recorded as unsupported; acquisition does not fall back to an older wheel's commands. All-wheel-variant coverage and source-distribution inspection are not implemented.

Wheel downloads are reused by SHA-256 under `var/cache/pypi/wheels/`. Imports verify the expected digest before reading the ZIP, then verify the normalized package name and selected version against its core metadata. Only the explicitly selected wheels are imported, so an old cached wheel cannot preserve removed commands. A corrupt cached wheel causes an explicit import failure; remove that file and rerun the wheel download job to repair it.

## Completeness and failures

npm/PyPI catalogs and subsequent package responses constitute retained, bounded, non-atomic scans, not point-in-time mirrors. npm pagination retains each response and publishes a completion marker only after reaching the end. Incremental replication checkpoints and PyPI journal reconciliation are not implemented.

HTTP 404 and 410 responses are retained with status sidecars and become failed package observations. Other HTTP errors, interrupted downloads, malformed metadata, missing planned responses, and digest mismatches fail the job. Unsupported PyPI releases and npm executable declarations carry a `reason` in the package observation's variant map. These failures make the snapshot partial; they are never represented as successful packages with no commands.

A complete curation-discovery snapshot means all selected inputs were processed, not that the entire registry or every possible relationship to a curated project has been acquired. Published providers remain limited to reviewed curations. Acquisition manifests retain scope and failure details outside the canonical four-field provider relation.
