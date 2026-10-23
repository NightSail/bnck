import { expect, mock, test } from 'bun:test';
import dataset from './fixtures/dataset.json';
import { mountSearch } from './browser.js';
import { buildIndex } from './query.js';

const loaded = { dataset, index: buildIndex(dataset) };
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

function initialize(path = '/', loadDataset = mock(async () => loaded)) {
	const documentHandlers = {};
	const windowHandlers = {};
	const attributes = new Map();
	const main = {
		innerHTML: '<h1>Executable lookup</h1>', focus: mock(() => {}),
		setAttribute: (key, value) => attributes.set(key, value), removeAttribute: (key) => attributes.delete(key),
	};
	const status = { textContent: '' };
	const input = { setAttribute: mock(() => {}) };
	const document = {
		title: 'Home · bnck(1)',
		querySelector: (selector) => ({ '#main': main, '#search-status': status, '#q': input })[selector],
		addEventListener: (type, handler) => { documentHandlers[type] = handler; },
	};
	const window = {
		location: new URL(path, 'https://bnck.test'),
		scrollTo: mock(() => {}),
		addEventListener: (type, handler) => { windowHandlers[type] = handler; },
		history: { pushState: mock((_state, _title, url) => { window.location = new URL(url); }) },
	};
	const ready = mountSearch(window, document, loadDataset);
	return { main, status, attributes, document, window, documentHandlers, windowHandlers, ready, loadDataset };
}

function click(state, path, overrides = {}) {
	const anchor = {
		href: new URL(path, state.window.location).href, target: '', rel: '', hasAttribute: () => false,
	};
	const event = {
		button: 0, defaultPrevented: false, preventDefault: mock(() => {}),
		target: { closest: (selector) => selector === 'a[href]' ? anchor : null }, ...overrides,
	};
	state.documentHandlers.click(event);
	return event;
}

test('home remains pre-rendered without fetching the dataset', async () => {
	const state = initialize('/?utm_source=terminal');
	await state.ready;
	expect(state.main.innerHTML).toContain('Executable lookup');
	expect(state.loadDataset).not.toHaveBeenCalled();
	expect(state.main.focus).not.toHaveBeenCalled();
});

test('bookmarked searches render results, update the title, and announce totals', async () => {
	const state = initialize('/?q=hx&channel=npm');
	await state.ready;
	expect(state.main.innerHTML).toContain('Search results');
	expect(state.main.innerHTML).toContain("value='hx'");
	expect(state.document.title).toBe('Search: hx · bnck(1)');
	expect(state.status.textContent).toBe('2 matching projects for “hx”');
	expect(state.attributes.has('aria-busy')).toBe(false);
	expect(state.window.history.pushState).not.toHaveBeenCalled();
});

test('ordinary search links update history; Back and Forward restore URL state', async () => {
	const state = initialize();
	await state.ready;
	const event = click(state, '/?q=hx');
	await tick();
	expect(event.preventDefault).toHaveBeenCalled();
	expect(state.window.location.search).toBe('?q=hx');
	expect(state.window.history.pushState).toHaveBeenCalledTimes(1);
	expect(state.main.focus).toHaveBeenCalled();
	state.window.location = new URL('https://bnck.test/');
	state.windowHandlers.popstate();
	await tick();
	expect(state.main.innerHTML).toContain('Executable lookup');
	state.window.location = new URL('https://bnck.test/?q=hx');
	state.windowHandlers.popstate();
	await tick();
	expect(state.main.innerHTML).toContain('Search results');
	expect(state.window.history.pushState).toHaveBeenCalledTimes(1);
});

test('modified clicks, fragments, detail pages, and external links retain native navigation', async () => {
	const state = initialize();
	await state.ready;
	for (const overrides of [{ ctrlKey: true }, { metaKey: true }, { shiftKey: true },
		{ altKey: true }, { button: 1 }, { defaultPrevented: true }]) {
		expect(click(state, '/?q=hx', overrides).preventDefault).not.toHaveBeenCalled();
	}
	for (const path of ['/projects/helix', '/about', '/dataset', '/#main', 'https://example.org/?q=hx']) {
		expect(click(state, path).preventDefault).not.toHaveBeenCalled();
	}
	expect(state.window.history.pushState).not.toHaveBeenCalled();
});

test('a stale fetch cannot replace a newer home navigation', async () => {
	const deferred = Promise.withResolvers();
	const state = initialize('/?q=hx', () => deferred.promise);
	expect(state.attributes.get('aria-busy')).toBe('true');
	click(state, '/');
	deferred.resolve(loaded);
	await state.ready;
	expect(state.main.innerHTML).toContain('Executable lookup');
	expect(state.main.innerHTML).not.toContain('Search results');
	expect(state.attributes.has('aria-busy')).toBe(false);
});

test('failed loads show an explicit retry action and retry without adding history', async () => {
	let attempts = 0;
	const state = initialize('/?q=hx', async () => {
		if (++attempts === 1) throw new Error('Offline');
		return loaded;
	});
	await state.ready;
	expect(state.main.innerHTML).toContain('Search unavailable');
	expect(state.main.innerHTML).toContain('data-retry-search');
	expect(state.status.textContent).toContain('Offline');
	state.documentHandlers.click({ button: 0, target: {
		closest: (selector) => selector === '[data-retry-search]' ? {} : null,
	} });
	await tick();
	expect(state.main.innerHTML).toContain('Search results');
	expect(state.window.history.pushState).not.toHaveBeenCalled();
});

test('invalid searches and out-of-range pages show errors rather than misleading empty results', async () => {
	for (const path of ['/?page=0', '/?q=hx&page=2', '/?channel=unknown', '/?q=a&q=b']) {
		const state = initialize(path);
		await state.ready;
		expect(state.main.innerHTML).toContain('Invalid search');
		expect(state.main.innerHTML).not.toContain('No matches');
		expect(state.main.innerHTML).not.toContain('data-retry-search');
	}
});
