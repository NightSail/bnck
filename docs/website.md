# Website

## Purpose

The website is a reference interface to the [provider dataset](design.md), not a software directory or package installer. Its primary tasks are finding the projects and distribution packages behind an executable name, and finding the executables provided by a canonical project.

The visual hierarchy follows those tasks: search first, identities second, provider relations next, and release context alongside the results. A dark, terminal-informed theme uses graphite surfaces, restrained green and cyan accents, and monospace headings, controls, and exact dataset values. Explanatory prose remains proportional, while forms, tables, and navigation retain web-native behavior. There is no simulated terminal window, command session, or animated cursor. It does not require JavaScript, external fonts, or third-party services.

The header uses `bnck(1)` followed by `bin name check`, a plain monospace label in manual-page style. `(1)` denotes the manual section for executable programs and shell commands.

Public copy uses short, descriptive headings and instructions. It explains lookup behavior and relevant dataset limits, without slogans or implementation details.

## Views

- Home: Search entry point, an example record, dataset counts, and browse links
- Search: A combined index of executable names and project IDs, with explicit identity types, exact-match labels, relation counts, and pagination
- Executable: All projects providing the exact command name, each grouped with its channel packages; multiple project identities receive an ambiguity notice
- Project: All exact executable names attributed to the project, their channel packages, and curator-supplied project links
- Dataset: Field definitions, record counts by channel, coverage limits, release identifier, and generation date

Package links display the exact channel-native package name rather than the URL. They point to current external channel pages, not pinned acquisition evidence or installation instructions. Unknown package-page mappings remain plain text.

## Search contract

Search uses GET requests to `/` so results can be bookmarked and shared.

| Parameter | Meaning | Default |
| --- | --- | --- |
| `q` | Literal substring of an executable name or project ID; at most 100 Unicode characters after trimming surrounding whitespace | Absent shows the home page; empty browses the index |
| `kind` | `all`, `executable`, or `project` | `all` |
| `channel` | A supported channel key, or empty for all channels | Empty |
| `page` | Positive page number | `1` |

Search folds ASCII letter case, but does not normalize Unicode or interpret wildcards. Results are ordered by exact spelling, case-insensitive equality, prefix, then other substring matches. Shorter names rank first within each class, followed by exact name and identity type. Empty-name browsing sorts by name and identity type.

Each page contains at most 30 identities. The total counts all matching identities, not just the displayed page. Pagination preserves the query and filters; submitting the search form starts at page one. A page beyond the available results returns HTTP 404.

A channel filter applies to both matches and their displayed counts. Detail links deliberately open the full identity across all channels. Project names and executable names remain distinct results even when their spelling is identical. Search does not match package names or descriptions.

Detail routes retain exact, case-sensitive lookup semantics:

```text
/executables/{executable}
/projects/{project}
```

`/about` presents the dataset, retaining `#bnck-database` as the release metadata anchor.

## Truthfulness

All example records, statistics, and release metadata come from the opened SQLite artifact. The interface does not imply a live acquisition status, comprehensive channel coverage, historical availability, or installability.

A missing identity or relation means there is no accepted provider record in this release. Empty results and detail pages explain this limitation. Channel counts describe accepted dataset records, not acquisition completeness. The SQLite artifact does not contain channel snapshot scope or failure details; those remain in the [release manifests](release-format.md).

The dataset page identifies the release and generation date. Artifact schemas, digests, generator details, and SQL examples belong in the release documentation rather than the public pages. No artifact download endpoint is exposed by this SQLite-only server.

## Implementation

`src/web.rs` serves read-only SQLite queries through Axum and renders MiniJinja templates under `web/templates/`. Shared forms, statistics, provider tables, and snapshot notes live in `components.html`. The stylesheet is `web/style.css`.

SQLite query tasks run on the blocking thread pool, with a separate read-only connection per operation. Search values are bound SQL parameters. Responses retain HTML escaping, encoded name segments, and a restrictive content security policy.

Debug builds reload templates and CSS on each request. Release builds embed both, including shared templates. The server accepts compressed or uncompressed SQLite artifacts and does not modify the source artifact.

Forms have explicit labels, tables have captions and scoped headings, keyboard navigation has visible focus and a skip link, and the layout works down to a 320-pixel viewport. Search and pagination work with JavaScript disabled.
