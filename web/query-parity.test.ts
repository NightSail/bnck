import { expect, test } from 'bun:test';

import { WebsiteDatabase, searchPageUrl } from './database';
import type { SearchQuery } from './types';
import { browserDataset } from './data';
import { buildIndex, parseSearch, queryIndex, searchUrl } from './query';
import { createFixture, provider } from './test-fixture';

test('browser queries match the SQLite search contract', async () => {
	const records = [
		provider('helix', 'hx', 'debian'),
		provider('helix', 'hx', 'crates.io'),
		provider('other', 'hx', 'npm'),
		provider('hx', 'hx-admin', 'debian'),
		provider('case', 'HX', 'debian'),
		provider('literal', 'tool%_', 'npm'),
		provider('unicode', 'étool', 'pypi'),
		provider('escaping', "<tool>&'", 'npm'),
		provider('age', 'age', 'debian'),
		provider('age', 'age-inspect', 'debian'),
		provider('age', 'age-keygen', 'debian'),
		provider('age', 'age-plugin-batchpass', 'npm'),
		provider('age-plugin-yubikey', 'age-plugin-yubikey', 'debian'),
		provider('0tool', 'lookup', 'npm'),
		provider('.tool', 'lookup', 'npm'),
		provider('tool-prefix', 'unrelated', 'npm'),
		provider('short-prefix', 'tool-x', 'npm'),
		provider('substring', 'xtool', 'npm'),
		provider('substring', 'tool-long-prefix', 'npm'),
		provider('unicode', 'tool\u{e000}', 'npm'),
		provider('unicode', 'tool\u{10000}', 'npm'),
		...Array.from({ length: 120 }, (_, index) => provider(`page-${String(index).padStart(3, '0')}`, 'tool', 'npm')),
	];
	const fixture = await createFixture(records);
	try {
		const database = await WebsiteDatabase.open(fixture.path);
		try {
			const dataset = browserDataset(database, database.listProjects());
			const index = buildIndex(dataset);
			for (const query of ['', 'hx', 'HX', 'age', 'tool', '%_', "<tool>&'", 'É', 'é', '\u{85}hx\u{85}', '\u{feff}hx']) {
				for (const channel of ['', 'npm', 'debian', 'arch']) {
					for (const initial of ['', '#', 'A']) {
						for (const page of [1, 2, 3, 4, 5]) {
							const search: SearchQuery = { q: query, channel, initial, page };
							const expected = database.search(search);
							const parsed = parseSearch(
								new URL(searchPageUrl(search, page), 'https://bnck.test').searchParams,
								dataset.channels,
							);
							const label = `${JSON.stringify({ query, channel, initial, page })}`;
							if (page > 1 && !expected.rows.length) {
								expect(() => queryIndex(index, parsed), label).toThrow('No results on this page');
								continue;
							}
							const actual = queryIndex(index, parsed);
							expect({
								...actual,
								initials: undefined,
								initial_links: ['', ...actual.initials].map((value) => ({
									label: value || 'all',
									url: searchUrl(parsed, { initial: value, page: 1 }),
									is_current: value === parsed.initial,
								})),
								rows: actual.rows.map((row) => ({
									name: row.name,
									related: row.related.map(({ name, channels }) => ({ name, channels })),
								})),
							}, label).toEqual({ ...expected, initials: undefined });
						}
					}
				}
			}
		} finally {
			database.close();
		}
	} finally {
		await fixture.cleanup();
	}
});
