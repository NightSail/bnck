# Website

## Purpose

The website is a reference interface to the [provider dataset](design.md), not a software directory or package installer. Its primary tasks are finding the projects and distribution packages behind an executable name, and finding the executables provided by a canonical project.

The visual hierarchy follows those tasks: search first, identities second, provider relations next, release context alongside results. A dark, terminal-informed theme uses graphite surfaces, restrained green and cyan accents, and monospace headings, controls, and exact dataset values. Explanatory prose stays proportional, while forms, tables, and navigation retain web-native behavior: no simulated terminal window, command session, or animated cursor. Search, filtering, and pagination use vanilla JavaScript over a locally loaded dataset. Identity pages, dataset information, and JSON views are static and work without JavaScript; clipboard copying is an optional enhancement. The site uses no external fonts or third-party services.

The header uses `bnck(1)` followed by `bin name check`, a plain monospace label in manual-page style. `(1)` denotes the manual section for executable programs and shell commands. Main navigation links to search or the current search form, Short, Dataset, and About. The footer repeats the site sections and links to dataset releases and source code.

Public copy uses short, descriptive headings and instructions, explains lookup behavior and dataset limits, and avoids slogans and implementation details. It does not show provider-record terminology or counts; summaries speak of executables, projects, channels, and packages. Headings, navigation, controls, and labels use lowercase; sentence case is reserved for prose and proper names.

## Views

- Home: Search entry point, a featured project, dataset counts, and browse links
- Search: Executable names and project IDs grouped by owning project, with highlighted matches, executable links, per-executable channels, and pagination
- Executable: All projects providing the exact command name, each grouped with its channel packages; reference and implementation associations appear separately, and multiple provider identities receive an ambiguity notice
- Project: A coverage matrix of exact executable names and channel packages, curated name associations, and curator-supplied project links
- Dataset: Field definitions, project and package counts by channel, coverage limits, release identifier, generation date, latest release alias, latest tag, GitHub Releases, artifact downloads, and license statement
- Short: Separate pages for every two- and three-letter lowercase name, linked from a shared section navigation; names seen in at least one project link to their executable pages, while unseen names remain plain and muted
- About: Lookup behavior, data sources and limits, and links to source code, issues, releases, contributing guidance, and license

Short name sections list `aa` through `zz` at `/short/2`. Three-letter names use one page per leading letter: `/short/a` lists `aaa` through `azz`, through `/short/z` for `zaa` through `zzz`. Each page links to every section and marks its own section in the navigation. Section counts describe seen names within that section. The map reflects the current dataset release and does not imply historical availability.

Project pages use one coverage matrix: packages as rows grouped under channel headings, executable names as columns. Checkmarks identify executables observed in each package; hyphens mean not observed in this dataset, not unavailable. Executable column headers preserve the anchors used by search links and link to executable details. Executables and channels/packages sort by exact name. The matrix scrolls horizontally with keyboard access on narrow screens, keeping package names and channel headings visible.

Package links display the exact channel-native package name rather than the URL. They point to current external channel pages, not pinned acquisition evidence or installation instructions. Unknown package-page mappings remain plain text.

## Search contract

Search uses GET-style query URLs at `/` so results can be bookmarked and shared. The static home page reads those parameters and computes results in the browser. On the home/search page, forms, filter links, and pagination update browser history without reloading; Back/Forward restores the URL's state. Modified clicks and links to other pages keep native browser behavior.

| Parameter | Meaning | Default |
| --- | --- | --- |
| `q` | Literal substring of an executable name or project ID; at most 100 Unicode characters after trimming surrounding whitespace | Absent shows the home page; empty browses the index |
| `channel` | A supported channel key, or empty for all channels | Empty |
| `initial` | `A`–`Z` for the first character of the project ID, or `#` for numbers and symbols | Empty shows all initials |
| `page` | Positive page number | `1` |

Search folds ASCII letter case but does not normalize Unicode or interpret wildcards. Matches rank by exact spelling, case-insensitive equality, prefix, then other substring matches; shorter matching names rank first within each class. Each project ranks by its best matching project ID or executable name, breaking ties by project ID. Empty-name browsing sorts by project ID.

