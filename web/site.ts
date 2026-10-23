import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rename, rm, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';

import { WebsiteDatabase } from './database';
import { browserDataset } from './data';
import { exportBytes } from './export-data';
import { HTML_CSP } from './config';
import { aboutPage, datasetPage, executablePage, homePage, notFoundPage, projectPage, shortPage } from './pages';
import type { PageContext } from './pages';

const WEB_ROOT = import.meta.dir;
const STYLE_PATH = join(WEB_ROOT, 'style.css');
const REDIRECTS_PATH = join(WEB_ROOT, '_redirects');
const BROWSER_ENTRYPOINTS = ['browser.ts', 'search.ts', 'exports.ts'].map((name) => join(WEB_ROOT, name));

export interface BuildOptions {
	liveReload?: boolean;
}

interface SiteAssets {
	application: string;
	dataset: string;
}

export async function buildSite(
	database: WebsiteDatabase,
	options: BuildOptions = {},
): Promise<Map<string, Uint8Array>> {
	const projects = database.listProjects();
	const executables = database.listExecutables();
	for (const project of projects) validateFileName(project);
	for (const executable of executables) validateFileName(executable);

	const dataset = browserDataset(database, projects);
	const datasetBytes = new TextEncoder().encode(JSON.stringify(dataset));
	const applicationAssets = await buildApplicationAssets();
	const assets = writeAssets(applicationAssets, datasetBytes);
	const files = new Map<string, Uint8Array>();
	const context: PageContext = {
		applicationAssets: assets.application,
		datasetUrl: assets.dataset,
		release: dataset.dataset_release,
		liveReload: options.liveReload ?? false,
	};

	addText(files, 'index.html', homePage(database, context));
	addText(files, 'about/index.html', aboutPage(context));
	addText(files, 'dataset/index.html', datasetPage(database, context));
	addText(files, 'short/index.html', shortPage(database, context));
	addText(files, '404.html', notFoundPage(context));
	for (const project of projects) {
		addText(files, `projects/${project}/index.html`, projectPage(database, project, context));
		addBytes(files, `projects/${project}/providers.json`, exportBytes(database, { project }));
	}
	for (const executable of executables) {
		addText(files, `executables/${executable}/index.html`, executablePage(database, executable, context));
		addBytes(files, `executables/${executable}/providers.json`, exportBytes(database, { executable }));
	}
	for (const [name, content] of applicationAssets) {
		addBytes(files, `assets/app.${assets.hash}/${name}`, content);
	}
	addBytes(files, `assets/dataset.${sha256(datasetBytes)}.json`, datasetBytes);
	writeHeaders(files);
	await writeRedirects(files, dataset.dataset_release);
	return files;
}

export async function generateSite(databasePath: string, outputPath: string,
	options: BuildOptions = {}): Promise<void> {
	const output = resolve(outputPath);
	if (await pathExists(output)) throw new Error(`website output already exists: ${output}`);
	const database = await WebsiteDatabase.open(databasePath);
	try {
		const files = await buildSite(database, options);
		const parent = dirname(output);
		await mkdir(parent, { recursive: true });
		const staging = await mkdtemp(join(parent, '.bnck-site-'));
		try {
			for (const [path, content] of files) {
				const target = join(staging, path);
				await mkdir(dirname(target), { recursive: true });
				/* Create new files only; this also detects case collisions on case-insensitive filesystems. */
				await writeFile(target, content, { flag: 'wx' });
			}
			if (await pathExists(output)) throw new Error(`website output already exists: ${output}`);
			await rename(staging, output);
		} catch (error) {
			await rm(staging, { recursive: true, force: true });
			throw error;
		}
	} finally {
		database.close();
	}
}

async function buildApplicationAssets(): Promise<Map<string, Uint8Array>> {
	const result = await Bun.build({
		entrypoints: BROWSER_ENTRYPOINTS,
		target: 'browser',
		format: 'esm',
		minify: false,
		sourcemap: 'none',
	});
	if (!result.success) {
		const details = result.logs.map((log) => String(log)).join('\n');
		throw new Error(`build website assets${details ? `:\n${details}` : ''}`);
	}
	const assets = new Map<string, Uint8Array>();
	for (const output of result.outputs) {
		assets.set(basename(output.path), new Uint8Array(await output.arrayBuffer()));
	}
	assets.set('style.css', await Bun.file(STYLE_PATH).bytes());
	for (const name of ['browser.js', 'search.js', 'exports.js', 'style.css']) {
		if (!assets.has(name)) throw new Error(`website asset was not built: ${name}`);
	}
	return assets;
}

function writeAssets(applicationAssets: Map<string, Uint8Array>, dataset: Uint8Array): SiteAssets & { hash: string } {
	const hash = applicationHash(applicationAssets);
	return {
		hash,
		application: `/assets/app.${hash}`,
		dataset: `/assets/dataset.${sha256(dataset)}.json`,
	};
}

function applicationHash(assets: Map<string, Uint8Array>): string {
	/* Keep relative module imports together without invalidating application assets for dataset updates. */
	const digest = createHash('sha256');
	for (const name of [...assets.keys()].sort()) {
		const content = assets.get(name)!;
		const length = Buffer.alloc(8);
		length.writeBigUInt64LE(BigInt(content.length));
		digest.update(name).update(length).update(content);
	}
	return digest.digest('hex');
}

function sha256(content: Uint8Array): string {
	return createHash('sha256').update(content).digest('hex');
}

function addText(files: Map<string, Uint8Array>, path: string, content: string): void {
	addBytes(files, path, new TextEncoder().encode(content));
}

function addBytes(files: Map<string, Uint8Array>, path: string, content: Uint8Array): void {
	if (files.has(path)) throw new Error(`website file was generated twice: ${path}`);
	files.set(path, content);
}

function writeHeaders(files: Map<string, Uint8Array>): void {
	let headers = `/*\n  X-Content-Type-Options: nosniff\n  Content-Security-Policy: ${HTML_CSP}\n`;
	/* Avoid overlapping cache rules: some hosts combine rather than override matching headers. */
	for (const path of [
		'/',
		'/index.html',
		'/about',
		'/about/*',
		'/dataset',
		'/dataset/*',
		'/short',
		'/short/*',
		'/projects/*',
		'/executables/*',
		'/404.html',
	]) {
		headers += `\n${path}\n  Cache-Control: no-cache\n`;
	}
	headers += '\n/assets/*\n  Cache-Control: public, max-age=31536000, immutable\n';
	addText(files, '_headers', headers);
}

async function writeRedirects(files: Map<string, Uint8Array>, release: string): Promise<void> {
	const template = await readFile(REDIRECTS_PATH, 'utf8');
	addText(files, '_redirects', template.split('{{release}}').join(release));
}

function validateFileName(name: string): void {
	if (name === '.' || name === '..' || name.includes('/') || name.includes('\\') || name.includes('\0')) {
		throw new Error(`identity ${JSON.stringify(name)} cannot be represented as a static page path`);
	}
}

async function pathExists(path: string): Promise<boolean> {
	try {
		await stat(path);
		return true;
	} catch (error) {
		if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
		throw error;
	}
}

