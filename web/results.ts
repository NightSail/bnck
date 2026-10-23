import { escapeHtml } from './html';
import { formatNumber } from './naming';
import {
	RELATED_LIMIT,
	encodeSegment,
	foldAscii,
	searchUrl,
	type IndexedSearchResults,
	type RankedProject,
	type RankedRelation,
	type Search,
} from './query';
import type { BrowserChannel } from './types';

export { escapeHtml };

export function highlightName(name: string, query: string): string {
	if (!query) return escapeHtml(name);
	const folded = foldAscii(name);
	const needle = foldAscii(query);
	let offset = 0;
	let html = '';
	for (let start = folded.indexOf(needle); start !== -1; start = folded.indexOf(needle, offset)) {
		html += escapeHtml(name.slice(offset, start));
		offset = start + query.length;
		html += `<mark>${escapeHtml(name.slice(start, offset))}</mark>`;
	}
	return html + escapeHtml(name.slice(offset));
}

const countLabel = (count: number, singular: string, plural = `${singular}s`) =>
	`${formatNumber(count)} ${count === 1 ? singular : plural}`;
const exactTag = (name: string, query: string) =>
	query && name === query ? '<span class=\'exact-match\'>exact match</span>' : '';
const link = (url: string, text: string, attributes = '') =>
	`<a href='${escapeHtml(url)}' ${attributes}>${text}</a>`;

type SearchFields = Partial<Pick<Search, 'query' | 'channel' | 'initial'>>;

function searchForm(search: SearchFields = {}): string {
	return `<form class='search-form' action='/' method='get' role='search'>
		<label class='visually-hidden' for='q'>executable name or project ID</label>
		<div class='search-input'>
			<span class='search-symbol' aria-hidden='true'>⌕</span>
			<input id='q' name='q' type='search' value='${escapeHtml(search.query ?? '')}' placeholder='press / to'
				autocomplete='off' spellcheck='false' aria-describedby='search-hint'
				aria-keyshortcuts='/' title='press / to focus search'>
			<button type='submit'>search <span aria-hidden='true'>↵</span></button>
		</div>
		${search.channel ? `<input type='hidden' name='channel' value='${escapeHtml(search.channel)}'>` : ''}
		${search.initial ? `<input type='hidden' name='initial' value='${escapeHtml(search.initial)}'>` : ''}
	</form>`;
}

function filterLinks(search: Search, values: { value: string; label: string }[], field: 'channel' | 'initial'): string {
	return values.map(({ value, label }) => link(
		searchUrl(search, { [field]: value, page: 1 }), escapeHtml(label),
		`${search[field] === value ? 'aria-current=\'true\'' : ''}
		${field === 'initial' && value === '#' ? 'aria-label=\'numbers and symbols\'' : ''}`,
	)).join('');
}

function filterNotices(search: Search, labels: Map<string, string>): string {
	let html = '';
	if (search.channel) {
		html += `<p class='filter-notice'>Showing matches in <strong>${escapeHtml(labels.get(search.channel))}</strong>.
			Open a result for all listed channels.
			${link(searchUrl(search, { channel: '', page: 1 }), 'clear channel', 'class=\'text-link\'')}</p>`;
	}
	if (search.initial) {
		const label = search.initial === '#' ? 'numbers and symbols' : search.initial;
		html += `<p class='filter-notice'>project initial: <strong>${escapeHtml(label)}</strong>.
			${link(searchUrl(search, { initial: '', page: 1 }), 'clear initial', 'class=\'text-link\'')}</p>`;
	}
	return html;
}

