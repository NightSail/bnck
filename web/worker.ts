import { parseIdentityPath } from './identity-view';

interface AssetFetcher {
	fetch(input: Request | URL | string): Promise<Response>;
}

export interface WorkerEnvironment {
	ASSETS: AssetFetcher;
}

const IDENTITY_PREFIXES = ['/projects/', '/executables/'];

export default {
	async fetch(request: Request, env: WorkerEnvironment): Promise<Response> {
		const url = new URL(request.url);
		if (!IDENTITY_PREFIXES.some((prefix) => url.pathname.startsWith(prefix))) {
			return env.ASSETS.fetch(request);
		}
		if (request.method !== 'GET' && request.method !== 'HEAD') {
			return new Response('Method not allowed\n', {
				status: 405,
				headers: { 'Content-Type': 'text/plain; charset=utf-8' },
			});
		}
		if (url.pathname.endsWith('/')) {
			return redirectWithoutTrailingSlash(url);
		}
		if (/^\/(?:projects|executables)\/[^/]+\/providers\.json$/.test(url.pathname)) {
			const target = new URL(url);
			target.pathname = url.pathname.slice(0, -'/providers.json'.length);
			return Response.redirect(target.toString(), 301);
		}
		if (!parseIdentityPath(url.pathname)) {
			return notFound(request, env, url);
		}
		return env.ASSETS.fetch(new Request(new URL('/identity', url), request));
	},
};

function redirectWithoutTrailingSlash(url: URL): Response {
	const target = new URL(url);
	target.pathname = target.pathname.replace(/\/+$/, '');
	return Response.redirect(target.toString(), 301);
}

async function notFound(request: Request, env: WorkerEnvironment, url: URL): Promise<Response> {
	const response = await env.ASSETS.fetch(new Request(new URL('/404', url), request));
	if (!response.ok) {
		return new Response('Not found\n', {
			status: 404,
			headers: { 'Content-Type': 'text/plain; charset=utf-8' },
		});
	}
	return new Response(response.body, { status: 404, headers: response.headers });
}
