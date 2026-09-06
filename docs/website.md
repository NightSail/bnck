# Website

## Purpose

The website is a reference interface to the [provider dataset](design.md), not a software directory or package installer. Its primary tasks are finding the projects and distribution packages behind an executable name, and finding the executables provided by a canonical project.

The visual hierarchy follows those tasks: search first, identities second, provider relations next, and release context alongside the results. A dark, terminal-informed theme uses graphite surfaces, restrained green and cyan accents, and monospace headings, controls, and exact dataset values. Explanatory prose remains proportional, while forms, tables, and navigation retain web-native behavior. There is no simulated terminal window, command session, or animated cursor. Search, filtering, and pagination use vanilla JavaScript over a locally loaded dataset. Identity pages, dataset information, and JSON viewing are static and work without JavaScript; clipboard copying is an optional enhancement. The site uses no external fonts or third-party services.

The header uses `bnck(1)` followed by `bin name check`, a plain monospace label in manual-page style. `(1)` denotes the manual section for executable programs and shell commands.

Public copy uses short, descriptive headings and instructions. It explains lookup behavior and relevant dataset limits, without slogans or implementation details. Provider-record terminology and counts are not shown; summaries use executable, project, channel, and package information.

## Views

- Home: Search entry point, an example lookup, dataset counts, and browse links
- Search: Executable names and project IDs grouped by owning project, with highlighted matches, executable links, per-executable channels, and pagination
- Executable: All projects providing the exact command name, each grouped with its channel packages; multiple project identities receive an ambiguity notice
- Project: A coverage matrix of exact executable names and channel packages, plus curator-supplied project links
- Dataset: Field definitions, project and package counts by channel, coverage limits, release identifier, and generation date

Project pages use one coverage matrix, with executable names as rows and packages as columns grouped under channel headings. Each channel/package pair appears once; marks identify executables observed in each package, while dots mean not observed in this dataset, not unavailability. Executable rows preserve the anchors used by search links and link to executable details. Rows and channel/package columns sort by exact name. The matrix scrolls horizontally with keyboard access on narrow screens, keeping executable names visible.

Package links display the exact channel-native package name rather than the URL. They point to current external channel pages, not pinned acquisition evidence or installation instructions. Unknown package-page mappings remain plain text.

## Search contract

Search uses GET-style query URLs at `/` so results can be bookmarked and shared. The static home page reads those parameters and computes results in the browser. On the home/search page, forms, filter links, and pagination update browser history without reloading the document; Back/Forward navigation restores the URL's search state. Modified clicks and links to other pages retain native browser behavior.

| Parameter | Meaning | Default |
| --- | --- | --- |
| `q` | Literal substring of an executable name or project ID; at most 100 Unicode characters after trimming surrounding whitespace | Absent shows the home page; empty browses the index |
| `channel` | A supported channel key, or empty for all channels | Empty |
| `initial` | `A`–`Z` for the first character of the project ID, or `#` for numbers and symbols | Empty shows all initials |
| `page` | Positive page number | `1` |

Search folds ASCII letter case, but does not normalize Unicode or interpret wildcards. Matches rank by exact spelling, case-insensitive equality, prefix, then other substring matches. Shorter matching names rank first within each class. Each project ranks by its best matching project ID or executable name, breaking ties by project ID. Empty-name browsing sorts by project ID.

An inline “Exact match” tag identifies each project ID or executable name that equals the trimmed query, preserving case and literal spelling. Project tags follow the project name; executable tags appear at the end of the row, after any displayed channels. Exact matches rank first among projects and within each project's executable list. Shared executable names receive a tag under each matching owner; a project and its executable can both be tagged. Partial and case-insensitive-only matches are not tagged. Tags follow the active search filters and do not appear when browsing.

Search returns one group per owning project, whether the query matches its ID or one of its executable names. Ownership comes from accepted provider records, never a shared prefix or an inferred main executable. Independently owned ecosystem tools remain separate projects. A command provided by multiple projects appears under each matching owner, with executable links opening the corresponding section on that owner's project page.

Results use a minimal grouped layout without column headers, type labels, arrows, relationship prefixes, or channel counts. Each group shows an underlined project heading linking to project details, followed by executable rows. Each row has an underlined `name(1)` link. Without a channel filter, rows also show the distinct channels for that project/executable pair. When a channel filter is selected, both preview and expanded rows omit the channel list and its colon; the active channel remains visible above the results. Executable links open the executable's section on the owning project's page.

Matching substrings in project and executable names are highlighted with semantic `mark` elements, preserving original spelling and HTML escaping. Highlighting uses the same literal, ASCII-case-insensitive matching as search, including repeated occurrences. Empty queries do not produce highlights.

Each group previews up to three executable rows. Additional rows use the same format inside a native “N more executables” disclosure that works without JavaScript. Executables are ordered by match quality and then length and spelling, with nonmatches last; exact executable matches remain in the preview even for large projects. Empty-name browsing orders executable rows by exact name.

