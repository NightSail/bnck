import { PROJECT_MATRIX_CELL_LIMIT, channelLabel } from './config';
import { denseProjectMatrix } from './data';
import { encodeSegment, formatNumber, foldAscii } from './naming';
import { html, Html } from './html';
import type {
	ChannelGroup,
	DatasetStats,
	PackageReference,
	ProjectCoverage,
	ProjectMatrix,
	SearchRelation,
	SearchResult,
} from './types';

export function countLabel(count: number, singular: string, plural = `${singular}s`): string {
	return `${formatNumber(count)} ${count === 1 ? singular : plural}`;
}

export function searchForm(
	search: Partial<{ query: string; channel: string; initial: string }> = {},
): Html {
	const query = search.query ?? '';
	const channel = search.channel ?? '';
	const initial = search.initial ?? '';
	return html`<form class='search-form' action='/' method='get' role='search'>
	<label class='visually-hidden' for='q'>executable name or project ID</label>
	<div class='search-input'>
		<span class='search-symbol' aria-hidden='true'>⌕</span>
		<input id='q' name='q' type='search' value='${query}' placeholder='press / to'
			autocomplete='off' spellcheck='false' aria-describedby='search-hint'>
		<button type='submit'>search <span aria-hidden='true'>↵</span></button>
	</div>
	${channel ? html`<input type='hidden' name='channel' value='${channel}'>` : ''}
	${initial ? html`<input type='hidden' name='initial' value='${initial}'>` : ''}
</form>`;
}

export function stats(values: DatasetStats): Html {
	return html`<dl class='stats' aria-label='Current dataset statistics'>
	<div><dt>executable names</dt><dd>${formatNumber(values.executables)}</dd></div>
	<div><dt>projects</dt><dd>${formatNumber(values.projects)}</dd></div>
	<div><dt>channels</dt><dd>${formatNumber(values.channels)}</dd></div>
</dl>`;
}

export function packageLink(packageReference: PackageReference, channel: string): Html {
	if (!packageReference.url) return html`<code>${packageReference.name}</code>`;
	return html`<a class='package-link' href='${packageReference.url}' rel='external'
		aria-label='${packageReference.name} on ${channelLabel(channel)} (external package page)'>
		<code>${packageReference.name}</code><span aria-hidden='true'>↗</span>
	</a>`;
}

export function projectCoverage(project: string, coverage: ProjectCoverage): Html {
	if (!coverage.channels.length) {
		return html`<p class='detail-note'>No current channel packages are curated for this project.</p>`;
	}
	if (coverage.cells <= PROJECT_MATRIX_CELL_LIMIT) {
		return coverageMatrix(project, denseProjectMatrix(coverage));
	}
	return coveragePackages(project, coverage);
}

function coverageMatrix(project: string, matrix: ProjectMatrix): Html {
	return html`<p class='matrix-legend' id='matrix-legend'>
	<span class='matrix-observed' aria-hidden='true'>✓</span> Observed in this package.
	<span aria-hidden='true'>-</span> Not observed in this dataset.
</p>
<div class='matrix-scroll' role='region' aria-labelledby='providers-title' tabindex='0'>
	<table class='provider-matrix' aria-describedby='matrix-legend'>
		<caption class='visually-hidden'>executable coverage for ${project}</caption>
		<thead>
			<tr>
				<th class='matrix-name' scope='col'>package</th>
				${matrix.executables.map((executable) => html`<th class='matrix-executable' scope='col'
					id='executable-${executable}'>
					<a class='related-name' href='/executables/${encodeSegment(executable)}'>
						<code>${executable}</code>
					</a>
				</th>`)}
			</tr>
		</thead>
		<tbody>
			${matrix.channels.map((channel) => html`<tr>
				<th class='matrix-channel' scope='rowgroup' colspan='${matrix.executables.length + 1}'>
					<span class='matrix-channel-label'>${channelLabel(channel.channel)}</span>
				</th>
			</tr>
			${channel.packages.map((packageReference) => html`<tr>
				<th class='matrix-name' scope='row'>${packageLink(packageReference.reference, channel.channel)}</th>
				${packageReference.observed.map((isObserved) => html`<td>
					${isObserved
						? html`<span class='matrix-observed' aria-hidden='true'>✓</span>
							<span class='visually-hidden'>observed</span>`
						: html`<span class='muted' aria-hidden='true'>-</span>
							<span class='visually-hidden'>not observed</span>`}
				</td>`)}
			</tr>`)}`)}
		</tbody>
	</table>
</div>`;
}

