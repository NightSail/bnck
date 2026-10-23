import { afterEach, beforeEach, expect, test } from 'bun:test';
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';

const just = Bun.which('just');
if (!just) throw new Error('Language job tests require just.');
const justfile = resolve(import.meta.dir, '../justfile');
const fixture = join(import.meta.dir, 'fixtures/language-job-command.sh');
let root;
let inputs;
let selection;
let log;

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), 'bnck language jobs '));
	inputs = join(root, 'var/input/languages');
	selection = join(inputs, 'current');
	log = join(root, 'commands.log');
	mkdirSync(join(root, 'bin'));
	/* Exercise the real aggregate recipes without network requests or Rust imports. */
	for (const command of ['just', 'bnck-test']) {
		const path = join(root, 'bin', command);
		copyFileSync(fixture, path);
		chmodSync(path, 0o755);
	}
});

afterEach(() => rmSync(root, { recursive: true, force: true }));

function run(job, environment = {}, variables = {}) {
	const commandArguments = ['--justfile', justfile, '--working-directory', root, '--set', 'bnck_bulk', 'bnck-test'];
	for (const [key, value] of Object.entries(variables)) commandArguments.push('--set', key, value);
	const jobs = Array.isArray(job) ? job : [job];
	const result = Bun.spawnSync([just, ...commandArguments, ...jobs], {
		env: {
			...process.env,
			PATH: `${join(root, 'bin')}:${process.env.PATH}`,
			BNCK_TEST_LOG: log,
			BNCK_TEST_CURRENT: selection,
			...environment,
		},
		stdout: 'pipe', stderr: 'pipe',
	});
	return { exitCode: result.exitCode, stdout: result.stdout.toString(), stderr: result.stderr.toString() };
}

function commands() {
	return existsSync(log) ? readFileSync(log, 'utf8').trimEnd().split('\n').filter(Boolean)
		.map((line) => line.split('\t')) : [];
}

function fetchSnapshot() {
	const result = run('fetch-languages');
	expect(result.exitCode).toBe(0);
	const snapshot = readFileSync(selection, 'utf8').trimEnd();
	expect(result.stdout).toContain(snapshot);
	return snapshot;
}

test('crates acquisition exposes only dump jobs and the demo avoids the dump', () => {
	const summary = Bun.spawnSync([just, '--justfile', justfile, '--summary']);
	expect(summary.exitCode).toBe(0);
	const recipes = summary.stdout.toString().trim().split(/\s+/);
	expect(recipes).toContain('fetch-crates-dump');
	expect(recipes).toContain('import-crates-dump');
	expect(recipes).not.toContain('fetch-crate');
	expect(recipes).not.toContain('import-crates-api');
	const dump = Bun.spawnSync([just, '--justfile', justfile, '--dry-run', 'fetch-crates-dump']);
	expect(dump.exitCode).toBe(0);
	const fetch = dump.stderr.toString();
	expect(fetch.match(/curl /g)?.length).toBe(1);
	expect(fetch).toContain('https://static.crates.io/db-dump.tar.gz');
	expect(fetch).toContain('*/data/crates.csv');
	expect(fetch).toContain('*/data/versions.csv');
	expect(fetch).not.toContain('crates.io/api/');
	const demo = Bun.spawnSync([just, '--justfile', justfile, '--dry-run', 'demo']);
	expect(demo.exitCode).toBe(0);
	expect(demo.stderr.toString()).toContain('var/demo/run.XXXXXX');
	expect(demo.stderr.toString()).toContain('just fetch-npm "$snapshot/npm"');
	expect(demo.stderr.toString()).toContain('"$snapshot/acquired/pypi.jsonl" fetch-pypi black import-pypi');
	expect(demo.stderr.toString()).not.toContain('var/acquired');
	expect(demo.stderr.toString()).not.toContain('crates.io');
});

test('PyPI metadata fetching plans curated packages directly without a global index', () => {
	const snapshot = join(root, 'pypi snapshot');
	expect(run(['fetch-pypi-metadata', snapshot]).exitCode).toBe(0);
	expect(commands()).toEqual([
		['bnck-test', 'plan-pypi-metadata', '--curations', 'curations',
			'--output-directory', `${snapshot}/packages`, '--output', `${snapshot}/metadata.json`],
		['just', 'fetch-registry-plan', 'pypi', `${snapshot}/metadata.tsv`],
	]);
	writeFileSync(log, '');
	expect(run(['fetch-pypi-metadata', snapshot]).exitCode).not.toBe(0);
	expect(commands()).toEqual([]);
	const failed = join(root, 'failed snapshot');
	expect(run(['fetch-pypi-metadata', failed], { BNCK_TEST_FAIL_JOB: 'plan-pypi-metadata' }).exitCode).not.toBe(0);
	expect(commands().length).toBe(1);
	const summary = Bun.spawnSync([just, '--justfile', justfile, '--summary']);
	expect(summary.exitCode).toBe(0);
	const recipes = summary.stdout.toString().trim().split(/\s+/);
	expect(recipes).toContain('fetch-pypi-metadata');
	expect(recipes).not.toContain('fetch-pypi-index');
	expect(recipes).not.toContain('fetch-registry-metadata');
});

