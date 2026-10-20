# Dataset release format

## Purpose

This document defines the artifacts, schemas, versioning, provenance, integrity, and compatibility rules for bnck dataset releases. A release contains the canonical provider relations from [design.md](design.md) plus enough provenance to trace them to the channel snapshots described in [channels.md](channels.md).

## Release contents

Each immutable release contains:

```text
bnck-2026-03-24.1/
├── manifest.json
├── providers.jsonl.zst
├── bnck.sqlite.zst
├── SHA256SUMS
├── SHA256SUMS.minisig
└── channel-manifests/
    ├── debian-<snapshot>.json
    ├── fedora-<snapshot>.json
    ├── arch-<snapshot>.json
    ├── homebrew-<snapshot>.json
    ├── crates-io-<snapshot>.json
    ├── npm-<snapshot>.json
    └── pypi-<snapshot>.json
```

The two dataset artifacts represent the same provider relations:

- `providers.jsonl.zst`: Canonical, portable, streamable representation
- `bnck.sqlite.zst`: Derived representation optimized for the supported queries

The JSON Lines artifact defines the logical dataset identity; SQLite is the recommended artifact for applications.

## Provider record schema

Each provider record has exactly four fields:

```text
(project, executable, channel, package)
```

- `project`: Stable curation key for the canonical project
- `executable`: Exact logical command name, including case
- `channel`: Stable machine-readable distribution channel key
- `package`: Exact channel-native package identity

Package identities retain required channel qualification: scoped npm names keep their scope, and non-core Homebrew formulae keep their tap when needed to avoid ambiguity.

Observed package versions, architectures, artifact paths, and acquisition evidence describe how a relation was acquired, not the relation itself. They stay in channel snapshot manifests.

## Canonical JSON Lines

`providers.jsonl.zst` is the canonical representation. Uncompressed, it holds one JSON object per line:

```json
{"project":"helix","executable":"hx","channel":"crates.io","package":"helix-term"}
{"project":"helix","executable":"hx","channel":"debian","package":"helix"}
```

Encoding rules:

- UTF-8 without a byte-order mark
- One record per line, LF line endings, no blank lines
- Field order `project`, `executable`, `channel`, `package`
- Compact JSON without insignificant whitespace
- No duplicate records
- Bytewise record sorting by `project`, `executable`, `channel`, `package`
- A final LF after the last record

Strings use JSON escaping and otherwise preserve their exact dataset value; executable names are not case-folded or Unicode-normalized during serialization.

The SHA-256 digest of the uncompressed canonical JSON Lines is the logical dataset identity, independent of the Zstandard implementation and compression level.

JSON Lines supports streaming, bounded-memory processing, incremental validation, and straightforward conversion to other formats.

## SQLite

`bnck.sqlite.zst` is the primary consumer artifact. It uses only broadly supported SQLite features without extensions.

### Schema

Schema version 2 represents the canonical relation, adds domain checks at the storage boundary, and includes curated project links for consumer interfaces:

```sql
CREATE TABLE metadata (
	key TEXT PRIMARY KEY,
	value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE providers (
	project TEXT NOT NULL CHECK (length(project) > 0),
	executable TEXT NOT NULL CHECK (length(executable) > 0 AND instr(executable, '/') = 0),
	channel TEXT NOT NULL CHECK (channel IN ('arch', 'crates.io', 'debian', 'fedora', 'homebrew', 'npm', 'pypi')),
	package TEXT NOT NULL CHECK (length(package) > 0),

	PRIMARY KEY (
		project,
		executable,
		channel,
		package
	)
) WITHOUT ROWID;

CREATE INDEX providers_by_executable
ON providers (
	executable,
	project,
	channel,
	package
);

CREATE TABLE project_links (
	project TEXT NOT NULL,
	type TEXT NOT NULL CHECK (
		type IN ('homepage', 'repository', 'documentation', 'issue tracker', 'link')
	),
	url TEXT NOT NULL CHECK (length(url) > 0),
	PRIMARY KEY (project, type, url)
) WITHOUT ROWID;
```

The primary key supports project lookup and prevents duplicate provider records; the covering secondary index supports executable lookup without another table access.

`project_links` contains only curator-supplied HTTP or HTTPS URLs. Its primary key supports project lookup; no further index is required.

No package index exists initially because package lookup is not a supported core query. A later schema version can add one if a supported query requires it.

### Query by executable

```sql
SELECT project, channel, package
FROM providers
WHERE executable = ?
ORDER BY project, channel, package;
```

### Query by project

```sql
SELECT executable, channel, package
FROM providers
WHERE project = ?
ORDER BY executable, channel, package;
```

### Query project links

```sql
SELECT type, url
FROM project_links
WHERE project = ?
ORDER BY type, url;
```

### Metadata

The `metadata` table contains:

- `schema_version`: SQLite schema version
- `dataset_schema_version`: Logical provider schema version
- `dataset_release`
- `dataset_sha256`
- `provider_count`
- `generated_at`
- `generator_version`
- `generator_revision`

`PRAGMA user_version` contains the same numeric SQLite schema version as `metadata.schema_version`.

SQLite is derived from the canonical JSON Lines. Consumers must not treat page layout, row order without `ORDER BY`, or internal row identifiers as part of the release contract.

## Release manifest

`manifest.json` describes the release and its provenance; it does not repeat provider records.

