import { expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

test('static preview preserves paths, headers, HEAD, and 404s without exposing other files', async () => {
	const directory = await mkdtemp(join(tmpdir(), 'bnck-preview-'));
	const root = join(directory, 'site');
	await mkdir(join(root, 'folder'), { recursive: true });
	await mkdir(join(root, 'assets/app.hash'), { recursive: true });
	const files = {
		'index.html': '<h1>Home</h1>',
		'404.html': '<h1>Not found</h1>',
		'about.html': '<h1>About</h1>',
		'folder/index.html': '<h1>Folder</h1>',
		'identity.html': '<h1>Identity</h1>',
		'assets/dataset.hash.json': '{}',
		'assets/app.hash/browser.js': 'export const value = 1;',
		'assets/app.hash/style.css': 'body { color: green; }',
		"é%&'#?.html": '<h1>Special</h1>',
		'_headers': 'host configuration',
	};
	for (const [path, contents] of Object.entries(files)) await writeFile(join(root, path), contents);
	await writeFile(join(directory, 'secret.txt'), 'private');
	await symlink(join(directory, 'secret.txt'), join(root, 'outside.txt'));
	const child = Bun.spawn([process.execPath, join(import.meta.dir, 'preview.ts'), root, '127.0.0.1:0'], {
		stdout: 'pipe', stderr: 'inherit',
	});
	try {
		const reader = child.stdout.getReader();
		const { value } = await reader.read();
		const origin = new TextDecoder().decode(value).match(/http:\/\/[^\s]+/)[0];
		for (const [path, expected] of [
			['/', files['index.html']], ['/?q=hx', files['index.html']],
			['/about', files['about.html']],
			['/folder', files['folder/index.html']],
			['/folder/', files['folder/index.html']],
			['/projects/tool', files['identity.html']],
			['/projects/Tool', files['identity.html']],
			['/projects/tool/', files['identity.html']],
			['/executables/tool', files['identity.html']],
			[`/${encodeURIComponent("é%&'#?.html")}`, files["é%&'#?.html"]],
		]) {
			const result = await fetch(new URL(path, origin));
			expect(result.status).toBe(200);
			expect(result.headers.get('Content-Type')).toContain('text/html');
			expect(result.headers.get('Cache-Control')).toBe('no-cache');
			expect(result.headers.get('Content-Security-Policy')).toContain("script-src 'self'");
			expect(await result.text()).toBe(expected);
		}
		const head = await fetch(origin, { method: 'HEAD' });
		expect(head.headers.get('Content-Length')).toBe(String(files['index.html'].length));
		expect(await head.text()).toBe('');
		for (const path of ['/missing', '/projects/tool/providers.json', '/outside.txt', '/_headers']) {
			const result = await fetch(new URL(path, origin));
			expect(result.status).toBe(404);
			expect(await result.text()).toBe(files['404.html']);
		}
		for (const [file, type] of [['dataset.hash.json', 'application/json'], ['app.hash/browser.js', 'javascript'],
			['app.hash/style.css', 'text/css']]) {
			const result = await fetch(new URL(`/assets/${file}`, origin));
			expect(result.headers.get('Content-Type')).toContain(type);
			expect(result.headers.get('Cache-Control')).toContain('immutable');
		}
		expect((await fetch(new URL('/%zz', origin))).status).toBe(400);
		expect((await fetch(new URL('/%00', origin))).status).toBe(400);
		expect((await fetch(origin, { method: 'POST' })).status).toBe(405);
	} finally {
		child.kill();
		await child.exited;
		await rm(directory, { recursive: true });
	}
});
