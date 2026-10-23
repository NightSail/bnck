import { afterEach, beforeEach, expect, test } from 'bun:test';
import {
	chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const just = Bun.which('just');
if (!just) throw new Error('npm job tests require just.');
const justfile = resolve(import.meta.dir, '../justfile');
const fixture = join(import.meta.dir, 'fixtures/npm-job-command.sh');
let root;
let log;
let inputs;
let selection;

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), 'bnck npm jobs '));
	log = join(root, 'commands.log');
	inputs = join(root, 'var/input/npm');
	selection = join(inputs, 'current');
	mkdirSync(join(root, 'bin'));
	for (const command of ['curl', 'sleep', 'bnck-test']) {
		const path = join(root, 'bin', command);
		copyFileSync(fixture, path);
		chmodSync(path, 0o755);
	}
	/* Exercise nested recipes in the same temporary directory, with no network requests. */
	const nestedJust = join(root, 'bin/just');
	writeFileSync(nestedJust, '#!/usr/bin/env bash\n'
		+ 'exec "$BNCK_TEST_JUST" --justfile "$BNCK_TEST_JUSTFILE" --working-directory "$BNCK_TEST_ROOT" "$@"\n');
	chmodSync(nestedJust, 0o755);
});

afterEach(() => rmSync(root, { recursive: true, force: true }));

