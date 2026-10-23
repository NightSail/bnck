import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const script = resolve(import.meta.dir, '../.github/scripts/render-release-report.sh');

test.skipIf(!Bun.which('jq'))('renders dataset counts and channel summaries from a release manifest', () => {
	const root = mkdtempSync(join(tmpdir(), 'bnck release report '));
	try {
		const manifest = join(root, 'manifest.json');
		writeFileSync(manifest, JSON.stringify({
			release: '2026-05-01.1',
			generated_at: '2026-05-01T12:00:00Z',
			dataset: {
				provider_count: 7,
				name_associations: { association_count: 3 },
			},
			channels: [
				{ name: 'debian', status: 'complete', package_count: 100, provider_count: 2 },
				{ name: 'debian', status: 'complete', package_count: 200, provider_count: 2 },
				{ name: 'npm', status: 'partial', package_count: 1, provider_count: 1 },
			],
		}));
		const result = Bun.spawnSync([script, manifest], { stdout: 'pipe', stderr: 'pipe' });
		expect(result.exitCode).toBe(0);
		const output = result.stdout.toString();
		expect(output).toContain('Dataset release `2026-05-01.1`, generated at `2026-05-01T12:00:00Z`.');
		expect(output).toContain('| Provider relations | 7 |');
		expect(output).toContain('| Name associations | 3 |');
		expect(output).toContain('| debian | complete | 300 | 4 |');
		expect(output).toContain('| npm | partial | 1 | 1 |');
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});
