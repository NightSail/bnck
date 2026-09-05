# Dataset release format

## Purpose

This document defines the artifacts, schemas, versioning, provenance, integrity, and compatibility rules for bnck dataset releases. A release contains the canonical provider relations described in [design.md](design.md), plus enough provenance to trace them to the channel snapshots described in [channels.md](channels.md).

## Release contents

Each immutable release should contain:

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

The JSON Lines artifact defines the logical dataset identity. SQLite is the recommended artifact for applications.

## Provider record schema

Each provider record has exactly four fields:

```text
(project, executable, channel, package)
```

Their meanings are:

- `project`: Stable curation key for the canonical project
- `executable`: Exact logical command name, including case
- `channel`: Stable machine-readable distribution channel key
- `package`: Exact channel-native package identity

Package identities retain required channel qualification. For example, scoped npm names retain their scope and non-core Homebrew formulae retain their tap when needed to avoid ambiguity.

Observed package versions, architectures, artifact paths, and acquisition evidence do not belong in provider records. They describe how a relation was acquired, not the canonical relation itself, and remain in channel snapshot manifests.

## Canonical JSON Lines

`providers.jsonl.zst` is the canonical representation. Its uncompressed content uses one JSON object per line:

```json
{"project":"helix","executable":"hx","channel":"crates.io","package":"helix-term"}
{"project":"helix","executable":"hx","channel":"debian","package":"helix"}
```

The encoding rules are:

- UTF-8 without a byte-order mark
- One record per line
- LF line endings
- No blank lines
- Field order of `project`, `executable`, `channel`, then `package`
- Compact JSON without insignificant whitespace
- No duplicate records
- Bytewise record sorting by `project`, `executable`, `channel`, then `package`
- A final LF after the last record

Strings use JSON escaping and otherwise preserve their exact dataset value. In particular, executable names are not case-folded or Unicode-normalized during serialization.

The SHA-256 digest of the uncompressed canonical JSON Lines content is the logical dataset identity. Hashing the uncompressed representation makes this identity independent of the Zstandard implementation and compression level.

JSON Lines is preferred over a single JSON array because it supports streaming, bounded-memory processing, incremental validation, and straightforward conversion to other formats.

## SQLite

`bnck.sqlite.zst` is the primary consumer artifact. It should use only broadly supported SQLite features and should not require extensions.

### Schema

SQLite schema version 2 represents the canonical relation, adds domain checks at the storage boundary, and includes curated project links for consumer interfaces:

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

The primary key supports project lookup and prevents duplicate provider records. The covering secondary index supports executable lookup without another table access.

The `project_links` table contains only curator-supplied HTTP or HTTPS URLs. Its primary key supports project lookup; no additional index is required.

No package index is required initially because package lookup is not one of the supported core queries. It can be added in a later schema version if a supported query requires it.

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

`PRAGMA user_version` should contain the same numeric SQLite schema version as `metadata.schema_version`.

The SQLite artifact is derived from the canonical JSON Lines artifact. Consumers must not treat SQLite page layout, row order without `ORDER BY`, or internal row identifiers as part of the release contract.

## Release manifest

`manifest.json` describes the release and its provenance. It does not repeat provider records.

A representative manifest is:

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

### Required top-level fields

- `format`: Constant identifying a bnck dataset release manifest
- `format_version`: Integer version of the manifest format
- `release`: Human-facing immutable dataset release identifier
- `generated_at`: UTC release-generation timestamp in RFC 3339 form
- `dataset`: Logical schema version, content digest, and record count
- `generator`: Generator version and source revision
- `artifacts`: Released artifact identities, formats, sizes, and digests
- `channels`: Scope, provenance, counts, and acquisition status for each channel snapshot; a channel may appear more than once when independently acquired scopes are combined

Manifest consumers should ignore unknown fields under a known `format_version`. Removing a required field or changing the meaning or type of an existing field requires a new format version.

## Channel snapshot manifests

Each channel entry references a content-addressed channel snapshot manifest defined in [acquisition-format.md](acquisition-format.md). That manifest records:

- Channel and acquisition scope
- Channel-native snapshot boundary
- Retrieval time
- Selected release policy
- Input URIs, sizes, and cryptographic digests
- Signature verification results where applicable
- Selected package releases and variants
- Importer source revision and configuration
- Failed, missing, unsupported, or deliberately excluded inputs

Channel manifests provide acquisition provenance without adding package versions or evidence fields to every canonical provider record.

A channel with incomplete acquisition should have a non-`complete` status in the release manifest. A failed package must not be indistinguishable from a successfully observed package with no executables.

## Versioning

Dataset content and format contracts use separate versions.

### Dataset release

