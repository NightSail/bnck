# Acquisition snapshot format

## Purpose

Each importer writes candidate JSON Lines and an adjacent snapshot manifest named `<candidates>.manifest.json`. Candidates hold executable observations; the manifest identifies the acquisition scope, channel boundary, inputs and retrieval provenance, package releases, completeness, candidate artifact, and acquisition metrics.

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
  "format_version": 2,
  "snapshot": "<content identity>",
  "channel": "debian",
  "status": "complete",
  "scope": {
    "architecture": "amd64",
    "component": "main",
    "suite": "stable"
  },
  "boundary": {
    "release-date": "Sat, 12 Sep 2026 07:55:41 UTC"
  },
  "inputs": [
    {
      "path": "var/input/debian/InRelease",
      "size": 140421,
      "sha256": "<digest>",
      "uri": "https://deb.debian.org/debian/dists/stable/InRelease",
      "media_type": "application/pgp-signature",
      "retrieved_at": "2026-10-23T00:00:00Z",
      "signature": {
        "kind": "openpgp",
        "result": "verified",
        "signer": "<fingerprint>"
      }
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
  "metrics": {
    "input_count": 1,
    "input_bytes": 140421,
    "observed_packages": 68820,
    "failed_packages": 0,
    "removed_packages": 0,
    "candidate_count": 47342,
    "candidate_bytes": 18421958
  },
  "importer_version": "0.1.0",
  "importer_revision": "<git revision>"
}
```

Input fields:

- `path`, `size`, and `sha256` identify local bytes
- `uri`, `media_type`, `retrieved_at`, and `signature` record retrieval provenance
- `signature.result` is `verified`, `failed`, or `not-available`; `signer` is optional
- Inputs without a remote origin, such as decompressed indexes, omit `uri`

`boundary` records channel-native state such as a Debian release date, Fedora `repomd.xml` revision, Homebrew tap commit, or PyPI serial. Boundaries are strings because channel representations differ.

`metrics` is derived from the other manifest content and validated on load. Snapshot failures use `status: "failed"` and a non-empty `failure` message; complete and partial snapshots omit `failure`.

The snapshot identity covers the channel, status, scope, input content identities, candidate artifact, package observations, and importer identity. Manifest loading recomputes it and verifies local input and candidate sizes and digests. Retrieval timestamps, URIs, media types, signatures, boundaries, metrics, and failure messages describe the acquisition event and are not part of the content identity.

`status` is `complete`, `partial`, or `failed`. Package status is `observed`, `failed`, or `removed`. An observed package requires a version; failed and removed packages may omit it.

Distro and bulk crates.io importers emit complete snapshots on success. Planned npm/PyPI acquisitions become partial with failed package observations and a `reason` in the variant map when selected metadata is unavailable or executable extraction is unsupported, and use `package-selection=curated-packages`. Focused PyPI directory imports are partial with `package-selection=explicit-packages`, because they do not cover all curated packages. Malformed inputs, missing planned files, and digest mismatches fail import. See [language registry acquisition](language-registries.md) for selection and failure semantics.

A hard import failure still writes an empty candidate artifact and a `failed` snapshot with the error summary, so acquisition failure is distinguishable from an absent channel. Package removals are emitted as `removed` observations when an import compares against a preceding complete snapshot for the same channel and scope with `--previous`; partial snapshots never infer removals.

`scope` comes from repeated `--scope key=value` arguments; import commands reject an empty scope.

### Acquisition context

Fetch jobs pass provenance to import commands with `--context <path>`. The context is a tab-separated file with the header `bnck-acquisition-context<TAB>1` and two record kinds:

```text
input<TAB>path<TAB>uri<TAB>media_type<TAB>retrieved_at<TAB>signature_kind<TAB>signature_result<TAB>signer
boundary<TAB>key<TAB>value
```

Empty optional fields are permitted. Import commands fall back to file modification times when an input has no `retrieved_at` value. Context entries are matched to manifest inputs by absolute path; entries not used by the importer are ignored.

## Versioning

`format_version` is `2`. Increment it for changes to candidate or snapshot field names, types, requiredness, meaning, identity inputs, or the supported channel key set. Current readers reject unknown fields, so adding a field is also incompatible. The acquisition format version is independent of the provider, name association, release, curation, and browser schemas.

## Generation rules

Dataset generation requires snapshot manifests and validates every one before resolving curations.

`bnck generate` and `bnck assemble-release` accept `--candidate-only`, which validates manifest identity, candidate artifact size and digest, candidate count, and package observations without reopening raw input files. `just generate` and `just release` enable this mode with `BNCK_CANDIDATE_ONLY=1`. Use it only when a separate acquisition job has already validated the raw inputs and only candidate artifacts are transferred.

For complete channel snapshots, a curated package absent from every observed package list is an error unless a `removed` observation explicitly records its absence. For a partial acquisition, absence does not make curation stale and the package is skipped. An observed package with no candidates is valid and yields no providers unless curation adds an executable.
