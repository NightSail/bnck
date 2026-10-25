import { realpath } from 'node:fs/promises';

import { parseAddress, staticResponse } from './http';
import { identityRoutePath, notFoundResponse, readStaticFile, staticFileResponse } from './files';

const root = await realpath(process.argv[2] ?? 'var/site');
const { hostname, port } = parseAddress(process.argv[3] ?? '127.0.0.1:3000');

const server = Bun.serve({
	hostname,
	port,
	async fetch(request) {
		if (!['GET', 'HEAD'].includes(request.method)) {
			return staticResponse('Method not allowed', {
				status: 405,
				contentType: 'text/plain; charset=utf-8',
				contentLength: 'Method not allowed'.length,
			});
		}
		let pathname: string;
		try {
			pathname = decodeURIComponent(new URL(request.url).pathname);
		} catch {
			return staticResponse('Invalid path', { status: 400 });
		}
		if (pathname.includes('\0') || pathname.includes('\\') || pathname.split('/').includes('..')) {
			return staticResponse('Invalid path', { status: 400 });
		}
		const file = pathname === '/_headers' ? null : await readStaticFile(root, identityRoutePath(pathname) ?? pathname);
		return file ? staticFileResponse(file, request, { immutable: pathname.startsWith('/assets/') }) :
			notFoundResponse(root, request);
	},
});
console.info(`Previewing ${root} at ${server.url}`);
