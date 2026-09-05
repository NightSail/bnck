# Website

## Purpose

The website is a reference interface to the [provider dataset](design.md), not a software directory or package installer. Its primary tasks are finding the projects and distribution packages behind an executable name, and finding the executables provided by a canonical project.

The visual hierarchy follows those tasks: search first, identities second, provider relations next, and release context alongside the results. A dark, terminal-informed theme uses graphite surfaces, restrained green and cyan accents, and monospace headings, controls, and exact dataset values. Explanatory prose remains proportional, while forms, tables, and navigation retain web-native behavior. There is no simulated terminal window, command session, or animated cursor. It does not require JavaScript, external fonts, or third-party services.

The header uses `bnck(1)` followed by `bin name check`, a plain monospace label in manual-page style. `(1)` denotes the manual section for executable programs and shell commands.

Public copy uses short, descriptive headings and instructions. It explains lookup behavior and relevant dataset limits, without slogans or implementation details.

## Views

- Home: Search entry point, an example record, dataset counts, and browse links
- Search: Executable names and project IDs grouped by owning project, with executable links, exact-match labels, relationship summaries, and pagination; type-specific searches retain flat indexes
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

Search folds ASCII letter case, but does not normalize Unicode or interpret wildcards. Matches rank by exact spelling, case-insensitive equality, prefix, then other substring matches. Shorter matching names rank first within each class. “All types” ranks each project by its best matching project ID or executable name, breaking ties by project ID. Type-specific searches break ties by exact name. Empty-name browsing sorts by project ID or executable name, depending on the selected type.

“All types” returns one group per owning project, whether the query matches its ID or one of its executable names. Ownership comes from accepted provider records, never a shared prefix or an inferred main executable. Independently owned ecosystem tools remain separate projects. A command provided by multiple projects appears under each matching owner, with executable links opening the corresponding section on that owner's project page.

Each project group has an underlined project heading linking to project details. It previews up to three executable rows, each showing an underlined `name(1)` link followed by the distinct channels providing that executable within the project. Additional rows use the same format inside a native “X more executables” disclosure that works without JavaScript. Executables are ordered by match quality and then length and spelling, with nonmatches last; browsing orders them by exact name. Exact executable matches are labelled and remain in the preview even for large projects.

Each page contains at most 30 projects in “All types” and project-only searches, or 30 distinct exact-spelling names in executable-only searches. Totals count the corresponding groups across all pages. A project's executable list is never split across pages. Case variants of executable names remain distinct. Pagination preserves the query and filters; submitting the search form starts at page one. A page beyond the available results returns HTTP 404.

Type-specific searches retain single-purpose tables and match only the selected identity type. Their relationship summaries link the actual related names when there are at most three, otherwise they show the relationship count. A channel filter applies to matches, group membership, counts, and executable lists. Project counts include all of the project's records in the selected channels, not only records for matching executable names. Detail links deliberately open the full identity across all channels. Search does not match package names or descriptions.

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