function relatedRows(project: RankedProject, relations: RankedRelation[], search: Search,
	labels: Map<string, string>): string {
	return `<ul class='related-rows' role='list'>${relations.map((relation) => {
		const url = `/projects/${encodeSegment(project.name)}#executable-${encodeSegment(relation.name)}`;
		const name = `<code>${highlightName(relation.name, search.query)}(1)</code>`;
		const channels = search.channel ? '' : `: ${relation.channels.map((channel) =>
			escapeHtml(labels.get(channel))).join(', ')}`;
		return `<li>${link(url, name, 'class=\'related-name\'')}${channels}
			${exactTag(relation.name, search.query)}</li>`;
	}).join('')}</ul>`;
}

function projectResult(project: RankedProject, position: number, search: Search,
	labels: Map<string, string>): string {
	const remaining = project.related.length - RELATED_LIMIT;
	const url = `/projects/${encodeSegment(project.name)}`;
	const name = `<code>${highlightName(project.name, search.query)}</code>`;
	return `<li><article class='name-result' aria-labelledby='result-${position}'>
		<header class='name-result-heading'><h3 id='result-${position}'>
			${link(url, name)} ${exactTag(project.name, search.query)}
		</h3></header>
		${relatedRows(project, project.related.slice(0, RELATED_LIMIT), search, labels)}
		${remaining > 0 ? `<details class='more-related'>
			<summary>${countLabel(remaining, 'more executable')}
				<span class='visually-hidden'> for ${escapeHtml(project.name)}</span></summary>
			${relatedRows(project, project.related.slice(RELATED_LIMIT), search, labels)}
		</details>` : ''}
	</article></li>`;
}

function emptyResults(search: Search): string {
	const hasFilters = search.channel || search.initial;
	const queryLabel = search.query ? `for <code>“${escapeHtml(search.query)}”</code>` : '';
	const action = search.query ? 'search' : 'browse';
	const recoveries = (['channel', 'initial'] as const).filter((field) => search[field]).map((field) =>
		`<li>${link(searchUrl(search, { [field]: '', page: 1 }), `${action} all ${field}s ${queryLabel}`,
			'class=\'text-link\'')}</li>`);
	if (search.channel && search.initial) {
		recoveries.push(`<li>${link(searchUrl(search, { channel: '', initial: '', page: 1 }),
			'clear both filters', 'class=\'text-link\'')}</li>`);
	}
	return `<div class='empty-state'><span class='empty-symbol' aria-hidden='true'>∅</span><h3>no matches</h3>
		<p>${hasFilters ? 'Try removing a filter.' : search.query ?
			'Try a shorter name.' : 'No names are listed in this release.'}</p>
		${hasFilters ? `<ul class='search-recovery' role='list'>${recoveries.join('')}</ul>` : ''}
		${link('/?q=', 'browse names <span aria-hidden=\'true\'>→</span>', 'class=\'button-link\'')}
	</div>`;
}

function pageList(results: IndexedSearchResults): string {
	return `<ol class='page-list'>${results.page_links.map((item) => item.url
		? `<li>${link(item.url, escapeHtml(item.label),
			`class='page-link'${item.is_current ? " aria-current='page'" : ''}`)}</li>`
		: `<li><span class='page-gap' aria-hidden='true'>${escapeHtml(item.label)}</span></li>`
	).join('')}</ol>`;
}

function pageJump(search: Search, results: IndexedSearchResults): string {
	const hidden = (name: string, value: string) => `<input type='hidden' name='${name}' value='${escapeHtml(value)}'>`;
	return `<form class='page-jump' action='/' method='get'>
		${hidden('q', search.query)}${hidden('channel', search.channel)}${hidden('initial', search.initial)}
		<label for='page-jump'>go to page</label>
		<input id='page-jump' name='page' type='number' min='1' max='${results.pages}' value='${results.page}'
			inputmode='numeric' required>
		<button type='submit'>go</button>
	</form>`;
}

function pagination(search: Search, results: IndexedSearchResults): string {
	return `<nav class='pagination' aria-label='Result pages'>
		<span class='page-summary'>${formatNumber(results.start)}–${formatNumber(results.end)} of ${
			formatNumber(results.total)}</span>
		<div class='page-controls'>
			${results.previous ? link(results.previous, '← previous', "class='page-step' rel='prev'")
				: '<span class=\'page-step is-disabled\'>← previous</span>'}
			${results.page_links.length ? pageList(results) : ''}
			${results.next ? link(results.next, 'next →', "class='page-step' rel='next'")
				: '<span class=\'page-step is-disabled\'>next →</span>'}
		</div>
		${pageJump(search, results)}
	</nav>`;
}

export function resultSummary(search: Search, results: IndexedSearchResults): string {
	return `${countLabel(results.total, 'matching project')}${search.query ? ` for “${search.query}”` : ''}`;
}

export function renderResults(search: Search, results: IndexedSearchResults, channels: BrowserChannel[]): string {
	const labels = new Map(channels.map((channel) => [channel.name, channel.label]));
	const channelFilters = [{ value: '', label: 'all channels' },
		...channels.map((channel) => ({ value: channel.name, label: channel.label }))];
	const initialFilters = [{ value: '', label: 'all' },
		...results.initials.map((initial) => ({ value: initial, label: initial }))];
	return `<section class='page-heading search-heading'>
		<h1>${search.query ? 'search results' : 'browse'}</h1>${searchForm(search)}
		<p class='search-hint' id='search-hint'>Leave blank to browse.</p>
	</section><div class='content-layout'><section class='search-results' aria-labelledby='results-title'>
		<div class='results-heading'><h2 id='results-title'>
			${countLabel(results.total, 'matching project')}
			${search.query ? `for <code>“${escapeHtml(search.query)}”</code>` : ''}</h2>
			${search.query || search.channel || search.initial ?
				link('/?q=', 'reset search', 'class=\'text-link\'') : ''}</div>
		<nav class='channel-filter' aria-label='Filter matches by channel'>
			${filterLinks(search, channelFilters, 'channel')}</nav>
		${filterNotices(search, labels)}
		<nav class='initial-filter' aria-label='Filter projects by initial'>
			${filterLinks(search, initialFilters, 'initial')}</nav>
		${results.rows.length ? `<ul class='name-results' role='list' aria-label='Matching projects'>
			${results.rows.map((project, index) => projectResult(project, index + 1, search, labels)).join('')}
			</ul>${results.pages > 1 ? pagination(search, results) : ''}` : emptyResults(search)}
	</section><aside class='side-column'><section class='side-note'>
		<h2>search notes</h2><p>Matches parts of executable names and project IDs, not package names.</p>
	</section><section class='side-note'><h2>coverage</h2><p>An unlisted command may still be available.</p>
		${link('/dataset#coverage', 'dataset limits <span aria-hidden=\'true\'>→</span>', 'class=\'text-link\'')}
	</section></aside></div>`;
}

export function renderMessage(title: string, message: string, options: {
	canRetry?: boolean;
	search?: SearchFields;
} = {}): string {
	return `<section class='error-page'><h1>${escapeHtml(title)}</h1><p class='lead'>${escapeHtml(message)}</p>
		${options.canRetry ? '<button type=\'button\' class=\'button-link\' data-retry-search>retry</button>' : ''}
		${searchForm(options.search)}
		<p class='search-hint' id='search-hint'>Search by full or partial name.
			${link('/?q=', 'browse names →')}</p></section>`;
}
