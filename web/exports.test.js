import { expect, mock, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('./exports.js', import.meta.url), 'utf8');
const exportUrl = 'https://bnck.example/executables/age/providers.json';
const payload = {
	dataset_release: 'bnck:release:test',
	query: { executable: 'age' },
	providers: [{ project: 'age', executable: 'age', channel: 'debian', package: 'age' }],
};
const formattedJson = JSON.stringify(payload, null, '\t') + '\n';
const failureMessage = 'Copy failed. Use view JSON to copy manually.';

class FakeClipboardItem {
	constructor(data) {
		this.data = data;
	}
}

function deferred() {
	let resolve;
	const promise = new Promise((complete) => { resolve = complete; });
	return { promise, resolve };
}

function manualTimers() {
	let nextId = 0;
	const pending = new Map();
	return {
		schedule: (callback, delay) => {
			const id = ++nextId;
			pending.set(id, { callback, delay });
			return id;
		},
		cancel: (id) => pending.delete(id),
		run: (delay) => {
			for (const [id, timer] of [...pending]) {
				if (timer.delay === delay) {
					pending.delete(id);
					timer.callback();
				}
			}
		},
	};
}

function initialize({
	clipboard = { writeText: mock(async () => {}) },
	fetch = mock(async () => new Response(JSON.stringify(payload))),
	isSecureContext = true,
	ClipboardItem,
	timers = manualTimers(),
	schedule = timers.schedule,
	cancel = timers.cancel,
} = {}) {
	const handlers = {};
	const button = {
		hidden: true,
		disabled: false,
		textContent: 'copy JSON',
		addEventListener: (event, handler) => { handlers[event] = handler; },
	};
	const classes = new Set(['visually-hidden']);
	const status = {
		textContent: '',
		classList: {
			add: (name) => classes.add(name),
			remove: (name) => classes.delete(name),
			contains: (name) => classes.has(name),
		},
	};
	const view = { href: exportUrl };
	const elements = { '[data-copy-json]': button, '[data-view-json]': view, '[role="status"]': status };
	const actions = { querySelector: (selector) => elements[selector] };
	const document = { querySelectorAll: (selector) => selector === '[data-json-export]' ? [actions] : [] };
	runInNewContext(source, {
		document,
		window: { isSecureContext },
		navigator: { clipboard },
		ClipboardItem,
		Blob,
		AbortController,
		fetch,
		setTimeout: schedule,
		clearTimeout: cancel,
	});
	return { button, status, view, fetch, clipboard, timers, click: () => handlers.click() };
}

function pendingFetch() {
	return mock((_url, { signal }) => new Promise((_resolve, reject) => {
		signal.addEventListener('abort', () => reject(new Error('Fetch aborted')), { once: true });
	}));
}

test('copy remains hidden without a secure context or clipboard API', () => {
	for (const options of [{ isSecureContext: false }, { clipboard: {} }]) {
		const state = initialize(options);
		expect(state.button.hidden).toBe(true);
		expect(state.fetch).not.toHaveBeenCalled();
		expect(state.view.href).toBe(exportUrl);
	}
});

test('copy fetches only on click and writes readable JSON through writeText', async () => {
	const state = initialize();
	expect(state.button.hidden).toBe(false);
	expect(state.fetch).not.toHaveBeenCalled();
	await state.click();
	expect(state.fetch).toHaveBeenCalledTimes(1);
	const [url, options] = state.fetch.mock.calls[0];
	expect(url).toBe(exportUrl);
	expect(options.headers.Accept).toBe('application/json');
	expect(options.credentials).toBe('same-origin');
	expect(options.redirect).toBe('error');
	expect(state.clipboard.writeText).toHaveBeenCalledWith(formattedJson);
	expect(state.status.textContent).toBe('JSON copied.');
	expect(state.button.disabled).toBe(false);
});

test('success appears in the button without a visible status line and resets after two seconds', async () => {
	const state = initialize();
	await state.click();
	expect(state.button.textContent).toBe('JSON copied');
	expect(state.status.textContent).toBe('JSON copied.');
	expect(state.status.classList.contains('visually-hidden')).toBe(true);
	state.timers.run(2000);
	expect(state.button.textContent).toBe('copy JSON');
	expect(state.status.textContent).toBe('');
});