An initial filter above the results offers “All” and only the initials present in projects matching the current query and channel. Letters are shown as A–Z; `#` groups names starting with numbers or symbols. Available initials are computed before initial filtering and pagination, so selecting one does not hide the other choices. Selecting an initial preserves the query and channel and starts at page one; “All” clears only the initial filter. Refining the query preserves the selected initial.

Each page contains at most 30 projects. Totals count matching projects across all pages. A project's executable list is never split across pages. Case variants of executable names remain distinct. Pagination preserves the query and filters; submitting the search form starts at page one. A page beyond the available results displays an explicit error, not an empty match list. Invalid query parameters also display an error. Static hosting returns the same root HTML regardless of query parameters, so these errors do not change the HTTP status.

Search boxes contain only the name field and submit button. Channel links above the results offer “All channels” and each supported channel, including channels without matching records. Selecting a channel preserves the query and initial filter and starts at page one. Channel filters apply to matches, group membership, totals, and executable rows. Active channel and initial filters are shown above the results with separate “Clear channel” and “Clear initial” links. Each clears only its own filter, preserving the query and the other filter and returning to page one. Reset search clears the query and both filters. Each group includes all related names in the selected channel, not only matching names. Detail links deliberately open the full identity across all channels. Search does not match package names or descriptions.

Empty results offer links to search all channels or all initials when the corresponding filter is active. When both filters are active, a “Clear both filters” link preserves the query while removing both restrictions. The links do not promise additional matches and require no extra queries when rendering the page. Unfiltered empty searches suggest a shorter name; an empty index states that no names are listed in this release. Browsing the full index remains available.

Project and executable pages replace the header's Search link with a compact name-only search form before the Dataset link, leaving the identity information and provider listings uninterrupted. On narrow screens, the search form and Dataset link wrap below the brand. The form starts empty and submits a new lookup across all channels and initials; an empty submission browses the index. Home and search pages retain their existing main search forms without a duplicate header form.

With JavaScript enabled, `/` focuses the page's search box and selects its current query. The shortcut ignores inputs, textareas, selects, editable content, IME composition, and events already handled elsewhere. Ctrl, Alt, and Meta combinations are left alone; Shift is allowed when needed to type `/` on a keyboard layout. The input advertises the shortcut through a tooltip and `aria-keyshortcuts`. Pages without a search box retain their normal keyboard behavior. The script is `web/search.js`, with tests run by `bun test ./web/search.test.js`.

Detail routes retain exact, case-sensitive lookup semantics:

```text
/executables/{executable}
/projects/{project}
```

`/about` presents the dataset, retaining `#bnck-database` as the release metadata anchor.

## JSON exports

Project and executable detail pages offer “Copy JSON” and “View JSON” actions for their accepted mappings. View JSON is an ordinary link that opens formatted JSON inline for inspection and manual copying. Copy JSON is shown only when the first-party script loads and clipboard APIs are available in a secure context, normally HTTPS or localhost. The actions use the existing exact, case-sensitive lookups and include all related channels and packages, without pagination or filtering. No export payload is embedded in the detail page or fetched until an action is used. The separate browser search dataset contains the full accepted mappings and curated project links.

Copy JSON fetches the export on demand and copies tab-indented plain text. After the clipboard write completes, the button shows “JSON copied” for two seconds before returning to “Copy JSON”. Loading and success announcements remain available to assistive technology without adding a visible line or shifting the surrounding content. Failed requests, invalid JSON, and rejected clipboard access produce an explicit failure message pointing to View JSON. Fetches time out after ten seconds, and the copy button is disabled while an attempt is pending. Where supported, a promised clipboard item starts the write during the click gesture so fetching does not lose user activation.

```text
/projects/{project}/providers.json
/executables/{executable}/providers.json
```

Each successful response begins with `dataset_release`, prefixed with `bnck:release:`, and a `query` object containing exactly one key: `project` or `executable`, with the exact requested name. The prefix namespaces the release identifier in exports without changing the stored identifier. The `providers` array contains four-field mappings, ordered by project, executable, channel, and package. Project exports include every executable attributed to that project; executable exports include every owning project. Names preserve their original spelling, and each mapping appears once. Project links, package versions, and installation instructions are not included.

```json
{
	"dataset_release": "bnck:release:2026-04-16.1",
	"query": { "executable": "hx" },
	"providers": [
		{
			"project": "helix",
			"executable": "hx",
			"channel": "debian",
			"package": "helix"
		}
	]
}
```

Responses use `application/json` without an attachment disposition. Successful exports are tab-indented with a trailing newline. An unknown exact identity has no generated export file and returns the static host's HTTP 404 response, which need not be JSON. Query parameters do not filter or paginate these exports. Exports are generated with the rest of the site; generation failures abort publication rather than leaving partial pages or exports.

SQLite reads and JSON serialization happen at build time. Viewing exports requires no JavaScript. The optional clipboard script at `web/exports.js` adds no dependencies or external services. Its unit tests run with `bun test ./web/exports.test.js`.

