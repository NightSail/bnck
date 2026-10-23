import { HTML_CSP } from './config';

export interface Address {
	hostname: string;
	port: number;
}

export function parseAddress(value: string): Address {
	let url: URL;
	try {
		url = new URL(`http://${value}`);
	} catch {
		throw new Error(`invalid address ${JSON.stringify(value)}; expected HOST:PORT`);
	}
	const isValid = url.port && url.pathname === '/' && !url.search && !url.hash && !url.username && !url.password;
	if (!isValid) throw new Error(`invalid address ${JSON.stringify(value)}; expected HOST:PORT`);
	return { hostname: url.hostname, port: Number(url.port) };
}

export function contentTypeForPath(path: string): string {
	const extension = path.slice(path.lastIndexOf('.'));
	switch (extension) {
		case '.html':
			return 'text/html; charset=utf-8';
		case '.js':
			return 'text/javascript; charset=utf-8';
		case '.css':
			return 'text/css; charset=utf-8';
		case '.json':
			return 'application/json';
		default:
			return 'application/octet-stream';
	}
}

export interface StaticResponseOptions {
	status?: number;
	contentType?: string;
	cacheControl?: string;
	contentLength?: number;
}

export function staticResponse(body: BodyInit | null, options: StaticResponseOptions = {}): Response {
	const headers = new Headers({
		'Cache-Control': options.cacheControl ?? 'no-cache',
		'Content-Security-Policy': HTML_CSP,
		'X-Content-Type-Options': 'nosniff',
	});
	if (options.contentType) headers.set('Content-Type', options.contentType);
	if (options.contentLength !== undefined) headers.set('Content-Length', String(options.contentLength));
	return new Response(body, { status: options.status ?? 200, headers });
}
