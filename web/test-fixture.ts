import { Database } from 'bun:sqlite';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type { AssociationRole, Provider } from './types';

export interface FixtureProjectLink {
	project: string;
	link_type: string;
	url: string;
}

export interface Fixture {
	path: string;
	release: string;
	cleanup: () => Promise<void>;
}

export interface FixtureAssociation {
	project: string;
	executable: string;
	role: AssociationRole;
}

export function provider(project: string, executable: string, channel: string,
	packageName = `${project}-package`): Provider {
	return { project, executable, channel, package: packageName };
}

export async function createFixture(
	providers: Provider[],
	links: FixtureProjectLink[] = [],
	options: { compressed?: boolean; release?: string; generatedAt?: string; associations?: FixtureAssociation[] } = {},
): Promise<Fixture> {
	const root = await mkdtemp(join(tmpdir(), 'bnck-web-'));
	const release = options.release ?? '2026-04-16.1';
	const generatedAt = options.generatedAt ?? '2026-04-16T12:00:00Z';
	const source = join(root, 'bnck.sqlite');
	const database = new Database(source, { create: true, readwrite: true });
	database.exec(`
		CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
		CREATE TABLE providers (
			project TEXT NOT NULL CHECK (length(project) > 0),
			executable TEXT NOT NULL CHECK (length(executable) > 0 AND instr(executable, '/') = 0),
			channel TEXT NOT NULL CHECK (channel IN ('arch', 'crates.io', 'debian', 'fedora', 'homebrew', 'npm', 'pypi')),
			package TEXT NOT NULL CHECK (length(package) > 0),
			PRIMARY KEY (project, executable, channel, package)
		) WITHOUT ROWID;
		CREATE INDEX providers_by_executable ON providers (executable, project, channel, package);
		CREATE TABLE name_associations (
			executable TEXT NOT NULL CHECK (length(executable) > 0 AND instr(executable, '/') = 0),
			project TEXT NOT NULL CHECK (length(project) > 0),
			role TEXT NOT NULL CHECK (role IN ('reference', 'implementation')),
			PRIMARY KEY (executable, project)
		) WITHOUT ROWID;
		CREATE INDEX name_associations_by_project ON name_associations (project, executable, role);
		CREATE TABLE project_links (
			project TEXT NOT NULL,
			type TEXT NOT NULL CHECK (type IN ('homepage', 'repository', 'documentation', 'issue tracker', 'link')),
			url TEXT NOT NULL CHECK (length(url) > 0),
			PRIMARY KEY (project, type, url)
		) WITHOUT ROWID;
	`);
	const insertProvider = database.prepare('INSERT INTO providers VALUES (?1, ?2, ?3, ?4)');
	for (const record of providers) {
		insertProvider.run(record.project, record.executable, record.channel, record.package);
	}
	const insertAssociation = database.prepare('INSERT INTO name_associations VALUES (?1, ?2, ?3)');
	const associations = options.associations ?? [];
	for (const association of associations) {
		insertAssociation.run(association.executable, association.project, association.role);
	}
	const insertLink = database.prepare('INSERT INTO project_links VALUES (?1, ?2, ?3)');
	for (const link of links) insertLink.run(link.project, link.link_type, link.url);
	const metadata = database.prepare('INSERT INTO metadata VALUES (?1, ?2)');
	for (const [key, value] of [
		['schema_version', '2'],
		['dataset_schema_version', '1'],
		['association_schema_version', '1'],
		['dataset_release', release],
		['dataset_sha256', '0'.repeat(64)],
		['provider_count', String(providers.length)],
		['association_sha256', '0'.repeat(64)],
		['association_count', String(associations.length)],
		['generated_at', generatedAt],
		['generator_version', '0.1.0'],
		['generator_revision', '0123456789abcdef'],
	]) {
		metadata.run(key, value);
	}
	database.exec('PRAGMA user_version = 2;');
	database.close();

	let path = source;
	if (options.compressed) {
		path = `${source}.zst`;
		await writeFile(path, Bun.zstdCompressSync(await readFile(source)));
		await rm(source);
	}
	return { path, release, cleanup: () => rm(root, { recursive: true, force: true }) };
}