test('a new attempt cancels the previous label reset while its request is pending', async () => {
	const response = deferred();
	let attempts = 0;
	const state = initialize({
		fetch: mock(() => ++attempts === 1 ? Promise.resolve(new Response(JSON.stringify(payload))) : response.promise),
	});
	await state.click();
	const copying = state.click();
	expect(state.button.textContent).toBe('copying…');
	expect(state.status.classList.contains('visually-hidden')).toBe(true);
	state.timers.run(2000);
	expect(state.button.textContent).toBe('copying…');
	response.resolve(new Response(JSON.stringify(payload)));
	await copying;
	expect(state.button.textContent).toBe('JSON copied');
	state.timers.run(2000);
	expect(state.button.textContent).toBe('copy JSON');
});

test('clipboard items preserve user activation and success waits for the write', async () => {
	const response = deferred();
	const write = deferred();
	const clipboard = { write: mock(() => write.promise) };
	const state = initialize({ clipboard, fetch: mock(() => response.promise), ClipboardItem: FakeClipboardItem });
	const copying = state.click();
	expect(clipboard.write).toHaveBeenCalledTimes(1);
	expect(state.button.disabled).toBe(true);
	expect(state.status.textContent).toBe('copying JSON…');
	response.resolve(new Response(JSON.stringify(payload)));
	const blob = await clipboard.write.mock.calls[0][0][0].data['text/plain'];
	expect(blob.type.split(';')[0]).toBe('text/plain');
	expect(await blob.text()).toBe(formattedJson);
	expect(state.status.textContent).toBe('copying JSON…');
	write.resolve();
	await copying;
	expect(state.status.textContent).toBe('JSON copied.');
	expect(state.button.disabled).toBe(false);
});

for (const [name, fetch] of [
	['HTTP errors', async () => new Response('{"error":"Unavailable"}', { status: 500 })],
	['invalid JSON', async () => new Response('<html>Not JSON</html>')],
	['network errors', async () => { throw new Error('Offline'); }],
]) {
	test(`${name} leave view JSON available and do not copy an error response`, async () => {
		const state = initialize({ fetch: mock(fetch) });
		await state.click();
		expect(state.clipboard.writeText).not.toHaveBeenCalled();
		expect(state.status.textContent).toBe(failureMessage);
		expect(state.status.classList.contains('visually-hidden')).toBe(false);
		expect(state.button.textContent).toBe('copy JSON');
		expect(state.button.disabled).toBe(false);
		expect(state.view.href).toBe(exportUrl);
	});
}

test('clipboard denial reports failure and a later attempt can succeed', async () => {
	let isAllowed = false;
	const clipboard = { writeText: mock(async () => {
		if (!isAllowed) {
			throw new Error('Clipboard denied');
		}
	}) };
	const state = initialize({ clipboard });
	await state.click();
	expect(state.status.textContent).toBe(failureMessage);
	expect(state.button.disabled).toBe(false);
	isAllowed = true;
	await state.click();
	expect(state.status.textContent).toBe('JSON copied.');
	expect(state.status.classList.contains('visually-hidden')).toBe(true);
	expect(state.fetch).toHaveBeenCalledTimes(2);
});

test('early clipboard denial aborts the pending fetch without an unhandled rejection', async () => {
	const clipboard = { write: mock(async () => { throw new Error('Clipboard denied'); }) };
	const state = initialize({ clipboard, fetch: pendingFetch(), ClipboardItem: FakeClipboardItem });
	await state.click();
	expect(state.fetch.mock.calls[0][1].signal.aborted).toBe(true);
	expect(state.status.textContent).toBe(failureMessage);
	expect(state.button.disabled).toBe(false);
});

test('a stalled fetch times out and restores the copy button', async () => {
	let expire;
	const cancel = mock(() => {});
	const state = initialize({
		fetch: pendingFetch(),
		schedule: (callback, delay) => {
			expect(delay).toBe(10000);
			expire = callback;
			return 7;
		},
		cancel,
	});
	const copying = state.click();
	expire();
	await copying;
	expect(state.status.textContent).toBe(failureMessage);
	expect(state.button.disabled).toBe(false);
	expect(state.clipboard.writeText).not.toHaveBeenCalled();
	expect(cancel).toHaveBeenCalledWith(7);
});

test('repeated clicks during a pending request do not start another export', async () => {
	const response = deferred();
	const state = initialize({ fetch: mock(() => response.promise) });
	const copying = state.click();
	await state.click();
	expect(state.fetch).toHaveBeenCalledTimes(1);
	response.resolve(new Response(JSON.stringify(payload)));
	await copying;
	expect(state.clipboard.writeText).toHaveBeenCalledTimes(1);
	expect(state.status.textContent).toBe('JSON copied.');
});