An inline "Exact match" tag identifies each project ID or executable name equal to the trimmed query, preserving case and literal spelling. Project tags follow the project name; executable tags appear at the end of the row, after any channels. Shared executable names receive a tag under each matching owner; a project and its executable can both be tagged. Partial and case-insensitive-only matches are not tagged. Tags follow the active filters and do not appear when browsing.

Results return one group per owning project, whether the query matches its ID or one of its executable names. Ownership comes from accepted provider records, never a shared prefix or inferred main executable; independently owned ecosystem tools remain separate projects. A command provided by multiple projects appears under each owner, with executable links opening that section on the owner's project page. Name associations add project rows for names a project implements under another command or through an applet namespace; these rows link to the executable page and carry a `reference` or `implementation` badge.

Results use a minimal grouped layout without column headers or type labels: an underlined project heading linked to project details, followed by executable rows, each with an underlined `name(1)` link. Without a channel filter, rows also show the distinct channels for that project/executable pair; with one, rows omit the channel list and the active channel stays visible above the results.

Matching substrings are highlighted with semantic `mark` elements, preserving original spelling and HTML escaping, using the same literal, ASCII-case-insensitive matching as search, including repeated occurrences. Empty queries produce no highlights.

Each group previews up to three executable rows; additional rows use the same format inside a native "N more executables" disclosure that works without JavaScript. Executables are ordered by match quality, then length and spelling, with nonmatches last; exact executable matches stay in the preview even for large projects. Empty-name browsing orders rows by exact name.

An initial filter above the results offers "All" and only the initials present among projects matching the current query and channel. Available initials are computed before initial filtering and pagination, so selecting one does not hide the other choices. Selecting an initial preserves query and channel and starts at page one; "All" clears only the initial filter. Refining the query preserves the selected initial.

Each page contains at most 30 projects, with totals counting matches across all pages. A project's executable list is never split across pages, and case variants of executable names remain distinct. Pagination shows numbered shortcuts for the first, last, and nearby pages, with ellipses for gaps, plus a "Go to page" form. Pagination preserves query and filters; submitting the search form starts at page one. A page beyond the available results, or any invalid query parameter, displays an explicit error. Static hosting returns the same root HTML regardless of parameters, so these errors do not change the HTTP status.

Search boxes contain only the name field and submit button. Channel links above the results offer "All channels" and every supported channel, including channels without matching records; selecting one preserves the query and initial filter, applies to matches, group membership, totals, and executable rows, and starts at page one. Active channel and initial filters are shown above the results with separate "Clear channel" and "Clear initial" links, each clearing only its own filter and returning to page one. Reset search clears the query and both filters. Each group includes all related names in the selected channel, not only matching ones, while detail links open the full identity across all channels. Search does not match package names or descriptions.

Empty results offer links to search all channels or all initials when the corresponding filter is active. When both are active, "Clear both filters" preserves the query and removes both. The links do not promise additional matches. Unfiltered empty searches suggest a shorter name; an empty index states that no names are listed in this release.

Project and executable pages replace the header's Search link with a compact name-only search form before the Dataset and About links, leaving identity information and provider listings uninterrupted. On narrow screens the form and navigation wrap below the brand. The form starts empty and submits a lookup across all channels and initials; an empty submission browses the index. Home and search pages keep their main search form without a duplicate header form.

With JavaScript enabled, `/` focuses the page's search box and selects its current query. The shortcut ignores inputs, textareas, selects, editable content, IME composition, and events already handled elsewhere; Ctrl, Alt, and Meta combinations are left alone, and Shift is allowed when needed to type `/` on a keyboard layout. The input advertises the shortcut through a tooltip and `aria-keyshortcuts`. Pages without a search box keep normal keyboard behavior. The source is `web/search.ts`, tested by `bun test ./web/search.test.ts`.

Detail routes retain exact, case-sensitive lookup semantics:

```text
/executables/{executable}
/projects/{project}
```

`/short/2` presents two-letter names, and `/short/{letter}` presents three-letter names for that leading letter. `/dataset` presents the dataset and its release artifacts. `/about` presents the project and links to external resources. The dataset release section keeps `#release` as its anchor.

## JSON exports

