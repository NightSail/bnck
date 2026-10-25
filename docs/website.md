# Website

## Purpose

The website is a reference interface to the [provider dataset](design.md), not a software directory or package installer. It finds the projects and distribution packages behind an executable name, and the executables provided by a canonical project.

Dataset information and the short-name map are static and work without JavaScript. Search, filtering, pagination, project pages, and executable pages use vanilla JavaScript over a locally loaded dataset. Clipboard copying is an optional enhancement, and the site uses no external fonts or third-party services.

## Views

- Home: Search entry point, a featured project, dataset counts, and browse links
- Search: Executable names and project IDs grouped by owning project, with highlighted matches, executable links, per-executable channels, and pagination
- Executable: All projects providing the exact command name, their channel packages, and separate reference and implementation associations
- Project: Package coverage of exact executable names, curated name associations, and curator-supplied links
- Dataset: Field definitions, counts by channel, coverage limits, release identifier and date, release links, artifact downloads, and license
- Short: Separate pages for every two- and three-letter lowercase name, linked from section navigation
- About: Lookup behavior, data sources and limits, and links to source code, issues, releases, contributing guidance, and license

The short name map lists `aa` through `zz` at `/short/2`, with one page per leading letter for three-letter names at `/short/a` through `/short/z`. Each page links to every section and marks its own. Names seen in at least one project link to their executable pages; unseen names remain plain.

Project pages normally use one coverage matrix: packages as rows grouped under channel headings, executable names as columns. Checkmarks identify executables observed in each package; hyphens mean not observed in this dataset, not unavailable. The matrix scrolls horizontally on narrow screens while keeping package names and channel headings visible. Package links show the exact channel-native name and point to current external channel pages, not acquisition evidence or installation instructions.

A project with more than 10,000 package/name combinations uses a sparse coverage view instead: an alphabetical executable index, then one row per package listing only the executable names observed in it. The same per-executable anchors remain available. This avoids a dense cross-product and keeps browser rendering responsive; absence still means not observed in this dataset.

Project and executable pages are rendered in the browser from the shared dataset. The host serves one `/identity.html` shell for both route families.

## Search contract

Search uses GET-style query URLs at `/` so results can be bookmarked and shared. The static home page reads those parameters and computes results in the browser.

| Parameter | Meaning | Default |
| --- | --- | --- |
| `q` | Literal substring of an executable name or project ID; at most 100 Unicode characters after trimming | Absent shows the home page; empty browses the index |
| `channel` | A supported channel key, or empty for all channels | Empty |
| `initial` | `A` through `Z` for the first character of the project ID, or `#` for numbers and symbols | Empty shows all initials |
| `page` | Positive page number | `1` |

Search folds ASCII letter case but does not normalize Unicode or interpret wildcards. Matches rank by exact spelling, case-insensitive equality, prefix, then other substring matches; shorter matching names rank first within each class. Each project ranks by its best matching project ID or executable name, breaking ties by project ID. Empty-name browsing sorts by project ID. Exact project or executable matches receive an inline tag, preserving case and spelling.

Results return one group per owning project, whether the query matches its ID or one of its executable names. Ownership comes from accepted provider records, never a shared prefix or inferred main executable. A command provided by multiple projects appears under each owner. Name associations add project rows for names a project implements under another command or through an applet namespace; these rows carry a `reference` or `implementation` badge.

Without a channel filter, rows show the distinct channels for each project/executable pair. With one, rows omit the channel list and the active channel stays visible above the results.

Each group previews up to three executable rows; additional rows use a native "N more executables" disclosure that works without JavaScript. Executables are ordered by match quality, then length and spelling, with nonmatches last; exact executable matches stay in the preview. Empty-name browsing orders rows by exact name.

An initial filter offers "All" and only the initials present among projects matching the current query and channel. Available initials are computed before filtering and pagination. Selecting an initial preserves query and channel and starts at page one; "All" clears only the initial filter. Refining the query preserves the selected initial.

Each page contains at most 30 projects, with totals counting matches across all pages. A project's executable list is never split across pages, and case variants remain distinct. Pagination preserves query and filters; submitting the search form starts at page one. A page beyond the available results, or any invalid parameter, displays an explicit error; static hosting returns the same root HTML regardless of parameters.

Channel links offer "All channels" and every supported channel, including those without matches; selecting one preserves the query and initial filter and starts at page one. Each group includes all related names in the selected channel, not only matching ones, while detail links open the full identity across all channels. Search does not match package names or descriptions.

Empty results offer links to clear active filters. Unfiltered empty searches suggest a shorter name.

Detail routes retain exact, case-sensitive lookup semantics:

```text
/executables/{executable}
/projects/{project}
/short/2
/short/{letter}
/dataset
/about
```

The host serves the shared `/identity.html` shell for `/projects/*` and `/executables/*`. The browser decodes the path once and looks up the exact identity in the dataset; unknown identities render a searchable not-found page. The dataset release section keeps `#release` as its anchor.

## JSON exports

Project and executable pages offer "Copy JSON" and "View JSON" actions. Both are generated in the browser from the loaded dataset using exact, case-sensitive lookups. "View JSON" expands a tab-indented block in the page. "Copy JSON" requires the script and clipboard APIs in a secure context.

The generated object begins with `dataset_release`, prefixed with `bnck:release:`, and a `query` object with exactly one key, `project` or `executable`, holding the exact requested name. The prefix namespaces the release identifier without changing the stored identifier. The `providers` array holds four-field mappings ordered by project, executable, channel, and package. Project exports include every executable attributed to that project; executable exports include every owning project. The `associations` array holds channel-independent name associations: project exports list associated executables, and executable exports list associated projects, with `reference` or `implementation` roles. Names preserve their original spelling, and each relation appears once. Project links, package versions, and installation instructions are not included.

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

