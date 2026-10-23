import assert from 'node:assert/strict';
import { buildIndex, parseSearch, queryIndex, searchUrl } from './query.js';

/* Rust supplies real SQLite results across queries, channels, initials, and page boundaries. */
const { dataset, cases } = await Bun.stdin.json();
const index = buildIndex(dataset);
for (const { url, results } of cases) {
	const search = parseSearch(new URL(url, 'https://bnck.test').searchParams, dataset.channels);
	if (search.page > 1 && !results.rows.length) {
		assert.throws(() => queryIndex(index, search), /No results on this page/, url);
		continue;
	}
	const actual = queryIndex(index, search);
	assert.deepEqual({
		...actual,
		initials: undefined,
		initial_links: ['', ...actual.initials].map((initial) => ({
			label: initial || 'all', url: searchUrl(search, { initial, page: 1 }), is_current: initial === search.initial,
		})),
		rows: actual.rows.map((row) => ({
			name: row.name, related: row.related.map(({ name, channels }) => ({ name, channels })),
		})),
	}, { ...results, initials: undefined }, url);
}