Project and executable detail pages offer "Copy JSON" and "View JSON" actions for their accepted mappings. View JSON is an ordinary link that opens formatted JSON inline. Copy JSON appears only when the first-party script loads and clipboard APIs are available in a secure context, normally HTTPS or localhost. Both use the exact, case-sensitive lookups and include all related channels and packages, without pagination or filtering. No export payload is embedded in the detail page or fetched until an action is used. The separate browser search dataset contains the full accepted mappings, curated project links, and name associations.

Copy JSON fetches the export on demand and copies tab-indented plain text, then shows "JSON copied" for two seconds. Loading and success announcements reach assistive technology without adding a visible line or shifting content. Failed requests, invalid JSON, and rejected clipboard access produce an explicit failure message pointing to View JSON. Fetches time out after ten seconds, and the button is disabled while a copy is pending. Where supported, a promised clipboard item starts the write during the click gesture so fetching does not lose user activation.

```text
/projects/{project}/providers.json
/executables/{executable}/providers.json
```

Each successful response begins with `dataset_release`, prefixed with `bnck:release:`, and a `query` object with exactly one key, `project` or `executable`, holding the exact requested name. The prefix namespaces the release identifier in exports without changing the stored identifier. The `providers` array holds four-field mappings ordered by project, executable, channel, and package. Project exports include every executable attributed to that project; executable exports include every owning project. The `associations` array holds channel-independent name associations: project exports list associated executables, and executable exports list associated projects, with `reference` or `implementation` roles. Names preserve their original spelling, and each relation appears once. Project links, package versions, and installation instructions are not included.

```json
{
	"dataset_release": "bnck:release:2026-04-16.1",
	"query": { "executable": "rg" },
	"providers": [
		{
			"project": "ripgrep",
			"executable": "rg",
			"channel": "arch",
			"package": "ripgrep"
		}
	],
	"associations": [
		{ "project": "ripgrep", "role": "reference" }
	]
}
```

Responses use `application/json` without an attachment disposition, are tab-indented, and end with a newline. An unknown exact identity has no export file and returns the static host's HTTP 404, which need not be JSON. Query parameters do not filter or paginate exports. Generation failures abort publication rather than leaving partial pages or exports.

SQLite reads and JSON serialization happen at build time; viewing exports requires no JavaScript. The optional clipboard source `web/exports.ts` adds no dependencies or external services and is tested by `bun test ./web/exports.test.ts`.

## Truthfulness

All example records, statistics, and release metadata come from the opened SQLite artifact. The interface never implies a live acquisition status, comprehensive channel coverage, historical availability, or installability.

A missing identity or relation means there is no accepted provider record or name association in this release; empty results and detail pages explain this limitation. Channel counts describe accepted dataset records, not acquisition completeness. The SQLite artifact lacks channel snapshot scope and failure details, which stay in the [release manifests](release-format.md).

The dataset page identifies the release and generation date and links to the latest release alias, the matching GitHub tag, all GitHub Releases, and current artifact downloads. Artifact schemas, generator details, and SQL examples belong in the release documentation, not public pages. Full release artifacts are hosted on GitHub Releases rather than copied into the site; per-identity JSON exports derive from the opened SQLite artifact.

## Implementation

`web/generate.ts` implements static generation. `web/database.ts` opens the compressed or uncompressed SQLite artifact in memory and exposes the dataset queries; `web/site.ts` stages the complete output; `web/pages.ts` and `web/components.ts` render the HTML. The stylesheet is `web/style.css`. The generator bundles the TypeScript browser entry points with Bun and reads every source file from disk on each build.

The generated browser dataset has `schema_version`, `dataset_release`, `generated_at`, supported channel keys and labels, and projects containing exact names, curated links, executable/channel/package relations, and name associations. Its `schema_version` is independent of the release, provider, and association schemas; increment it when the project, link, provider, or association shape changes incompatibly. It is a derived website artifact, not a replacement for the canonical release formats. The stylesheet and bundled JavaScript entry points share `/assets/app.<hash>/`, hashed only from those files. The browser dataset is a separate `/assets/dataset.<hash>.json`, hashed from its JSON bytes. Generated HTML references both URLs explicitly: dataset-only changes retain the application URLs, application-only changes retain the dataset URL, and byte-identical rebuilds retain both.

