import { expect, test } from 'bun:test';
import dataset from './fixtures/dataset.json';
import { buildIndex, compareNames, encodeSegment, foldAscii, parseSearch, queryIndex, searchUrl } from './query.js';
import { escapeHtml, highlightName, renderResults } from './results.js';

const parse = (query) => parseSearch(new URLSearchParams(query), dataset.channels);
const query = (parameters) => queryIndex(buildIndex(dataset), parse(parameters));

test('home and explicit browsing remain distinct', () => {
	expect(parse('').isHome).toBe(true);
	expect(parse('page=1&channel=&initial=').isHome).toBe(true);
	expect(parse('utm_source=terminal').isHome).toBe(true);
	for (const parameters of ['q=', 'channel=npm', 'initial=A', 'page=2']) {
		expect(parse(parameters).isHome).toBe(false);
	}
});

test('query validation preserves the Rust whitespace and Unicode length rules', () => {
	expect(parse(`q=${encodeURIComponent('\u0085 hx \u0085')}`).query).toBe('hx');
	expect(parse(`q=${encodeURIComponent('\ufeffhx')}`).query).toBe('\ufeffhx');
	expect(parse(`q=${'😀'.repeat(100)}`).query).toHaveLength(200);
	expect(() => parse(`q=${'😀'.repeat(101)}`)).toThrow('100 characters');
	for (const parameters of [
		'q=a&q=b', 'page=1&page=2', 'channel=npm&channel=debian', 'initial=A&initial=B',
		'page=0', 'page=-1', 'page=1.5', 'page=1e2', 'page=', 'page=4294967296',
		'channel=unknown', 'initial=a', 'initial=AB', 'initial=É',
	]) expect(() => parse(parameters)).toThrow();
	expect(parse('page=0002').page).toBe(2);
});

test('folding and ordering are ASCII-only and Unicode scalar based', () => {
	expect(foldAscii('HXÉİ')).toBe('hxÉİ');
	expect(compareNames('\ue000', '𐀀')).toBeLessThan(0);
	expect(compareNames('HX', 'hx')).toBeLessThan(0);
	expect(compareNames('hx', 'hx')).toBe(0);
});

test('URLs encode every reserved character and preserve filters', () => {
	expect(encodeSegment("é%&'<>#?/+")).toBe('%C3%A9%25%26%27%3C%3E%23%3F%2F%2B');
	const url = searchUrl(parse('q=hx&channel=npm&initial=H&page=2'), { channel: '', page: 1 });
	expect(url).toBe('/?q=hx&channel=&initial=H&page=1');
});

test('search groups shared names under each owner and deduplicates channels', () => {
	const results = query('q=hx');
	expect(results.rows.map((row) => row.name)).toEqual(['helix', 'hx', 'other']);
	expect(results.rows[0].related.map((row) => row.name)).toEqual(['hx', 'helper']);
	expect(results.rows[0].related[0].channels).toEqual(['debian', 'npm']);
	expect(results.total).toBe(3);
	expect(query('q=@scope').total).toBe(0);
});

test('channel filtering applies to group membership and all related rows', () => {
	const results = query('q=hx&channel=npm');
	expect(results.rows.map((row) => row.name)).toEqual(['helix', 'other']);
	expect(results.rows[0].related.map((row) => row.name)).toEqual(['hx']);
	expect(results.rows[0].related[0].channels).toEqual(['npm']);
	expect(query('q=&channel=arch').total).toBe(0);
});

test('initial choices are computed before filtering and pagination', () => {
	const results = query('q=hx&initial=H');
	expect(results.initials).toEqual(['H', 'O']);
	expect(results.rows.map((row) => row.name)).toEqual(['helix', 'hx']);
	expect(query('q=hx&initial=A').initials).toEqual(['H', 'O']);
});

test('pagination counts whole projects, with out-of-range pages rejected', () => {
	const expanded = { ...dataset, projects: Array.from({ length: 65 }, (_, index) => ({
		...dataset.projects[0], name: `project-${String(index).padStart(2, '0')}`,
	})) };
	const index = buildIndex(expanded);
	const first = queryIndex(index, parse('q='));
	const second = queryIndex(index, parse('q=&page=2'));
	const last = queryIndex(index, parse('q=&page=3'));
	expect([first.rows.length, second.rows.length, last.rows.length]).toEqual([30, 30, 5]);
	expect([second.start, second.end, second.total]).toEqual([31, 60, 65]);
	expect(second.previous).toContain('page=1');
	expect(second.next).toContain('page=3');
	expect(last.next).toBe(null);
	expect(() => queryIndex(index, parse('q=&page=4'))).toThrow('No results on this page');
});

test('highlights preserve literal spelling, escaping, and nonoverlapping matches', () => {
	expect(highlightName('HxHXhx', 'hx')).toBe('<mark>Hx</mark><mark>HX</mark><mark>hx</mark>');
	expect(highlightName('aaaaa', 'aa')).toBe('<mark>aa</mark><mark>aa</mark>a');
	expect(highlightName('<script>&', '<script>')).toBe('<mark>&lt;script&gt;</mark>&amp;');
	expect(highlightName('éÉ', 'é')).toBe('<mark>é</mark>É');
	expect(escapeHtml("\"'&<>")).toBe('&quot;&#39;&amp;&lt;&gt;');
});

test('rendering preserves exact tags, labels, and filter recovery links', () => {
	const search = parse('q=hx');
	const html = renderResults(search, query('q=hx'), dataset.channels);
	expect(html.match(/class='exact-match'/g)).toHaveLength(3);
	expect(html).toContain("href='/projects/helix#executable-hx'");
	expect(html).toContain('Debian, npm');
	expect(html).toContain('Arch Linux');
	const filtered = renderResults(parse('q=hx&channel=npm'), query('q=hx&channel=npm'), dataset.channels);
	expect(filtered).not.toContain('Debian, npm');
	const empty = renderResults(parse('q=hx&channel=npm&initial=A'),
		query('q=hx&channel=npm&initial=A'), dataset.channels);
	expect(empty).toContain('Clear both filters');
	expect(empty).toContain('Search all channels');
	expect(empty).toContain('Search all initials');
});

test('untrusted dataset names and queries cannot inject HTML or link attributes', () => {
	const malicious = { ...dataset, projects: [{ name: "<img src=x onerror='bad'>", providers: [
		{ executable: "<script>alert('x')</script>", channel: 'npm', package: 'safe' },
	] }] };
	const search = parse('q=script');
	const html = renderResults(search, queryIndex(buildIndex(malicious), search), dataset.channels);
	expect(html).not.toContain('<img');
	expect(html).not.toContain('<script>');
	expect(html).toContain('%3Cimg%20src%3Dx%20onerror%3D%27bad%27%3E');
	expect(html).toContain('&lt;');
});
