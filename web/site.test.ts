import { Database } from 'bun:sqlite';
import { expect, test } from 'bun:test';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { WebsiteDatabase } from './database';
import { buildSite, generateSite } from './site';
import { createFixture, provider } from './test-fixture';

async function assetPath(root: string, prefix: string): Promise<string> {
	const paths = (await readdir(join(root, 'assets'))).filter((name) => name.startsWith(prefix));
	expect(paths).toHaveLength(1);
	return join(root, 'assets', paths[0]);
}

test('generates a complete versioned site without modifying the database', async () => {
	const fixture = await createFixture([
		provider('helix', 'hx', 'debian', 'helix'),
		provider('helix', 'hx', 'crates.io', 'helix-term'),
		provider('other', 'hx', 'npm'),
		provider('other', 'HX', 'npm'),
		provider('escaping', "é%&'<>#?", 'npm'),
	], [{ project: 'helix', link_type: 'homepage', url: 'https://helix-editor.com/' }], {
		compressed: true,
		associations: [{ project: 'helix', executable: 'cat', role: 'implementation' }],
	});
	const root = await mkdtemp(join(tmpdir(), 'bnck-site-'));
	try {
		const output = join(root, 'site');
		const before = await readFile(fixture.path);
		await generateSite(fixture.path, output);
		expect(await readFile(fixture.path)).toEqual(before);

		const home = await readFile(join(output, 'index.html'), 'utf8');
		expect(home).toContain("name='bnck-dataset'");
		expect(home).toContain('Search and filtered browsing require JavaScript');
		const applicationDirectory = await assetPath(output, 'app.');
		const datasetPath = await assetPath(output, 'dataset.');
		const applicationName = applicationDirectory.split('/').at(-1)!;
		const datasetName = datasetPath.split('/').at(-1)!;
		expect(home).toContain(`src='/assets/${applicationName}/browser.js'`);
		expect(home).toContain(`content='/assets/${datasetName}'`);

		const dataset = JSON.parse(await readFile(datasetPath, 'utf8'));
		expect(dataset.schema_version).toBe(2);
		expect(dataset.dataset_release).toBe('2026-04-16.1');
		expect(dataset.channels).toHaveLength(7);
		expect(dataset.projects).toHaveLength(3);
		expect(dataset.projects[1].links[0].url).toBe('https://helix-editor.com/');
		expect(dataset.projects[1].providers).toHaveLength(2);
		expect(dataset.projects[1].associations).toEqual([{ executable: 'cat', role: 'implementation' }]);

		for (const asset of ['style.css', 'search.js', 'exports.js', 'browser.js']) {
			expect(await readdir(applicationDirectory)).toContain(asset);
		}
		for (const path of [
			'about.html',
			'dataset.html',
			'short/2.html',
			'short/a.html',
			'short/z.html',
			'404.html',
			'projects/helix.html',
			'executables/hx.html',
			'executables/HX.html',
			'executables/cat.html',
			"executables/é%&'<>#?.html",
		]) {
			expect((await readFile(join(output, path))).length).toBeGreaterThan(0);
		}

		const datasetPage = await readFile(join(output, 'dataset.html'), 'utf8');
		expect(datasetPage).toContain("href='/data/2026-04-16.1'");
		expect(datasetPage).toContain("href='/data/latest'");
		expect(datasetPage).toContain('releases/tag/bnck-dataset-2026-04-16.1');
		expect(datasetPage).toContain("href='/data/2026-04-16.1/bnck.sqlite.zst'");
		expect(datasetPage).not.toContain("href='/data/latest/bnck");
		const about = await readFile(join(output, 'about.html'), 'utf8');
		expect(about).toContain('maps executable names to the projects');
		expect(about).toContain('github.com/NightSail/bnck');
		const project = await readFile(join(output, 'projects/helix.html'), 'utf8');
		expect(project).toContain('https://helix-editor.com/');
		expect(project).toContain("<h2 id='associations-title'>name associations</h2>");
		expect(project).not.toContain('browser.js');
		expect((await readdir(join(output, 'short'))).sort())
			.toEqual(['2', ...'abcdefghijklmnopqrstuvwxyz'].map((name) => `${name}.html`));
		const short = await readFile(join(output, 'short/2.html'), 'utf8');
		expect(short).toContain('short name map');
		expect(short).toContain("href='/short/2' aria-current='page'");
		expect(short).toContain("href='/executables/hx'");
		const shortH = await readFile(join(output, 'short/h.html'), 'utf8');
		expect(shortH).toContain("id='short-h'");
		expect(shortH).toContain("href='/short/h' aria-current='page'");
		expect(shortH).not.toContain("href='/executables/hx'");
		expect(home).toContain("href='/short/2'");

		const exported = await readFile(join(output, 'executables/hx/providers.json'), 'utf8');
		expect(exported).toContain('\n\t"dataset_release"');
		expect(exported.endsWith('\n')).toBe(true);
		const payload = JSON.parse(exported);
		expect(payload.providers).toHaveLength(3);
		expect(payload.associations).toEqual([]);
		expect(payload.query).toEqual({ executable: 'hx' });

		const associationExport = JSON.parse(
			await readFile(join(output, 'executables/cat/providers.json'), 'utf8'),
		);
		expect(associationExport.providers).toEqual([]);
		expect(associationExport.associations).toEqual([{ project: 'helix', role: 'implementation' }]);

		const projectExport = JSON.parse(await readFile(join(output, 'projects/helix/providers.json'), 'utf8'));
		expect(projectExport.associations).toEqual([{ executable: 'cat', role: 'implementation' }]);

		const headers = await readFile(join(output, '_headers'), 'utf8');
		expect(headers).toContain('/dataset\n  Cache-Control: no-cache');
		expect(headers).toContain('/short\n  Cache-Control: no-cache');
		expect(headers).toContain('max-age=31536000, immutable');
		expect(headers).toContain('Content-Security-Policy');
		expect(headers.split('\n\n')[0]).not.toContain('Cache-Control');
		const redirects = await readFile(join(output, '_redirects'), 'utf8');
		expect(redirects).toContain(
			'/data/latest https://github.com/NightSail/bnck/releases/tag/bnck-dataset-2026-04-16.1 302',
		);
		expect(redirects).toContain(
			'/data/:release https://github.com/NightSail/bnck/releases/tag/bnck-dataset-:release 301',
		);
		expect(redirects).toContain(
			'/data/:release/* https://github.com/NightSail/bnck/releases/download/bnck-dataset-:release/:splat 301',
		);
		for (const path of ['/about/', '/dataset/', '/short/:section/', '/projects/:project/',
			'/executables/:executable/']) {
			expect(redirects).toContain(`${path} ${path.slice(0, -1)} 301`);
		}
		expect(redirects).not.toContain('{{release}}');

		const second = join(root, 'second');
		await generateSite(fixture.path, second);
		expect((await assetPath(second, 'app.')).split('/').at(-1)).toBe(applicationName);
		expect((await assetPath(second, 'dataset.')).split('/').at(-1)).toBe(datasetName);
		await expect(generateSite(fixture.path, output)).rejects.toThrow('already exists');
	} finally {
		await fixture.cleanup();
		await rm(root, { recursive: true, force: true });
	}
});