```json
{
  "format": "bnck-dataset-release",
  "format_version": 1,
  "release": "2026-03-24.1",
  "generated_at": "2026-03-24T18:30:00Z",
  "dataset": {
    "schema_version": 1,
    "sha256": "<uncompressed providers.jsonl SHA-256>",
    "provider_count": 123456
  },
  "generator": {
    "version": "0.1.0",
    "source_revision": "<git revision>"
  },
  "artifacts": [
    {
      "name": "providers.jsonl.zst",
      "media_type": "application/x-ndjson",
      "compression": "zstd",
      "sha256": "<compressed artifact SHA-256>",
      "size": 1234567
    },
    {
      "name": "bnck.sqlite.zst",
      "media_type": "application/vnd.sqlite3",
      "compression": "zstd",
      "schema_version": 1,
      "sha256": "<compressed artifact SHA-256>",
      "size": 2345678
    }
  ],
  "channels": [
    {
      "name": "crates.io",
      "scope": {
        "release_policy": "latest-non-yanked"
      },
      "snapshot_manifest": {
        "path": "channel-manifests/crates-io-<snapshot>.json",
        "sha256": "<channel manifest SHA-256>"
      },
      "package_count": 12345,
      "provider_count": 2345,
      "status": "complete"
    }
  ]
}
```

Required top-level fields:

- `format`: Constant identifying a bnck dataset release manifest
- `format_version`: Integer version of the manifest format
- `release`: Immutable dataset release identifier
- `generated_at`: UTC release-generation timestamp in RFC 3339 form
- `dataset`: Logical schema version, content digest, and record count
- `generator`: Generator version and source revision
- `artifacts`: Released artifact identities, formats, sizes, and digests
- `channels`: Scope, provenance, counts, and acquisition status per channel snapshot; a channel may appear more than once when independently acquired scopes are combined

Consumers ignore unknown fields under a known `format_version`. Removing a required field or changing the meaning or type of an existing field requires a new format version.

## Channel snapshot manifests

Each channel entry references a content-addressed snapshot manifest defined in [acquisition-format.md](acquisition-format.md), which records:

- Channel and acquisition scope
- Channel-native snapshot boundary
- Retrieval time
- Selected release policy
- Input URIs, sizes, and cryptographic digests
- Signature verification results where applicable
- Selected package releases and variants
- Importer source revision and configuration
- Failed, missing, unsupported, or deliberately excluded inputs

Channel manifests provide acquisition provenance without adding package versions or evidence fields to canonical provider records.

A channel with incomplete acquisition has a non-`complete` status. A failed package must not be indistinguishable from a successfully observed package with no executables.

## Versioning

Dataset content and format contracts use separate versions.

### Dataset release

Calendar versioning with a same-day sequence:

```text
2026-03-24.1
2026-03-24.2
2026-04-02.1
```

Adding or removing provider records is a data update, not a semantic API change, so semantic versioning does not fit. A published release is immutable; corrections produce a new release even on the same day. A mutable `latest` pointer may identify the recommended release but is not itself a version.

### Manifest format

`format_version` is an integer beginning at `1`. Increment it for incompatible changes to `manifest.json`.

### Provider schema

`dataset.schema_version` is an integer beginning at `1`. Increment it when the logical provider representation changes incompatibly.

### SQLite schema

The SQLite schema has its own integer version, incremented whenever consumers must account for changed tables, columns, constraints, or query behavior.

### Dataset identity

```text
sha256:<digest of uncompressed providers.jsonl>
```

The CalVer release is a human-facing identifier; the digest is the content identity.

## Integrity and signing

`SHA256SUMS` covers:

- `manifest.json`
- `providers.jsonl.zst`
- `bnck.sqlite.zst`
- Every included channel snapshot manifest
- Any notices or additional release artifacts

`SHA256SUMS.minisig` is a detached signature over `SHA256SUMS`. Distribute the public key independently of the release location. The `assemble-release` command writes every release artifact and `SHA256SUMS`; signing remains a separate publication step because release keys are not exposed to the generator.

The release manifest also records artifact digests for validation after trusting the manifest, and the logical dataset digest provides a second check after decompressing `providers.jsonl.zst`.

A transparency-log or Sigstore attestation can be added later without changing the provider schema.

## Stability rules

### Project keys

A project key is a stable curation identifier, not a display name or a repository URL derived at release time. It does not change because a project moves repositories or changes branding.

Aliases or replacements, if needed, are modeled explicitly in a future schema rather than silently changing identity.

### Executable names

Executable names preserve exact spelling and case. The dataset does not add platform wrapper suffixes such as `.exe`, `.cmd`, or `.ps1` when the channel declares a platform-independent logical command name.

### Channel keys

Channel keys are stable machine identifiers:

```text
debian
fedora
arch
homebrew
crates.io
npm
pypi
```

Changing a display label does not change the key.

### Package names

Package names preserve channel-native identity and case rules. Interpret a package name together with its channel.

## Excluded data

The canonical provider artifacts exclude:

- Historical observations
- Package release history
- Download counts or popularity
- Full project descriptions
- Package source and artifact URLs
- Acquisition confidence or evidence
- Failed acquisition details
- Channel-native aliases

These belong in channel manifests, curation sources, or future optional artifacts.

The derived SQLite artifact includes curated project homepages, repositories, documentation, issue trackers, and additional links. They support consumer interfaces but are not provider records and do not affect the canonical dataset identity.

## Additional formats

Derived artifacts may be introduced without changing the canonical provider schema. Candidate options include Parquet for bulk analytics, delimited text, a single JSON document, a custom binary format, and delta releases between adjacent versions. None is required today: the two supported lookup directions need no more than SQLite, and complete JSON Lines snapshots suffice while update size is small. Any canonical-format change must preserve streaming and exact content identity.

## Assembly

```sh
just release 2026-04-16.1
```

The command validates canonical providers and all acquisition manifests, verifies that every provider channel has acquisition provenance, writes both compressed artifacts, copies content-identified channel manifests, writes `manifest.json`, and calculates `SHA256SUMS`. It refuses to overwrite an existing release directory.

The release is publishable only after `SHA256SUMS.minisig` has been produced with the separately managed signing key.
