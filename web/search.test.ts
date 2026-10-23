import { expect, mock, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = new Bun.Transpiler({ loader: 'ts' }).transformSync(
	readFileSync(new URL('./search.ts', import.meta.url), 'utf8'),
);

class FakeElement {
	constructor(tag = 'div') {
		this.tag = tag;
		this.isContentEditable = false;
	}

	matches(selector) {
		expect(selector).toBe('input, textarea, select');
		return ['input', 'textarea', 'select'].includes(this.tag);
	}
}

function createSearch() {
	const attributes = new Map();
	return Object.assign(new FakeElement('input'), {
		value: 'ripgrep',
		disabled: false,
		hidden: false,
		focus: mock(() => {}),
		select: mock(() => {}),
		setAttribute: (name, value) => attributes.set(name, value),
		attributes,
	});
}

function keyEvent(overrides = {}) {
	return {
		key: '/',
		ctrlKey: false,
		altKey: false,
		metaKey: false,
		shiftKey: false,
		isComposing: false,
		defaultPrevented: false,
		composedPath: () => [new FakeElement('body')],
		preventDefault: mock(() => {}),
		...overrides,
	};
}

function initialize({ search = createSearch() } = {}) {
	const handlers = {};
	const document = {
		querySelector: (selector) => selector === '#q' ? search : null,
		addEventListener: mock((event, handler) => { handlers[event] = handler; }),
	};
	runInNewContext(source, { document, HTMLElement: FakeElement });
	return { search, document, press: (event) => handlers.keydown?.(event), replaceSearch: (next) => { search = next; } };
}

test('slash focuses search and selects the query without changing its contents', () => {
	const state = initialize();
	expect(state.search.attributes.get('aria-keyshortcuts')).toBe('/');
	expect(state.search.title).toBe('press / to focus search');
	expect(state.search.focus).not.toHaveBeenCalled();
	const event = keyEvent();
	state.press(event);
	expect(state.search.focus).toHaveBeenCalledTimes(1);
	expect(state.search.select).toHaveBeenCalledTimes(1);
	expect(event.preventDefault).toHaveBeenCalledTimes(1);
	expect(state.search.value).toBe('ripgrep');
});

test('non-slash keys, modifiers, composition, and handled events retain their behavior', () => {
	for (const overrides of [
		{ key: '?' },
		{ key: 'a' },
		{ ctrlKey: true },
		{ altKey: true },
		{ metaKey: true },
		{ isComposing: true },
		{ defaultPrevented: true },
	]) {
		const state = initialize();
		const event = keyEvent(overrides);
		state.press(event);
		expect(state.search.focus).not.toHaveBeenCalled();
		expect(event.preventDefault).not.toHaveBeenCalled();
	}
});

test('shift is allowed when a keyboard layout needs it to type slash', () => {
	const state = initialize();
	const event = keyEvent({ shiftKey: true, code: 'Digit7' });
	state.press(event);
	expect(state.search.focus).toHaveBeenCalledTimes(1);
	expect(event.preventDefault).toHaveBeenCalledTimes(1);
});

test('slash never takes focus from form controls or editable content', () => {
	const editable = Object.assign(new FakeElement(), { isContentEditable: true });
	for (const target of [new FakeElement('input'), new FakeElement('textarea'), new FakeElement('select'), editable]) {
		const state = initialize();
		const event = keyEvent({ composedPath: () => [target, new FakeElement('body')] });
		state.press(event);
		expect(state.search.focus).not.toHaveBeenCalled();
		expect(event.preventDefault).not.toHaveBeenCalled();
	}
});

test('editable ancestors and shadow-tree inputs also prevent the shortcut', () => {
	const editable = Object.assign(new FakeElement(), { isContentEditable: true });
	for (const path of [[new FakeElement('span'), editable], [new FakeElement('input'), new FakeElement()]]) {
		const state = initialize();
		const event = keyEvent({ composedPath: () => path });
		state.press(event);
		expect(state.search.focus).not.toHaveBeenCalled();
		expect(event.preventDefault).not.toHaveBeenCalled();
	}
});

test('slash remains a literal character when typing in the search box', () => {
	const state = initialize();
	const event = keyEvent({ composedPath: () => [state.search] });
	state.press(event);
	expect(state.search.select).not.toHaveBeenCalled();
	expect(event.preventDefault).not.toHaveBeenCalled();
});

test('disabled or hidden search boxes do not consume slash', () => {
	for (const property of ['disabled', 'hidden']) {
		const state = initialize();
		state.search[property] = true;
		const event = keyEvent();
		state.press(event);
		expect(state.search.focus).not.toHaveBeenCalled();
		expect(event.preventDefault).not.toHaveBeenCalled();
	}
});

test('slash follows the search box after browser-side navigation replaces it', () => {
	const state = initialize();
	const next = createSearch();
	state.replaceSearch(next);
	state.press(keyEvent());
	expect(next.focus).toHaveBeenCalledTimes(1);
	expect(state.search.focus).not.toHaveBeenCalled();
});

test('pages without search do not install a keyboard handler', () => {
	const state = initialize({ search: null });
	expect(state.document.addEventListener).not.toHaveBeenCalled();
});