test('rejects an unsupported SQLite schema with a rebuild message', async () => {
	const fixture = await createFixture([provider('helix', 'hx', 'debian')]);
	try {
		const database = new Database(fixture.path);
		database.exec("UPDATE metadata SET value = '1' WHERE key = 'schema_version'; PRAGMA user_version = 1;");
		database.close();
		await expect(WebsiteDatabase.open(fixture.path)).rejects.toThrow(
			'database SQLite schema 1 is unsupported; expected 2',
		);
	} finally {
		await fixture.cleanup();
	}
});

test('project link changes only invalidate the dataset', async () => {
	const fixture = await createFixture([provider('helix', 'hx', 'debian')]);
	const root = await mkdtemp(join(tmpdir(), 'bnck-site-'));
	try {
		const before = join(root, 'before');
		await generateSite(fixture.path, before);
		const database = new Database(fixture.path);
		database.exec("INSERT INTO project_links VALUES ('helix', 'homepage', 'https://helix-editor.com/')");
		database.close();
		const after = join(root, 'after');
		await generateSite(fixture.path, after);
		expect((await assetPath(before, 'app.')).split('/').at(-1)).toBe(
			(await assetPath(after, 'app.')).split('/').at(-1),
		);
		expect((await assetPath(before, 'dataset.')).split('/').at(-1)).not.toBe(
			(await assetPath(after, 'dataset.')).split('/').at(-1),
		);
	} finally {
		await fixture.cleanup();
		await rm(root, { recursive: true, force: true });
	}
});

test('failed builds do not publish partial files or staging directories', async () => {
	const fixture = await createFixture([provider('dot', '..', 'npm')]);
	const root = await mkdtemp(join(tmpdir(), 'bnck-site-'));
	try {
		const output = join(root, 'site');
		await expect(generateSite(fixture.path, output)).rejects.toThrow('static page path');
		expect(await readdir(root)).toEqual([]);
	} finally {
		await fixture.cleanup();
		await rm(root, { recursive: true, force: true });
	}
});

test('empty datasets still generate a usable site', async () => {
	const fixture = await createFixture([]);
	const root = await mkdtemp(join(tmpdir(), 'bnck-site-'));
	try {
		const output = join(root, 'site');
		await generateSite(fixture.path, output);
		const home = await readFile(join(output, 'index.html'), 'utf8');
		expect(home).toContain('No executables listed in this release.');
		const datasetPage = await readFile(join(output, 'dataset.html'), 'utf8');
		expect(datasetPage).toContain('No channels listed in this release.');
	} finally {
		await fixture.cleanup();
		await rm(root, { recursive: true, force: true });
	}
});

test('live reload builds reference the development endpoint', async () => {
	const fixture = await createFixture([provider('helix', 'hx', 'debian')]);
	const database = await WebsiteDatabase.open(fixture.path);
	try {
		const files = await buildSite(database, { liveReload: true });
		const home = new TextDecoder().decode(files.get('index.html'));
		const project = new TextDecoder().decode(files.get('projects/helix.html'));
		expect(home).toContain("src='/__reload.js'");
		expect(project).toContain("src='/__reload.js'");
	} finally {
		database.close();
		await fixture.cleanup();
	}
});