function coveragePackages(project: string, coverage: ProjectCoverage): Html {
	const packageCount = coverage.channels.reduce((count, channel) => count + channel.packages.length, 0);
	return html`<p class='matrix-legend' id='matrix-legend'>
	Each package lists the executable names observed in it. A name absent from a package is not observed in this
	dataset.
</p>
<p class='coverage-note'>This project has ${countLabel(coverage.executables.length, 'executable')} across
	${countLabel(packageCount, 'package')}. Coverage is listed by package instead of as a single matrix.</p>
<section class='executable-index' aria-labelledby='executable-index-title'>
	<h3 id='executable-index-title'>executables</h3>
	<ul class='executable-index-list'>
		${coverage.executables.map((executable) => html`<li>
			<a class='related-name' id='executable-${executable}'
				href='/executables/${encodeSegment(executable)}'><code>${executable}</code></a>
		</li>`)}
	</ul>
</section>
<div class='coverage-scroll' role='region' aria-labelledby='providers-title' tabindex='0'>
	<table class='provider-coverage' aria-describedby='matrix-legend'>
		<caption class='visually-hidden'>executable coverage for ${project}</caption>
		<thead>
			<tr><th scope='col'>package</th><th scope='col'>observed executables</th></tr>
		</thead>
		<tbody>
			${coverage.channels.map((channel) => html`<tr>
				<th class='coverage-channel' scope='rowgroup' colspan='2'>
					<span class='matrix-channel-label'>${channelLabel(channel.channel)}</span>
				</th>
			</tr>
			${channel.packages.map((packageReference) => html`<tr>
				<th class='coverage-package' scope='row'>
					${packageLink(packageReference.reference, channel.channel)}
				</th>
				<td>
					<ul class='coverage-executables'>
						${packageReference.executables.map((executable) => html`<li>
							<a class='related-name' href='/executables/${encodeSegment(executable)}'><code>${executable}</code></a>
						</li>`)}
					</ul>
				</td>
			</tr>`)}`)}
		</tbody>
	</table>
</div>`;
}

export function providerGroup(name: string, kind: 'project' | 'executable', channels: ChannelGroup[]): Html {
	return html`<article class='provider-group' id='${kind}-${name}'>
	<header class='provider-group-header'>
		<div>
			<span class='eyebrow'>${kind}</span>
			<h3>
				<a href='/${kind}s/${encodeSegment(name)}'>
					<code>${name}</code> <span aria-hidden='true'>↗</span>
				</a>
			</h3>
		</div>
		<span class='badge'>${countLabel(channels.length, 'channel')}</span>
	</header>
	<table class='provider-table'>
		<caption class='visually-hidden'>distribution packages for ${name}</caption>
		<thead><tr><th scope='col'>channel</th><th scope='col'>package</th></tr></thead>
		<tbody>
			${channels.map((channel) => html`<tr>
				<th scope='row'>
					<span class='channel-mark' aria-hidden='true'></span>${channelLabel(channel.channel)}
				</th>
				<td>
					<ul class='package-list'>
						${channel.packages.map((packageReference) => html`<li>${
							packageLink(packageReference, channel.channel)
						}</li>`)}
					</ul>
				</td>
			</tr>`)}
		</tbody>
	</table>
</article>`;
}

export function associationList(
	items: { name: string; role: string }[],
	kind: 'project' | 'executable',
): Html {
	const base = kind === 'project' ? 'projects' : 'executables';
	return html`<ul class='association-list'>
		${items.map((item) => html`<li>
			<a href='/${base}/${encodeSegment(item.name)}'><code>${item.name}</code></a>
			<span class='badge'>${item.role}</span>
		</li>`)}
	</ul>`;
}