Use calendar versioning with a same-day sequence:

```text
2026-03-24.1
2026-03-24.2
2026-04-02.1
```

Adding or removing provider records is a data update, not a semantic API change. Semantic versioning therefore does not describe dataset releases well.

A published release is immutable. Corrections produce a new release even when they occur on the same day. A mutable `latest` pointer may identify the recommended release but is not itself a version.

### Manifest format

`format_version` is an integer beginning at `1`. Increment it for incompatible changes to `manifest.json`.

### Provider schema

`dataset.schema_version` is an integer beginning at `1`. Increment it when the logical provider representation changes incompatibly.

### SQLite schema

The SQLite schema has its own integer version. Increment it whenever consumers must account for changed tables, columns, constraints, or query behavior.

### Dataset identity

The exact dataset identity is:

```text
sha256:<digest of uncompressed providers.jsonl>
```

The CalVer release is a human-facing identifier. The digest is the content identity.

## Integrity and signing

`SHA256SUMS` should cover:

- `manifest.json`
- `providers.jsonl.zst`
- `bnck.sqlite.zst`
- Every included channel snapshot manifest
- Any notices or additional release artifacts

`SHA256SUMS.minisig` is a detached signature over `SHA256SUMS`. The corresponding public key must be distributed independently of the release location. The `assemble-release` command writes every release artifact and `SHA256SUMS`; signing remains a separate publication step because release keys are not exposed to the generator.

The release manifest also records artifact digests so clients can validate an artifact after trusting the manifest. The logical dataset digest provides a second check after decompressing `providers.jsonl.zst`.

A transparency-log or Sigstore attestation can be added later without changing the provider schema.

## Stability rules

### Project keys

A project key is a stable curation identifier, not a display name or a repository URL derived at release time. Project keys should not change merely because a project moves repositories or changes branding.

If project-key aliases or replacements become necessary, they should be modeled explicitly in a future schema rather than silently changing identity.

### Executable names

Executable names preserve exact spelling and case. The dataset does not add platform wrapper suffixes such as `.exe`, `.cmd`, or `.ps1` when the channel declares a platform-independent logical command name.

### Channel keys

Channel keys are stable machine identifiers. Initial values are:

```text
debian
fedora
arch
homebrew
crates.io
npm
pypi
```

Changing a display label does not change the channel key.

### Package names

Package names preserve channel-native identity and case rules. Consumers must interpret a package name together with its channel.

## Excluded data

The canonical provider artifacts do not include:

- Historical observations
- Package release history
- Download counts or popularity
- Full project descriptions
- Package source and artifact URLs
- Acquisition confidence or evidence
- Failed acquisition details
- Channel-native aliases

These belong in channel manifests, curation sources, or future optional artifacts. Keeping them out of the provider relation preserves the small query-oriented data model.

The derived SQLite artifact includes curated project homepages, repositories, documentation, issue trackers, and additional links. These links support consumer interfaces but are not provider records and do not affect the canonical provider dataset identity.

## Additional formats

Additional derived artifacts may be introduced without changing the canonical provider schema.

### Parquet

Parquet may be useful for bulk analytics, but it is not required for the two supported lookup directions and is less widely embedded than SQLite.

### CSV or TSV

Delimited text is compact but introduces escaping and parser-compatibility concerns. JSON Lines is a safer canonical interchange format.

### Single JSON document

One large JSON array requires whole-document parsing and is less suitable for streaming.

### Custom binary format

A custom format may be smaller, but it would add implementation and compatibility costs before there is evidence that SQLite is insufficient.

### Delta releases

Additions and removals between adjacent releases could later be published as separate JSON Lines artifacts. Initial releases should remain complete snapshots until update size demonstrates that deltas are necessary.

## Assembly

Run the implemented release assembler with:

```sh
just release 2026-04-16.1
```

The command validates canonical providers and all acquisition manifests, verifies that every provider channel has acquisition provenance, writes both compressed artifacts, copies content-identified channel manifests, writes `manifest.json`, and calculates `SHA256SUMS`. It refuses to overwrite an existing release directory.

The release is publishable only after `SHA256SUMS.minisig` has been produced with the separately managed signing key.

## Initial contract

The initial release contract is:

- Canonical content: `providers.jsonl.zst`
- Consumer artifact: `bnck.sqlite.zst`
- Release metadata: `manifest.json`
- Provenance: Content-addressed channel snapshot manifests
- Human version: CalVer with a same-day sequence
- Content identity: SHA-256 of uncompressed canonical JSON Lines
- Integrity: Signed SHA-256 checksum list

This format keeps the public dataset small and query-oriented while preserving exact content identity and channel-level provenance.
