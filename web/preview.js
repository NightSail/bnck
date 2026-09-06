import { realpath, stat } from 'node:fs/promises';
import { resolve, sep } from 'node:path';

const root = await realpath(process.argv[2] ?? 'var/site');
const port = Number(process.argv[3] ?? 3000);
const policy = "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; " +
	"form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

function response(body, status, immutable = false) {
	return new Response(body, {
		status,
		headers: {
			'Cache-Control': immutable ? 'public, max-age=31536000, immutable' : 'no-cache',
			'Content-Security-Policy': policy,
			'X-Content-Type-Options': 'nosniff',
			...(body instanceof Blob ? { 'Content-Length': String(body.size) } : {}),
		},
	});
}

async function readFile(path) {
	try {
		let resolved = await realpath(path);
		if (!resolved.startsWith(`${root}${sep}`) && resolved !== root) return null;
		if ((await stat(resolved)).isDirectory()) resolved = await realpath(resolve(resolved, 'index.html'));
		if (!resolved.startsWith(`${root}${sep}`)) return null;
		return Bun.file(resolved);
	} catch (error) {
		if (['ENOENT', 'ENOTDIR'].includes(error.code)) return null;
		throw error;
	}
}

const server = Bun.serve({
	hostname: '127.0.0.1',
	port,
	async fetch(request) {
		if (!['GET', 'HEAD'].includes(request.method)) {
			return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
		}
		let pathname;
		try {
			pathname = decodeURIComponent(new URL(request.url).pathname);
		} catch {
			return response('Invalid path', 400);
		}
		if (pathname.includes('\0') || pathname.includes('\\') || pathname.split('/').includes('..')) {
			return response('Invalid path', 400);
		}
		const file = pathname === '/_headers' ? null : await readFile(resolve(root, `.${pathname}`));
		const result = file ? response(file, 200, pathname.startsWith('/assets/')) :
			response(await readFile(resolve(root, '404.html')) ?? 'Not found', 404);
		return request.method === 'HEAD' ? new Response(null, result) : result;
	},
});
console.info(`Previewing ${root} at ${server.url}`);
