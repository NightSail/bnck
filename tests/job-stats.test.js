import { afterEach, beforeEach, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const size = resolve(import.meta.dir, '../justfiles/size.sh');
let root;

beforeEach(() => {
	root = mkdtempSync(join(tmpdir(), 'bnck job stats '));
});

afterEach(() => rmSync(root, { recursive: true, force: true }));

function run(label, path) {
	const result = Bun.spawnSync([size, label, path], { stdout: 'pipe', stderr: 'pipe' });
	return { exitCode: result.exitCode, stdout: result.stdout.toString(), stderr: result.stderr.toString() };
}

test('reports a file size in IEC units', () => {
	const file = join(root, 'artifact.jsonl');
	writeFileSync(file, Buffer.alloc(1024));
	const result = run('Wrote artifact', file);
	expect(result.exitCode).toBe(0);
	expect(result.stdout).toBe('\nWrote artifact: 1.0KiB\n');
});

test('reports a directory size recursively', () => {
	const directory = join(root, 'input');
	mkdirSync(join(directory, 'packages'), { recursive: true });
	writeFileSync(join(directory, 'packages/one.json'), '{}');
	const result = run('Pulled into input', directory);
	expect(result.exitCode).toBe(0);
	expect(result.stdout).toMatch(/^\nPulled into input: \d/);
});

test('fails when the path does not exist', () => {
	const result = run('Pulled into input', join(root, 'missing'));
	expect(result.exitCode).not.toBe(0);
	expect(result.stderr).toContain('Cannot measure missing path');
});
