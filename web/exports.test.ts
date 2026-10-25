import { expect, mock, spyOn, test } from 'bun:test';

import { formatJsonExport, mountJsonExport } from './exports';
import type { ProviderExport } from './types';

const payload: ProviderExport = {
	dataset_release: 'bnck:release:test',
	query: { executable: 'age' },
	providers: [{ project: 'age', executable: 'age', channel: 'debian', package: 'age' }],
	associations: [],
};
const formattedJson = JSON.stringify(payload, null, '\t') + '\n';

class FakeElement {
	tagName = '';
	className = '';
	textContent = '';
	hidden = false;
	disabled = false;
	open = false;
	children: FakeElement[] = [];
	afterNode: FakeElement | null = null;
	listeners: Record<string, () => unknown> = {};
	classes = new Set<string>();

	constructor(tagName = '') {
		this.tagName = tagName;
	}

	get classList() {
		return {
			add: (name: string) => this.classes.add(name),
			remove: (name: string) => this.classes.delete(name),
			contains: (name: string) => this.classes.has(name),
		};
	}

	addEventListener(event: string, handler: () => unknown) {
		this.listeners[event] = handler;
	}

	append(...children: FakeElement[]) {
		this.children.push(...children);
	}

	after(node: FakeElement) {
		this.afterNode = node;
	}
}

function initialize({ isSecureContext = true, writeText = mock(async () => {}) } = {}) {
	const button = new FakeElement('button');
	button.hidden = true;
	button.textContent = 'copy JSON';
	const view = new FakeElement('button');
	view.textContent = 'view JSON';
	const status = new FakeElement('p');
	const elements = new Map<string, FakeElement>([
		['[data-copy-json]', button], ['[data-view-json]', view], ['[role="status"]', status],
	]);
	const actions = new FakeElement('div');
	actions.querySelector = (selector: string) => elements.get(selector) ?? null;
	const root = { querySelector: (selector: string) => selector === '[data-json-export]' ? actions : null };
	const created: FakeElement[] = [];
	const previous = {
		document: globalThis.document,
		window: (globalThis as { window?: unknown }).window,
		navigator: globalThis.navigator,
	};
	Object.assign(globalThis, {
		document: { createElement: (tagName: string) => {
			const element = new FakeElement(tagName);
			created.push(element);
			return element;
		} },
		window: { isSecureContext },
		navigator: { clipboard: { writeText } },
	});
	const timeout = spyOn(globalThis, 'setTimeout').mockImplementation(() => 0 as never);
	mountJsonExport(root as unknown as ParentNode, payload);
	return {
		button, view, status, actions, created, writeText,
		restore: () => {
			timeout.mockRestore();
			Object.assign(globalThis, previous);
		},
	};
}


test('formats an export with tab indentation and a trailing newline', () => {
	expect(formatJsonExport(payload)).toBe(formattedJson);
});

test('copy remains hidden without a secure context', () => {
	const state = initialize({ isSecureContext: false });
	try {
		expect(state.button.hidden).toBe(true);
	} finally {
		state.restore();
	}
});

test('copy writes the formatted export through writeText', async () => {
	const state = initialize();
	try {
		expect(state.button.hidden).toBe(false);
		await state.button.listeners.click();
		expect(state.writeText).toHaveBeenCalledWith(formattedJson);
		expect(state.status.textContent).toBe('JSON copied.');
		expect(state.button.textContent).toBe('JSON copied');
		expect(state.button.disabled).toBe(false);
	} finally {
		state.restore();
	}
});

test('copy failure is reported and the button is restored', async () => {
	const state = initialize({ writeText: mock(async () => { throw new Error('Clipboard denied'); }) });
	try {
		await state.button.listeners.click();
		expect(state.status.textContent).toBe('Copy failed. Use view JSON to copy manually.');
		expect(state.status.classList.contains('visually-hidden')).toBe(false);
		expect(state.button.textContent).toBe('copy JSON');
		expect(state.button.disabled).toBe(false);
	} finally {
		state.restore();
	}
});

test('view JSON inserts and toggles a preformatted export', () => {
	const state = initialize();
	try {
		state.view.listeners.click();
		expect(state.created).toHaveLength(3);
		const [details, summary, pre] = state.created;
		expect(details.tagName).toBe('details');
		expect(details.open).toBe(true);
		expect(summary.textContent).toBe('JSON export');
		expect(pre.textContent).toBe(formattedJson);
		expect(details.children).toEqual([summary, pre]);
		expect(state.actions.afterNode).toBe(details);
		state.view.listeners.click();
		expect(details.open).toBe(false);
		expect(state.created).toHaveLength(3);
	} finally {
		state.restore();
	}
});
