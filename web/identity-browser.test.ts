import { expect, mock, test } from 'bun:test';

import fixture from './fixtures/dataset.json';
import { mountIdentity } from './identity';
import { buildIndex } from './query';
import type { BrowserDataset } from './types';

const dataset = fixture as unknown as BrowserDataset;
const loaded = { dataset, index: buildIndex(dataset) };
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

function initialize(pathname: string, loadDataset = mock(async () => loaded), hash = '') {
	const attributes = new Map<string, string>();
	const retry = { addEventListener: mock(() => {}) };
	const main = {
		innerHTML: '',
		setAttribute: (key: string, value: string) => attributes.set(key, value),
		removeAttribute: (key: string) => attributes.delete(key),
		querySelector: (selector: string) => selector === '[data-retry-identity]' ? retry : null,
	};
	const status = { textContent: '' };
	const target = { scrollIntoView: mock(() => {}) };
	const document = {
		title: '',
		querySelector: (selector: string) =>
			({ '#main': main, '#identity-status': status })[selector as '#main' | '#identity-status'] ?? null,
		getElementById: (id: string) => id === 'executable-hx' ? target : null,
	};
	const window = { location: { href: `https://bnck.test${pathname}${hash}`, pathname, hash } };
	return {
		main, status, document, retry, target, attributes, loadDataset,
		ready: mountIdentity(window, document, loadDataset),
	};
}

test('mounts a project route from the shared dataset', async () => {
	const state = initialize('/projects/helix');
	await state.ready;
	expect(state.main.innerHTML).toContain("class='matrix-legend'");
	expect(state.main.innerHTML).toContain("href='https://helix-editor.com/'");
	expect(state.status.textContent).toBe('project helix');
	expect(state.document.title).toBe('project: helix · bnck(1)');
	expect(state.attributes.has('aria-busy')).toBe(false);
});

test('mounts an executable route with every owning project', async () => {
	const state = initialize('/executables/hx');
	await state.ready;
	expect(state.main.innerHTML).toContain('<strong>Multiple projects provide this name.</strong>');
	expect(state.main.innerHTML).toContain("href='/projects/helix'");
	expect(state.main.innerHTML).toContain("href='/projects/other'");
	expect(state.status.textContent).toBe('executable hx');
});

test('project anchors reveal the linked executable after rendering', async () => {
	const state = initialize('/projects/helix', undefined, '#executable-hx');
	await state.ready;
	expect(state.target.scrollIntoView).toHaveBeenCalledTimes(1);
});

test('unknown identities render a searchable not-found page', async () => {
	const state = initialize('/projects/missing');
	await state.ready;
	expect(state.main.innerHTML).toContain('not listed in this dataset release');
	expect(state.main.innerHTML).toContain("class='search-form'");
	expect(state.status.textContent).toBe('project not listed');
});

test('invalid identity paths render an explicit error without loading the dataset', async () => {
	const state = initialize('/projects/%zz');
	await state.ready;
	expect(state.main.innerHTML).toContain('page not found');
	expect(state.loadDataset).not.toHaveBeenCalled();
});

test('load failures offer a retry that can render the identity', async () => {
	let attempts = 0;
	const loadDataset = mock(async () => {
		if (++attempts === 1) throw new Error('Offline');
		return loaded;
	});
	const state = initialize('/executables/hx', loadDataset);
	await state.ready;
	expect(state.main.innerHTML).toContain('details unavailable');
	expect(state.main.innerHTML).toContain('Offline');
	expect(state.retry.addEventListener).toHaveBeenCalled();
	const handler = state.retry.addEventListener.mock.calls[0][1];
	handler();
	await tick();
	expect(state.main.innerHTML).toContain('Multiple projects provide this name.');
	expect(loadDataset).toHaveBeenCalledTimes(2);
});