Exports are tab-indented and end with a newline. Query parameters do not filter or paginate exports. The former per-identity URLs `/projects/{project}/providers.json` and `/executables/{executable}/providers.json` redirect to their identity pages; no per-identity files are generated. Generation failures abort publication rather than leaving partial output.

## Truthfulness

All example records, statistics, and release metadata come from the opened SQLite artifact. The interface never implies a live acquisition status, comprehensive channel coverage, historical availability, or installability.

A missing identity or relation means there is no accepted provider record or name association in this release. Channel counts describe accepted dataset records, not acquisition completeness. The SQLite artifact lacks channel scope and failure details, which stay in the release manifests.

## Implementation

`web/generate.ts` implements static generation. `web/database.ts` opens the compressed or uncompressed SQLite artifact and exposes dataset queries; `web/site.ts` stages the complete output; `web/pages.ts`, `web/identity-view.ts`, and `web/components.ts` render HTML. `web/identity.ts` mounts project and executable pages in the browser, and `web/exports.ts` handles JSON actions. The stylesheet is `web/style.css`. The generator bundles TypeScript browser entry points with Bun and reads source files from disk on each build.

The generated browser dataset has `schema_version`, `dataset_release`, `generated_at`, supported channel keys and labels, and projects containing exact names, curated links, executable/channel/package relations, and name associations. Its `schema_version` is independent of the release, provider, and association schemas; increment it when the project, link, provider, or association shape changes incompatibly. It is a derived website artifact, not a replacement for the canonical release formats.

The stylesheet and bundled JavaScript entry points share `/assets/app.<hash>/`, hashed from those files. The browser dataset is a separate `/assets/dataset.<hash>.json`, hashed from its JSON bytes. Generated HTML references both URLs, so dataset-only and application-only changes retain the other URL, and byte-identical rebuilds retain both.

`web/dataset.ts` lazily fetches JSON on the first search, browse, project, or executable request, checks its schema and release, and builds an in-memory index shared by concurrent and later lookups. Loading the home page without parameters downloads no dataset; identity pages load it to render their content. Failed loads time out after ten seconds and offer an explicit retry. HTTP caching handles repeat visits; there is no service worker, IndexedDB, SQLite runtime, or offline guarantee.

Forms have explicit labels, tables have captions and scoped headings, keyboard navigation has visible focus and a skip link, and the layout works down to a 320-pixel viewport. No-JavaScript notices explain that search, filtered browsing, project pages, and executable pages need scripting.

`just site-dev` builds the site into a temporary directory, watches `web/` and the SQLite artifact, and reloads connected pages after a successful rebuild. `just site-preview` serves a generated directory from disk.

## Testing

`bun test ./web/*.test.ts` covers search, identity routing and rendering, caching, history, JSON actions, deterministic asset identities, and browser/SQLite search parity. `bun run typecheck` checks the site source.

## Static deployment

The `Site` workflow builds and deploys the output from a released SQLite artifact without running acquisition or publishing the dataset. Generate into a fresh output directory. The generator stages the whole build beside the destination and publishes it only after every file succeeds; it refuses an existing destination.

The output root contains `index.html`, `about.html`, `dataset.html`, `identity.html`, `short/2.html`, `short/{letter}.html`, `404.html`, `_headers`, `_redirects`, and the content-addressed `assets/` directory. It generates no files per project or executable, so output file count stays bounded as the dataset grows.

Serve the site from an origin root. Extensionless HTML file lookup must resolve `/dataset`, `/short/2`, `/short/{letter}`, and `/about` from their `.html` files; directory index lookup must resolve `/` from `index.html`. Proxy `/projects/*` and `/executables/*` to `/identity.html` with status 200 while preserving the original path. Trailing slashes on leaf pages redirect to their file paths. Preserve exact case and decode percent-encoded UTF-8 path segments once; the browser performs identity lookup. Configure `404.html` as the error page, not a catch-all rewrite. Generation fails if any output file exceeds the host's 25 MiB asset limit or the output exceeds its 20,000-file limit.

The generated `_headers` file supplies settings for hosts supporting that convention; others must configure equivalents:

- HTML pages: `Cache-Control: no-cache`
- `/assets/*`: `Cache-Control: public, max-age=31536000, immutable`
- All responses: `X-Content-Type-Options: nosniff`
- Content security policy: `default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'`

The redirect map template lives in `web/_redirects`; the generator substitutes the current dataset release for the latest aliases. Cloudflare Workers applies the generated `_redirects` file directly; other hosts must configure equivalent redirects and rewrites:

- `/about/`, `/dataset/`, `/short/{section}/`, `/projects/{project}/`, `/executables/{executable}/`: 301 to the corresponding path
- `/projects/*` and `/executables/*`: 200 proxy to `/identity.html`
- `/projects/{project}/providers.json` and `/executables/{executable}/providers.json`: 301 to the identity page
- `/data` and `/data/`: 302 to `/data/latest`
- `/data/latest`: 302 to the current dataset release page
- `/data/latest/*`: 302 to the corresponding current dataset release asset
- `/data/:release`: 301 to the `bnck-dataset-:release` page
- `/data/:release/*`: 301 to the corresponding versioned release asset

Publish each build's pages, identity shell, and assets atomically, retaining previous application directories and dataset files so open pages can load matching code and data. Regenerate everything from the same release; never change contents at an existing content-addressed URL.

`just site-preview` runs the dependency-free Bun server in `web/preview.ts`, binding `127.0.0.1:3000` by default and accepting a `HOST:PORT` address. It serves generated files with extensionless HTML and directory index lookup, cache and security headers, the shared identity shell for `/projects/{project}` and `/executables/{executable}`, and a real 404 fallback. It does not evaluate host redirects.