`web/dataset.ts` lazily fetches JSON on the first search or browse request, checks its schema and release, and builds an in-memory index shared by concurrent and later lookups. Loading the home page without parameters or opening an identity page downloads no dataset. Failed loads time out after ten seconds and offer an explicit retry. HTTP caching handles repeat visits; there is no service worker, IndexedDB, SQLite runtime, or offline guarantee.

`web/query.ts` implements the search contract, including SQLite-compatible whitespace trimming, ASCII case folding, Unicode scalar ordering, and whole-project pagination. `web/results.ts` escapes all inserted dataset values and queries and encodes name segments in links. `web/browser.ts` coordinates rendering, URLs, history, loading/error announcements, and stale-request protection. Search navigation focuses the main content; initial loads do not take focus, and the slash shortcut finds the current search input after navigation.

Forms have explicit labels, tables have captions and scoped headings, keyboard navigation has visible focus and a skip link, and the layout works down to a 320-pixel viewport. Native executable disclosures need no additional JavaScript. A no-JavaScript notice on the home page explains that search and filtered browsing need scripting.

`just site-dev` builds the site into a temporary directory, watches `web/` and the SQLite artifact, and reloads connected pages after a successful rebuild. The watcher and the server-sent reload endpoint are hand-rolled; there are no runtime site dependencies. `just site-preview` instead serves a generated directory from disk.

`bun test ./web/*.test.ts` covers query validation, rendering, caching, retry behavior, history, stale requests, optional enhancements, generated pages and exports, failure cleanup, deterministic asset identities, and browser/SQLite search parity. `bun run typecheck` checks the site source.

## Static deployment

Generate into a fresh output directory. The generator stages the whole build beside the destination and publishes it only after every file succeeds; it refuses an existing destination. Identity directories contain `index.html` and `providers.json`; the root contains `index.html`, `about/index.html`, `dataset/index.html`, `short/2/index.html`, `short/{letter}/index.html`, `404.html`, `_headers`, `_redirects`, and the content-addressed `assets/` directory.

Serve the site from an origin root on a case-sensitive filesystem/host. Directory index lookup must resolve `/projects/{project}`, `/executables/{executable}`, `/dataset`, `/short/2`, `/short/{letter}`, and `/about`; a trailing-slash redirect is acceptable. Preserve exact case and decode percent-encoded UTF-8 path segments once. Configure `404.html` as the error page, not a catch-all rewrite to the search page. Names that cannot safely become static directory components, and case collisions on a case-insensitive build filesystem, fail generation rather than overwrite pages.

The generated `_headers` file supplies settings for hosts supporting that convention; others must configure equivalents:

- HTML and JSON exports: `Cache-Control: no-cache`
- `/assets/*`: `Cache-Control: public, max-age=31536000, immutable`
- All responses: `X-Content-Type-Options: nosniff`
- Content security policy: `default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'`

The redirect map template lives in `web/_redirects`; the generator substitutes the current dataset release for the latest aliases. Cloudflare Workers applies the generated `_redirects` file directly; other hosts must configure equivalent redirects:

- `/data` and `/data/`: 302 to `/data/latest`
- `/data/latest`: 302 to the current dataset release page
- `/data/latest/*`: 302 to the corresponding current dataset release asset
- `/data/:release`: 302 to the `bnck-dataset-:release` page
- `/data/:release/*`: 301 to the corresponding versioned release asset

Serve HTML as `text/html`, JSON as `application/json` without attachment disposition, CSS as `text/css`, and modules as `text/javascript` or `application/javascript`. Enable HTTP Brotli or gzip compression at the host; browsers decompress automatically, and no application-level compressed payload or decoder is shipped.

Publish each build's pages and assets atomically, retaining previous application directories and dataset files so open pages can load matching code and data. Regenerate all pages and exports from the same release; never change contents at an existing content-addressed URL. Browser caches may evict files and must be allowed to fetch them again.

`just site-preview` runs the dependency-free Bun server in `web/preview.ts`, binding `127.0.0.1:3000` by default and accepting a `HOST:PORT` address. It serves generated files with directory index lookup, cache/security headers, and a real 404 fallback. It does not evaluate host redirects. It is a local preview tool, not a production hosting requirement.
