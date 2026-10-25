import { rmSync } from 'node:fs';
import { mkdtemp, readdir, realpath, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { parseArgs } from 'node:util';

import { identityRoutePath, notFoundResponse, readStaticFile, staticFileResponse, type StaticFile } from './files';
import { parseAddress, staticResponse } from './http';

const LIVE_RELOAD_SCRIPT = `const events = new EventSource('/__reload');
events.addEventListener('reload', () => location.reload());
events.addEventListener('build-error', (event) => {
	console.error('bnck dev build failed:', JSON.parse(event.data));
});
`;

const GENERATOR = join(import.meta.dir, 'generate.ts');
const { values } = parseArgs({
	options: {
		database: { type: 'string', default: 'var/bnck.sqlite.zst' },
		address: { type: 'string', default: '127.0.0.1:3000' },
	},
});

const databasePath = values.database;
const { hostname, port } = parseAddress(values.address);
const webRoot = import.meta.dir;
const encoder = new TextEncoder();
const clients = new Set<ReadableStreamDefaultController<Uint8Array>>();
const workRoot = await mkdtemp(join(tmpdir(), 'bnck-dev-'));
const siteRoots: string[] = [];

let buildNumber = 0;
let lastFingerprint: string;
let rebuilding = false;

async function buildSite(): Promise<string> {
	const output = join(workRoot, `site-${buildNumber++}`);
	const child = Bun.spawn([
		process.execPath,
		GENERATOR,
		'--database', databasePath,
		'--output', output,
		'--live-reload',
	], { stdout: 'pipe', stderr: 'pipe' });
	const [stdout, stderr, exitCode] = await Promise.all([
		new Response(child.stdout).text(),
		new Response(child.stderr).text(),
		child.exited,
	]);
	if (exitCode !== 0) {
		if (stdout.trim()) console.error(stdout.trim());
		throw new Error(stderr.trim() || `site generator exited with status ${exitCode}`);
	}
	return realpath(output);
}

function eventStream(): Response {
	let streamController: ReadableStreamDefaultController<Uint8Array> | null = null;
	const stream = new ReadableStream<Uint8Array>({
		start(controller) {
			streamController = controller;
			controller.enqueue(encoder.encode('retry: 500\n\n'));
			clients.add(controller);
		},
		cancel() {
			if (streamController) clients.delete(streamController);
		},
	});
	return new Response(stream, {
		headers: {
			'Cache-Control': 'no-cache',
			'Content-Type': 'text/event-stream',
			'X-Content-Type-Options': 'nosniff',
		},
	});
}

function broadcast(event: string, data: unknown = ''): void {
	const payload = encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
	for (const client of clients) {
		try {
			client.enqueue(payload);
		} catch {
			clients.delete(client);
		}
	}
}

async function findFile(pathname: string): Promise<StaticFile | null> {
	for (const [index, root] of siteRoots.entries()) {
		/* Previous builds keep answering immutable asset URLs until an open page reloads. */
		if (index > 0 && !pathname.startsWith('/assets/')) break;
		const file = await readStaticFile(root, pathname);
		if (file) return file;
	}
	return null;
}

async function serve(request: Request): Promise<Response> {
	if (!['GET', 'HEAD'].includes(request.method)) {
		return staticResponse('Method not allowed', {
			status: 405,
			contentType: 'text/plain; charset=utf-8',
			contentLength: 'Method not allowed'.length,
		});
	}
	const url = new URL(request.url);
	if (url.pathname === '/__reload') return eventStream();
	if (url.pathname === '/__reload.js') {
		const body = encoder.encode(LIVE_RELOAD_SCRIPT);
		return staticResponse(request.method === 'HEAD' ? null : body, {
			contentType: 'text/javascript; charset=utf-8',
			contentLength: body.length,
		});
	}

	let pathname: string;
	try {
		pathname = decodeURIComponent(url.pathname);
	} catch {
		return staticResponse('Invalid path', { status: 400 });
	}
	if (pathname.includes('\0') || pathname.includes('\\') || pathname.split('/').includes('..') ||
		pathname === '/_headers') {
		return staticResponse('Invalid path', { status: 400 });
	}

	const file = await findFile(identityRoutePath(pathname) ?? pathname);
	if (file) return staticFileResponse(file, request, { immutable: pathname.startsWith('/assets/') });
	return notFoundResponse(siteRoots[0], request);
}

async function rebuild(): Promise<void> {
	const next = await buildSite();
	siteRoots.unshift(next);
	if (siteRoots.length > 2) {
		const old = siteRoots.pop();
		if (old) await rm(old, { recursive: true, force: true });
	}
}

async function fileStamp(path: string): Promise<string> {
	const info = await stat(path);
	return `${info.mtimeMs}:${info.size}`;
}

async function sourceFingerprint(): Promise<string> {
	const entries: string[] = [];
	async function visit(directory: string): Promise<void> {
		const children = await readdir(directory, { withFileTypes: true });
		children.sort((left, right) => left.name.localeCompare(right.name));
		for (const child of children) {
			if (child.name === 'fixtures' || child.name.endsWith('.test.ts') || child.name.endsWith('.test.js')) continue;
			const path = join(directory, child.name);
			if (child.isDirectory()) {
				await visit(path);
				continue;
			}
			const info = await stat(path);
			entries.push(`${relative(webRoot, path)}:${info.mtimeMs}:${info.size}`);
		}
	}
	await visit(webRoot);
	entries.push(`database:${await fileStamp(databasePath)}`);
	return entries.join('\n');
}

siteRoots.push(await buildSite());
lastFingerprint = await sourceFingerprint();

const server = Bun.serve({
	hostname,
	port,
	idleTimeout: 0,
	fetch: serve,
});
console.info(`dev server for ${databasePath} at ${server.url}`);
console.info(`watching ${webRoot} and ${databasePath}`);

setInterval(async () => {
	if (rebuilding) return;
	try {
		const nextFingerprint = await sourceFingerprint();
		if (nextFingerprint === lastFingerprint) return;
		lastFingerprint = nextFingerprint;
		rebuilding = true;
		const started = performance.now();
		try {
			await rebuild();
			console.info(`rebuilt site in ${Math.round(performance.now() - started)}ms`);
			broadcast('reload');
		} catch (error) {
			console.error(`website rebuild failed: ${error instanceof Error ? error.stack ?? error.message : String(error)}`);
			broadcast('build-error', error instanceof Error ? error.message : String(error));
		} finally {
			rebuilding = false;
		}
	} catch (error) {
		console.error(`website watch failed: ${error instanceof Error ? error.stack ?? error.message : String(error)}`);
	}
}, 300);

function cleanup(): void {
	rmSync(workRoot, { recursive: true, force: true });
}

async function shutdown(): Promise<void> {
	server.stop();
	cleanup();
	process.exit(0);
}

process.on('SIGINT', () => { void shutdown(); });
process.on('SIGTERM', () => { void shutdown(); });
process.on('exit', cleanup);