test('argument-free fetches create distinct retained snapshots under var/input/languages', () => {
	const first = fetchSnapshot();
	expect(dirname(first)).toBe(inputs);
	expect(basename(first)).toMatch(/^\d{4}-\d{2}-\d{2}T\d{6}Z\.[a-zA-Z0-9]+$/);
	expect(commands()).toEqual([
		['just', 'fetch-crates-dump', `${first}/crates`],
		['just', 'fetch-npm', `${first}/npm`],
		['just', 'fetch-pypi-metadata', `${first}/pypi`],
		['just', 'fetch-pypi-wheels', `${first}/pypi`],
	]);
	writeFileSync(join(first, 'npm/metadata.json'), 'retained input');
	const second = fetchSnapshot();
	expect(second).not.toBe(first);
	expect(dirname(second)).toBe(inputs);
	expect(readFileSync(join(first, 'npm/metadata.json'), 'utf8')).toBe('retained input');
	expect(readFileSync(selection, 'utf8')).toBe(`${second}\n`);
});

test('a failed first fetch is not selected for import', () => {
	expect(run('fetch-languages', { BNCK_TEST_FAIL_JOB: 'fetch-npm' }).exitCode).not.toBe(0);
	expect(existsSync(selection)).toBe(false);
	writeFileSync(log, '');
	const result = run('import-languages');
	expect(result.exitCode).not.toBe(0);
	expect(result.stderr).toContain('Run just fetch-languages first');
	expect(commands()).toEqual([]);
});

test('failure at any fetch stage leaves the last completed inputs selected', () => {
	const previous = fetchSnapshot();
	for (const job of ['fetch-crates-dump', 'fetch-npm', 'fetch-pypi-metadata', 'fetch-pypi-wheels']) {
		const result = run('fetch-languages', { BNCK_TEST_FAIL_JOB: job });
		expect(result.exitCode).not.toBe(0);
		expect(result.stderr).toContain(`Simulated failure: ${job}`);
		expect(readFileSync(selection, 'utf8')).toBe(`${previous}\n`);
	}
});

test('argument-free imports pin one completed snapshot even if selection changes mid-import', () => {
	const first = fetchSnapshot();
	const second = fetchSnapshot();
	writeFileSync(log, '');
	const result = run('import-languages', { BNCK_TEST_RESELECT: first });
	expect(result.exitCode).toBe(0);
	expect(readFileSync(selection, 'utf8')).toBe(`${first}\n`);
	expect(commands()).toEqual([
		['just', 'import-crates-dump', `${second}/crates`],
		['just', 'import-npm', `${second}/npm`, 'var/acquired/npm.jsonl'],
		['bnck-test', 'import-pypi', '--plan', `${second}/pypi/wheels.json`, '--output', 'var/acquired/pypi.jsonl',
			'--context', `${second}/pypi/acquisition-context.tsv`,
			'--scope', 'index=https://pypi.org/simple/', '--scope', 'release-policy=latest-non-yanked-stable',
			'--scope', 'wheel-policy=prefer-universal-then-smallest', '--scope', 'catalog-policy=bounded-non-atomic-scan'],
	]);
});

test('missing or invalid selections fail before starting imports', () => {
	const missing = run('import-languages');
	expect(missing.exitCode).not.toBe(0);
	expect(missing.stderr).toContain('Run just fetch-languages first');
	mkdirSync(inputs, { recursive: true });
	for (const value of ['', '\n', `${join(inputs, 'missing')}\n`]) {
		writeFileSync(selection, value);
		const result = run('import-languages');
		expect(result.exitCode).not.toBe(0);
		expect(result.stderr).toContain('Invalid language input selection');
	}
	expect(commands()).toEqual([]);
});

test('a directory at the selection path is rejected before downloading', () => {
	mkdirSync(selection, { recursive: true });
	const result = run('fetch-languages');
	expect(result.exitCode).not.toBe(0);
	expect(result.stderr).toContain('Language input selection must be a file');
	expect(commands()).toEqual([]);
});

test('a configured input root applies to both fetch and import', () => {
	const variables = { languages_input: 'custom inputs' };
	expect(run('fetch-languages', {}, variables).exitCode).toBe(0);
	const customSelection = join(root, 'custom inputs/current');
	const snapshot = readFileSync(customSelection, 'utf8').trimEnd();
	expect(dirname(snapshot)).toBe(join(root, 'custom inputs'));
	expect(existsSync(selection)).toBe(false);
	writeFileSync(log, '');
	expect(run('import-languages', {}, variables).exitCode).toBe(0);
	expect(commands()[0]).toEqual(['just', 'import-crates-dump', `${snapshot}/crates`]);
});