function run(job, environment = {}, variables = {}) {
	const arguments_ = [just, '--justfile', justfile, '--working-directory', root, '--set', 'bnck_bulk', 'bnck-test'];
	for (const [key, value] of Object.entries(variables)) arguments_.push('--set', key, value);
	const result = Bun.spawnSync([...arguments_, ...job], {
		env: {
			...process.env,
			PATH: `${join(root, 'bin')}:${process.env.PATH}`,
			BNCK_TEST_LOG: log,
			BNCK_TEST_JUST: just,
			BNCK_TEST_JUSTFILE: justfile,
			BNCK_TEST_ROOT: root,
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
	const result = run(['fetch-npm']);
	expect(result.exitCode).toBe(0);
	const snapshot = readFileSync(selection, 'utf8').trimEnd();
	expect(result.stdout).toContain(snapshot);
	expect(result.stdout).toContain(`Pulled into ${snapshot}: `);
	expect(existsSync(join(snapshot, 'complete'))).toBe(true);
	return snapshot;
}

test('argument-free fetch and import use one planned snapshot, never the old per-package cache', () => {
	mkdirSync(inputs, { recursive: true });
	writeFileSync(join(inputs, 'eslint.json'), 'old demo cache');
	const snapshot = fetchSnapshot();
	expect(commands()[0]).toEqual(['bnck-test', 'plan-npm', '--curations', 'curations',
		'--output-directory', `${snapshot}/packages`, '--output', `${snapshot}/metadata.json`]);
	const requests = commands().filter((call) => call[0] === 'curl');
	expect(requests.map((call) => call.at(-1))).toEqual([
		'https://registry.npmjs.org/one/latest', 'https://registry.npmjs.org/%40scope%2Ftwo/latest',
	]);
	for (const request of requests) {
		expect(request).toContain('--compressed');
		expect(request[request.indexOf('--retry') + 1]).toBe('3');
	}
	writeFileSync(log, '');
	const imported = run(['import-npm']);
	expect(imported.exitCode).toBe(0);
	expect(imported.stdout).toContain('Wrote var/acquired/npm.jsonl: ');
	expect(commands()).toEqual([['bnck-test', 'import-npm', '--plan', `${snapshot}/metadata.json`,
		'--output', 'var/acquired/npm.jsonl', '--context', `${snapshot}/acquisition-context.tsv`,
		'--scope', 'registry=https://registry.npmjs.org', '--scope', 'release-policy=latest-dist-tag']]);
	expect(readFileSync(join(inputs, 'eslint.json'), 'utf8')).toBe('old demo cache');
});

test('fresh runs refresh selected releases and retain previous snapshots', () => {
	const first = fetchSnapshot();
	writeFileSync(log, '');
	const second = fetchSnapshot();
	expect(second).not.toBe(first);
	expect(existsSync(join(first, 'packages/one.json'))).toBe(true);
	expect(commands().filter((call) => call[0] === 'curl').length).toBe(2);
});

test('failed refreshes preserve the last completed selection and resume only missing responses', () => {
	const previous = fetchSnapshot();
	const result = run(['fetch-npm'], { BNCK_TEST_RESPONSE_FILE: 'two.json.tmp', BNCK_TEST_RESPONSE_STATUS: '503' });
	expect(result.exitCode).not.toBe(0);
	expect(readFileSync(selection, 'utf8')).toBe(`${previous}\n`);
	const failed = readdirSync(inputs).map((name) => join(inputs, name))
		.find((path) => path !== previous && existsSync(join(path, 'metadata.json')));
	expect(existsSync(join(failed, 'packages/one.json'))).toBe(true);
	expect(existsSync(join(failed, 'packages/two.json'))).toBe(false);
	expect(existsSync(join(failed, 'complete'))).toBe(false);
	expect(run(['import-npm', failed]).exitCode).not.toBe(0);
	writeFileSync(log, '');
	expect(run(['resume-npm', failed]).exitCode).toBe(0);
	expect(commands().filter((call) => call[0] === 'curl').map((call) => call.at(-1)))
		.toEqual(['https://registry.npmjs.org/%40scope%2Ftwo/latest']);
	expect(commands().some((call) => call[1] === 'plan-npm')).toBe(false);
	expect(readFileSync(selection, 'utf8')).toBe(`${previous}\n`);
	expect(run(['import-npm', failed]).exitCode).toBe(0);
	writeFileSync(log, '');
	expect(run(['resume-npm', failed]).exitCode).toBe(0);
	expect(commands()).toEqual([]);
});

test('an explicit snapshot and output do not change default selections or production output', () => {
	const previous = fetchSnapshot();
	const snapshot = join(root, 'explicit snapshot');
	expect(run(['fetch-npm', snapshot]).exitCode).toBe(0);
	expect(readFileSync(selection, 'utf8')).toBe(`${previous}\n`);
	writeFileSync(log, '');
	expect(run(['fetch-npm', snapshot]).exitCode).not.toBe(0);
	expect(commands()).toEqual([]);
	const output = join(root, 'demo/npm.jsonl');
	expect(run(['import-npm', snapshot, output]).exitCode).toBe(0);
	expect(commands()[0]).toEqual(['bnck-test', 'import-npm', '--plan', `${snapshot}/metadata.json`,
		'--output', output, '--context', `${snapshot}/acquisition-context.tsv`,
		'--scope', 'registry=https://registry.npmjs.org', '--scope', 'release-policy=latest-dist-tag']);
});

test('missing selections, failed plans, and incomplete snapshots fail before importing', () => {
	expect(run(['import-npm']).stderr).toContain('Run just fetch-npm first');
	expect(commands()).toEqual([]);
	mkdirSync(inputs, { recursive: true });
	for (const contents of ['', '\n', `${join(inputs, 'missing')}\n`]) {
		writeFileSync(selection, contents);
		expect(run(['import-npm']).exitCode).not.toBe(0);
	}
	rmSync(selection);
	expect(run(['fetch-npm'], { BNCK_TEST_PLAN_FAILURE: '1' }).exitCode).not.toBe(0);
	expect(existsSync(selection)).toBe(false);
	expect(commands().filter((call) => call[0] === 'curl')).toEqual([]);
	mkdirSync(selection);
	expect(run(['fetch-npm']).stderr).toContain('npm input selection must be a file');
});

test('unavailable latest releases are retained and later packages are still fetched', () => {
	for (const status of ['404', '410']) {
		const snapshot = join(root, status);
		const result = run(['fetch-npm', snapshot], {
			BNCK_TEST_RESPONSE_FILE: 'one.json.tmp', BNCK_TEST_RESPONSE_STATUS: status,
		});
		expect(result.exitCode).toBe(0);
		expect(readFileSync(join(snapshot, 'packages/one.status'), 'utf8')).toBe(`${status}\n`);
		expect(existsSync(join(snapshot, 'packages/two.json'))).toBe(true);
		writeFileSync(log, '');
		expect(run(['resume-npm', snapshot]).exitCode).toBe(0);
		expect(commands()).toEqual([]);
	}
});

test('invalid HTTP responses never mark the snapshot complete', () => {
	for (const status of ['302', '429', '503']) {
		const snapshot = join(root, status);
		expect(run(['fetch-npm', snapshot], {
			BNCK_TEST_RESPONSE_FILE: 'one.json.tmp', BNCK_TEST_RESPONSE_STATUS: status,
		}).exitCode).not.toBe(0);
		expect(existsSync(join(snapshot, 'complete'))).toBe(false);
	}
});

test('catalog jobs are removed', () => {
	const summary = Bun.spawnSync([just, '--justfile', justfile, '--summary']);
	expect(summary.exitCode).toBe(0);
	expect(summary.stdout.toString().split(/\s+/)).not.toContain('fetch-npm-index');
	expect(summary.stdout.toString().split(/\s+/)).not.toContain('fetch-registry-metadata');
});

test('resume refetches a body without status and rejects invalid retained status', () => {
	const snapshot = fetchSnapshot();
	const status = join(snapshot, 'packages/one.status');
	rmSync(status);
	writeFileSync(log, '');
	expect(run(['resume-npm', snapshot]).exitCode).toBe(0);
	expect(commands().filter((call) => call[0] === 'curl').map((call) => call.at(-1)))
		.toEqual(['https://registry.npmjs.org/one/latest']);
	writeFileSync(status, '503\n');
	writeFileSync(log, '');
	expect(run(['resume-npm', snapshot]).exitCode).not.toBe(0);
	expect(commands()).toEqual([]);
});

test('fetch and import honor the configured npm input root', () => {
	const variables = { npm_input: 'custom inputs' };
	expect(run(['fetch-npm'], {}, variables).exitCode).toBe(0);
	expect(existsSync(selection)).toBe(false);
	const snapshot = readFileSync(join(root, 'custom inputs/current'), 'utf8').trimEnd();
	writeFileSync(log, '');
	expect(run(['import-npm'], {}, variables).exitCode).toBe(0);
	expect(commands()[0]).toContain(`${snapshot}/metadata.json`);
});
