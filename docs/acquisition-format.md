# Acquisition snapshot format

## Purpose

Each importer writes two files: canonical candidate JSON Lines and an adjacent snapshot manifest named `<candidates>.manifest.json`. The candidate file contains executable observations. The manifest identifies the acquisition scope, inputs, package releases, completeness, and candidate artifact.

Keeping package observations separate from executable candidates allows bnck to distinguish a package that was observed with no executable candidates from a package that was absent or failed.

## Candidate records

A candidate record has these required fields:

- `channel`: Stable channel key
- `package`: Exact channel-native package identity
- `version`: Observed package release
- `executable`: Exact logical command name
- `evidence`: One of `declared-entry-point`, `repository-file-listing`, `static-recipe-action`, or `artifact-metadata`

It may contain a `variant` map, `artifact_sha256`, and the project hints `source_package`, `homepage`, `repository`, and `description`. The release identity fields on a candidate match one observed package release in the adjacent manifest. Unknown fields are rejected.

Candidates are sorted and deduplicated as complete records. Observations from separate snapshot files are not collapsed. Dataset generation unions their executable names only after validating every snapshot.

## Snapshot manifest

A snapshot manifest has this shape:

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

The snapshot identity covers the channel, status, scope, input identities, candidate artifact, package observations, and importer identity. Manifest loading recomputes that identity and verifies local input and candidate sizes and SHA-256 digests.

`status` is one of `complete`, `partial`, or `failed`. Package status is one of `observed`, `failed`, or `removed`. An observed package requires a version; failed and removed package records may omit it. Distro and bulk crates.io importers emit complete snapshots when import succeeds. Planned npm/PyPI acquisitions become partial when selected metadata is unavailable or executable extraction is unsupported, with failed package observations and a `reason` in the variant map. Planned npm imports use `package-selection=curated-packages`; planned PyPI imports use `package-selection=curation-discovery`. Focused PyPI directory imports are partial with `package-selection=explicit-packages`, because they do not cover all curated packages. Malformed inputs, missing planned files, and digest mismatches fail import. See [language registry acquisition](language-registries.md) for selection and failure semantics.

`scope` is supplied through repeated `--scope key=value` arguments. Import commands reject an empty scope.

Input `uri` values are supported by the schema but current local import commands do not populate them. Retrieval timestamps, upstream signature results, and incremental tombstone production remain collector responsibilities and are not yet emitted by the just-based fetch layer.

## Generation rules

Dataset generation requires snapshot manifests. It validates every manifest before resolving curations.

For complete channel snapshots, a curated package absent from every observed package list is an error. For a partial channel acquisition, absence does not make curation stale and the package is skipped. An observed package with no candidate records is valid and resolves to no providers unless curation adds an executable explicitly.