export function exportActions(url: string): Html {
	return html`<div class='export-actions' data-json-export>
	<div class='export-buttons'>
		<button class='text-link copy-json' type='button' data-copy-json hidden>copy JSON</button>
		<a class='text-link' href='${url}' data-view-json>view JSON</a>
	</div>
	<p class='export-status visually-hidden' role='status' aria-live='polite'></p>
</div>`;
}

export function snapshotNote(): Html {
	return html`<section class='side-note'>
	<h2>coverage</h2>
	<p>An unlisted command may still be available.</p>
	<a class='text-link' href='/dataset#coverage'>dataset limits <span aria-hidden='true'>→</span></a>
</section>`;
}

export function coverageNote(): Html {
	return html`<p class='detail-note'>An unlisted command may still be available.
	<a class='text-link' href='/dataset#coverage'>dataset limits <span aria-hidden='true'>→</span></a></p>`;
}

interface HighlightPart {
	text: string;
	isMatch: boolean;
}

function highlightParts(name: string, query: string): HighlightPart[] {
	if (!query) return [{ text: name, isMatch: false }];
	const foldedName = foldAscii(name);
	const foldedQuery = foldAscii(query);
	const parts: HighlightPart[] = [];
	let offset = 0;
	for (let start = foldedName.indexOf(foldedQuery); start !== -1; start = foldedName.indexOf(foldedQuery, offset)) {
		if (start > offset) parts.push({ text: name.slice(offset, start), isMatch: false });
		offset = start + foldedQuery.length;
		parts.push({ text: name.slice(start, offset), isMatch: true });
	}
	if (offset < name.length) parts.push({ text: name.slice(offset), isMatch: false });
	return parts;
}

export function highlightedName(name: string, query = ''): Html {
	return html`${highlightParts(name, query).map((part) =>
		part.isMatch ? html`<mark>${part.text}</mark>` : part.text)}`;
}

export function relatedRows(
	result: SearchResult,
	relations: SearchRelation[],
	query = '',
	selectedChannel = '',
): Html {
	return html`<ul class='related-rows' role='list'>
	${relations.map((relation) => {
		const url = relation.role
			? `/executables/${encodeSegment(relation.name)}`
			: `/projects/${encodeSegment(result.name)}#executable-${encodeSegment(relation.name)}`;
		const channels = relation.role
			? html` <span class='badge'>${relation.role}</span>`
			: selectedChannel ? '' : html`: ${relation.channels.map((channel, index) =>
				html`${index > 0 ? ', ' : ''}${channelLabel(channel)}`)}`;
		return html`<li>
			<a class='related-name' href='${url}'>
				<code>${highlightedName(relation.name, query)}(1)</code></a>${channels}
			${query && relation.name === query ? html`<span class='exact-match'>exact match</span>` : ''}
		</li>`;
	})}
</ul>`;
}

export function searchResult(
	result: SearchResult,
	headingId: string,
	limit: number,
	query = '',
	moreUrl: string | null = null,
	selectedChannel = '',
): Html {
	const remaining = result.related.length - limit;
	return html`<article class='name-result' aria-labelledby='${headingId}'>
	<header class='name-result-heading'>
		<h3 id='${headingId}'>
			<a href='/projects/${encodeSegment(result.name)}'>
				<code>${highlightedName(result.name, query)}</code></a>
			${query && result.name === query ? html`<span class='exact-match'>exact match</span>` : ''}
		</h3>
	</header>
	${relatedRows(result, result.related.slice(0, limit), query, selectedChannel)}
	${remaining > 0 ? (moreUrl
		? html`<a class='more-related text-link' href='${moreUrl}'>
			${countLabel(remaining, 'more executable', 'more executables')}</a>`
		: html`<details class='more-related'>
			<summary>${countLabel(remaining, 'more executable', 'more executables')}
				<span class='visually-hidden'> for ${result.name}</span></summary>
			${relatedRows(result, result.related.slice(limit), query, selectedChannel)}
		</details>`) : ''}
</article>`;
}

