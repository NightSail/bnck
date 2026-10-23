# Acquisition snapshot format

## Purpose

Each importer writes candidate JSON Lines and an adjacent snapshot manifest named `<candidates>.manifest.json`. Candidates hold executable observations; the manifest identifies the acquisition scope, inputs, package releases, completeness, and candidate artifact.

Separate package observations let bnck distinguish a package observed with no executable candidates from one that was absent or failed.

## Candidate records

Required fields:

- `channel`: Stable channel key
- `package`: Exact channel-native package identity
- `version`: Observed package release
- `executable`: Exact logical command name
- `evidence`: One of `declared-entry-point`, `repository-file-listing`, `static-recipe-action`, or `artifact-metadata`

Optional fields: `variant` map, `artifact_sha256`, and the project hints `source_package`, `homepage`, `repository`, and `description`. The release identity fields must match one observed package release in the adjacent manifest. Unknown fields are rejected.

Candidates are sorted and deduplicated as complete records. Observations from separate snapshots are not collapsed; dataset generation unions their executable names only after validating every snapshot.

## Snapshot manifest

```json
{
  "format": "bnck-channel-snapshot",
  "format_version": 1,
  "snapshot": "<content identity>",
  "channel": "debian",
  "status": "complete",
  "scope": {
    "architecture": "amd64",
    "component": "main",
    "suite": "stable"
  },
  "inputs": [
    {
      "path": "var/input/debian/Packages",
      "size": 123,
      "sha256": "<digest>"
    }
  ],
  "candidates": {
    "path": "debian.jsonl",
    "size": 456,
    "sha256": "<digest>",
    "candidate_count": 10
  },
  "packages": [
    {
      "name": "ripgrep",
      "version": "14.1.1-1",
      "variant": {
        "architecture": "amd64"
      },
      "artifact_sha256": "<digest>",
      "status": "observed"
    }
  ],
  "importer_version": "0.1.0",
  "importer_revision": "<git revision>"
}
```

The snapshot identity covers the channel, status, scope, input identities, candidate artifact, package observations, and importer identity. Manifest loading recomputes it and verifies local input and candidate sizes and digests.

`status` is `complete`, `partial`, or `failed`. Package status is `observed`, `failed`, or `removed`. An observed package requires a version; failed and removed packages may omit it.

Distro and bulk crates.io importers emit complete snapshots on success. Planned npm/PyPI acquisitions become partial with failed package observations and a `reason` in the variant map when selected metadata is unavailable or executable extraction is unsupported, and use `package-selection=curated-packages`. Focused PyPI directory imports are partial with `package-selection=explicit-packages`, because they do not cover all curated packages. Malformed inputs, missing planned files, and digest mismatches fail import. See [language registry acquisition](language-registries.md) for selection and failure semantics.

`scope` comes from repeated `--scope key=value` arguments; import commands reject an empty scope.

The schema supports input `uri` values, but current local import commands do not populate them. Retrieval timestamps, upstream signature results, and incremental tombstones remain collector responsibilities and are not emitted by the just-based fetch layer.

## Versioning

`format_version` is `1`. Increment it for changes to candidate or snapshot field names, types, requiredness, meaning, identity inputs, or the supported channel key set. Current readers reject unknown fields, so adding a field is also incompatible. The acquisition format version is independent of the provider, release, curation, and browser schemas.

## Generation rules

Dataset generation requires snapshot manifests and validates every one before resolving curations.

`bnck generate` and `bnck assemble-release` accept `--candidate-only`, which validates manifest identity, candidate artifact size and digest, candidate count, and package observations without reopening raw input files. `just generate` and `just release` enable this mode with `BNCK_CANDIDATE_ONLY=1`. Use it only when a separate acquisition job has already validated the raw inputs and only candidate artifacts are transferred.

For complete channel snapshots, a curated package absent from every observed package list is an error. For a partial acquisition, absence does not make curation stale and the package is skipped. An observed package with no candidates is valid and yields no providers unless curation adds an executable.
