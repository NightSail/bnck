# Dataset release format

## Purpose

This document defines the artifacts, schemas, versioning, provenance, integrity, and compatibility rules for bnck dataset releases. A release contains the canonical provider relations from [design.md](design.md), acquisition provenance from [acquisition-format.md](acquisition-format.md), and channel-specific behavior from [channels.md](channels.md).

## Release contents

Each published release contains:

```text
bnck-2026-03-24.1/
├── manifest.json
├── providers.jsonl.zst
├── bnck.sqlite.zst
├── LICENSE
├── SHA256SUMS
├── SHA256SUMS.minisig
└── channel-manifests/
    ├── debian-<snapshot>.json
    ├── debian.jsonl
    ├── fedora-<snapshot>.json
    ├── fedora.jsonl
    ├── arch-<snapshot>.json
    ├── arch.jsonl
    ├── homebrew-<snapshot>.json
    ├── homebrew.jsonl
    ├── crates-io-<snapshot>.json
    ├── crates-io.jsonl
    ├── npm-<snapshot>.json
    ├── npm.jsonl
    ├── pypi-<snapshot>.json
    └── pypi.jsonl
```

The two dataset artifacts represent the same provider relations:

- `providers.jsonl.zst`: Canonical, portable, streamable representation
- `bnck.sqlite.zst`: Derived representation optimized for the supported queries

The JSON Lines artifact defines the logical dataset identity; SQLite is the recommended artifact for applications.

Candidate JSON Lines files sit beside their channel manifests. Raw channel inputs are not distributed; the manifests record their identities. Input paths are relative to the repository root, and assembly rejects absolute or parent-relative paths. Snapshots containing such paths must be re-imported from the repository root.

Assembly writes these files except `SHA256SUMS.minisig`; signing adds the detached signature after assembly.

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
{"project":"ripgrep","executable":"rg","channel":"arch","package":"ripgrep"}
{"project":"ripgrep","executable":"rg","channel":"debian","package":"ripgrep"}
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

Schema version 1 represents the canonical relation, adds domain checks at the storage boundary, and includes curated project links for consumer interfaces:

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
        "release-policy": "latest-non-yanked-stable"
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

- Channel, status, and acquisition scope
- Input paths, sizes, and SHA-256 digests, with optional URIs
- Candidate artifact path, size, SHA-256 digest, and record count
- Package observations with status, release, variant, and artifact digest
- Importer version and optional source revision
- The snapshot content identity

Candidate artifacts are distributed beside the channel manifests. Raw channel inputs are not distributed. Current snapshots do not record retrieval times, signatures, media types, or original input URIs; the optional `uri` field is left empty.

Channel manifests provide acquisition provenance without adding package versions or evidence fields to canonical provider records.

A channel with incomplete acquisition has a non-`complete` status. A failed package must not be indistinguishable from a successfully observed package with no executables.

## Versioning

Schema version fields are independent. Each increments only for an incompatible change to its own contract. Schema transitions are recorded in [CHANGELOG.md](../CHANGELOG.md).

### Schema version fields

| Field | Location | Meaning | Current |
| --- | --- | --- | --- |
| `format_version` | `manifest.json` | Release manifest format | 1 |
| `dataset.schema_version` | `manifest.json` | Canonical provider schema | 1 |
| `dataset_schema_version` | SQLite `metadata` | Canonical provider schema | 1 |
| `schema_version` | SQLite `metadata` | SQLite schema | 1 |
| `user_version` | SQLite `PRAGMA` | SQLite schema | 1 |
| `schema_version` | SQLite artifact in `manifest.json` | SQLite schema | 1 |
| `format_version` | Acquisition snapshot manifest | Acquisition snapshot format | 1 |
| `schema` | Curation manifest | Curation format | 0 |
| `schema_version` | Browser dataset | Website data format | 1 |

`metadata.schema_version`, `PRAGMA user_version`, and the SQLite artifact's `schema_version` must agree. The provider schema version appears as `dataset.schema_version` in the release manifest and `dataset_schema_version` in SQLite metadata. Acquisition, curation, and browser versions are independent; see [acquisition-format.md](acquisition-format.md#versioning), [curations.md](curations.md#schema-version), and [website.md](website.md#implementation).

### Dataset release

Calendar versioning with a same-day sequence:

```text
2026-03-24.1
2026-03-24.2
2026-04-02.1
```

Adding or removing provider records is a data update, not a semantic API change, so semantic versioning does not fit. A published release is immutable; corrections produce a new release even on the same day. A mutable `latest` pointer may identify the recommended release but is not itself a version.

### Release manifest format

`format_version` is an integer beginning at `1`. Increment it when changing the type or meaning of a field, removing a required field, or changing how consumers must interpret the manifest. Adding optional fields is compatible because consumers ignore unknown fields. Provider record changes do not increment this version.

### Provider schema

`dataset.schema_version` and `metadata.dataset_schema_version` are integers beginning at `1`. Increment the provider schema when the canonical provider record shape, field meaning, channel key set, or canonical encoding changes in a way consumers must handle. Adding or removing provider records is a data update, not a schema change.

### SQLite schema

`metadata.schema_version`, `PRAGMA user_version`, and the SQLite artifact's `schema_version` are integers beginning at `1`. Increment the SQLite schema when consumers must account for changed tables, columns, indexes, constraints, or query behavior. Data-only updates do not change it. The initial schema is `1`; pre-publication development artifacts labeled `2` are not release-compatible.

### Acquisition snapshot format

The acquisition snapshot manifest has its own `format_version`, currently `1`. Increment it for changes to candidate or snapshot field names, types, requiredness, meaning, or identity inputs. Current readers reject unknown fields, so adding a field is also incompatible. See [acquisition-format.md](acquisition-format.md#versioning).

### Curation format

The curation manifest has its own `schema` value, currently `0`. Increment it for incompatible changes to curation fields or resolution semantics. Current readers reject unknown fields, so adding a field is also incompatible. See [curations.md](curations.md#schema-version).

### Browser dataset

The generated browser dataset has its own `schema_version`, currently `1`. Increment it when the project, link, or provider shape changes incompatibly. See [website.md](website.md#implementation).

### Dataset identity

The dataset identity is the SHA-256 digest of the uncompressed `providers.jsonl`. Stored fields use lowercase hexadecimal; the `sha256:` form is notation, not part of the stored value.

The CalVer release is a human-facing identifier; the digest is the content identity.

## Integrity and signing

`SHA256SUMS` covers:

- `manifest.json`
- `providers.jsonl.zst`
- `bnck.sqlite.zst`
- `LICENSE`
- Every included channel snapshot manifest and candidate artifact
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

The command validates canonical providers and all acquisition manifests, verifies that every provider channel has acquisition provenance, writes both compressed artifacts, copies content-identified channel manifests and candidate artifacts, writes `manifest.json`, and calculates `SHA256SUMS`. It refuses to overwrite an existing release directory.

The release is publishable only after `SHA256SUMS.minisig` has been produced with the separately managed signing key.
