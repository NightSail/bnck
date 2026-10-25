import { expect, mock, test } from 'bun:test';

import worker, { type WorkerEnvironment } from './worker';

const identity = '<h1>Identity</h1>';
const notFound = '<h1>Not found</h1>';

function environment() {
	const fetch = mock(async (input: Request | URL | string) => {
		const path = typeof input === 'string' ? input : input instanceof URL ? input.pathname : new URL(input.url).pathname;
		if (path === '/identity') return new Response(identity, { headers: { 'Content-Type': 'text/html' } });
		if (path === '/404') return new Response(notFound, { headers: { 'Content-Type': 'text/html' } });
		if (path === '/identity.html' || path === '/404.html') {
			return Response.redirect(`https://bnck.test${path.slice(0, -'.html'.length)}`, 307);
		}
		return new Response('asset', { headers: { 'Content-Type': 'text/plain' } });
	});
	return { env: { ASSETS: { fetch } } as WorkerEnvironment, fetch };
}

test('identity routes serve the shared shell without changing the URL', async () => {
	for (const path of ['/projects/pacman', '/executables/pacman', '/executables/HX']) {
		const { env } = environment();
		const response = await worker.fetch(new Request(`https://bnck.test${path}`), env);
		expect(response.status).toBe(200);
		expect(response.headers.get('Location')).toBeNull();
		expect(await response.text()).toBe(identity);
	}
});

test('identity requests forward the method and fetch the shell asset', async () => {
	const { env, fetch } = environment();
	const response = await worker.fetch(new Request('https://bnck.test/projects/pacman', { method: 'HEAD' }), env);
	expect(response.status).toBe(200);
	expect(fetch).toHaveBeenCalledTimes(1);
	const request = fetch.mock.calls[0][0] as Request;
	expect(new URL(request.url).pathname).toBe('/identity');
	expect(request.method).toBe('HEAD');
});

test('trailing slashes and old export URLs redirect before rendering', async () => {
	const { env } = environment();
	for (const [path, location] of [
		['/projects/pacman/', 'https://bnck.test/projects/pacman'],
		['/executables/pacman/', 'https://bnck.test/executables/pacman'],
		['/projects/pacman/providers.json', 'https://bnck.test/projects/pacman'],
		['/executables/pacman/providers.json', 'https://bnck.test/executables/pacman'],
	]) {
		const response = await worker.fetch(new Request(`https://bnck.test${path}`), env);
		expect(response.status).toBe(301);
		expect(response.headers.get('Location')).toBe(location);
	}
});

test('invalid identity paths return the 404 page', async () => {
	const { env } = environment();
	for (const path of ['/projects/pacman/extra', '/executables/%zz/extra']) {
		const response = await worker.fetch(new Request(`https://bnck.test${path}`), env);
		expect(response.status).toBe(404);
		expect(await response.text()).toBe(notFound);
	}
});

test('non-identity paths are delegated to the asset binding', async () => {
	const { env, fetch } = environment();
	const request = new Request('https://bnck.test/dataset');
	const response = await worker.fetch(request, env);
	expect(response.status).toBe(200);
	expect(await response.text()).toBe('asset');
	expect(fetch).toHaveBeenCalledWith(request);
});

test('identity routes reject methods other than GET and HEAD', async () => {
	const { env } = environment();
	const response = await worker.fetch(new Request('https://bnck.test/projects/pacman', { method: 'POST' }), env);
	expect(response.status).toBe(405);
});