## Truthfulness

All example records, statistics, and release metadata come from the opened SQLite artifact. The interface does not imply a live acquisition status, comprehensive channel coverage, historical availability, or installability.

A missing identity or relation means there is no accepted provider record in this release. Empty results and detail pages explain this limitation. Channel counts describe accepted dataset records, not acquisition completeness. The SQLite artifact does not contain channel snapshot scope or failure details; those remain in the [release manifests](release-format.md).

The dataset page identifies the release and generation date. Artifact schemas, digests, generator details, and SQL examples belong in the release documentation rather than the public pages. Full release artifacts are not served; per-identity JSON exports are derived from the opened SQLite artifact.

## Implementation

`src/web/static_site.rs` implements `generate-site`. It opens the compressed or uncompressed SQLite artifact read-only and reuses the MiniJinja environment and detail-page models in `src/web.rs`. Shared forms, statistics, provider tables, and snapshot notes live in `web/templates/components.html`. The stylesheet is `web/style.css`. Debug builds read templates and browser assets from disk during generation; release builds embed the same files.

The generated browser dataset has `schema_version`, `dataset_release`, `generated_at`, supported channel keys and labels, and projects containing exact names, curated links, and executable/channel/package relations. It is a derived website artifact, not a replacement for the canonical release formats. CSS and JavaScript modules share `/assets/app.<hash>/`, hashed only from those files, so relative module imports remain intact. The browser dataset is a separate `/assets/dataset.<hash>.json`, hashed from its JSON bytes. Generated HTML references both URLs explicitly. Dataset-only changes retain the application URLs, application-only changes retain the dataset URL, and byte-identical rebuilds retain both.

`web/dataset.js` lazily fetches JSON on the first search or browse request, checks its schema and release, and builds an in-memory index. Concurrent and subsequent lookups share that fetch and index. Loading the home page without search parameters or opening an identity page does not download the dataset. Failed loads time out after ten seconds and offer an explicit retry. HTTP caching handles repeat visits; there is no service worker, IndexedDB, SQLite runtime, or offline-availability guarantee.

`web/query.js` implements the search contract, including Rust-compatible whitespace trimming, ASCII case folding, Unicode scalar ordering, and whole-project pagination. `web/results.js` escapes all inserted dataset values and queries and encodes name segments in links. `web/browser.js` coordinates rendering, URLs, history, loading/error announcements, and stale-request protection. Search navigation focuses the main content, while initial loads do not take focus. The slash shortcut finds the current search input after navigation.

Forms have explicit labels, tables have captions and scoped headings, keyboard navigation has visible focus and a skip link, and the layout works down to a 320-pixel viewport. Native executable disclosures require no additional JavaScript. A no-JavaScript notice on the home page explains that search and filtered browsing need scripting.

`bun test ./web/*.test.js` tests query validation, rendering, caching, retry behavior, history, stale requests, and optional enhancements. Rust tests validate generated pages and exports, failure cleanup, deterministic asset identities, and browser/SQLite search parity; the parity test invokes Bun. The existing `serve` command retains the SQLite-backed interface for server-rendered development and query comparisons.

## Static deployment

Generate into a fresh output directory. The generator stages the entire build beside the destination and publishes it only after every file succeeds; it refuses an existing destination. Identity directories contain `index.html` and `providers.json`. The root contains `index.html`, `about/index.html`, `404.html`, `_headers`, and the content-addressed `assets/` directory.

Serve the site from an origin root on a case-sensitive filesystem/host. Directory index lookup must resolve `/projects/{project}`, `/executables/{executable}`, and `/about`; a trailing-slash redirect is acceptable. Preserve exact case and decode percent-encoded UTF-8 path segments once. Configure `404.html` as the error page, not a catch-all rewrite to the search page. Names that cannot safely become static directory components cause generation to fail; case collisions on a case-insensitive build filesystem also fail rather than overwrite pages.

The generated `_headers` file supplies settings for hosts supporting that convention. Other hosts must configure equivalent response headers:

- HTML and JSON exports: `Cache-Control: no-cache`
- `/assets/*`: `Cache-Control: public, max-age=31536000, immutable`
- All responses: `X-Content-Type-Options: nosniff`
- Content security policy: `default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'`

Serve HTML as `text/html`, JSON as `application/json` without attachment disposition, CSS as `text/css`, and modules as `text/javascript` or `application/javascript`. Enable HTTP Brotli or gzip compression at the host; browsers decompress these responses automatically. No application-level compressed payload or decoder is shipped.

Publish each build's pages and assets atomically, retaining previous application directories and dataset files so already-open pages can still load matching code and data. Regenerate all pages and exports from the same release; never change the contents at an existing content-addressed URL. Browser caches may evict files and must be allowed to fetch them again.

`just preview` runs the dependency-free Bun server in `web/preview.js` on localhost. It serves generated files with directory index lookup, cache/security headers, and a real 404 fallback. It is a local preview tool, not a production hosting requirement.
