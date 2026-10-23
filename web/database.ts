import { Database } from 'bun:sqlite';
import { stat } from 'node:fs/promises';
import { extname } from 'node:path';

import { SEARCH_PAGE_SIZE, SEARCH_PAGE_SIBLINGS } from './config';
import { encodeSegment } from './naming';
import type {
	ChannelStats,
	DatasetStats,
	ExecutableProvider,
	ProjectLink,
	ProjectProvider,
	SearchFilterLink,
	SearchPageLink,
	SearchQuery,
	SearchResult,
	SearchResults,
} from './types';

/* Discovery folds ASCII case; grouping and detail lookups preserve exact identities. */
const SEARCH_MATCHES = `WITH matching_names AS (
	SELECT project AS name, executable AS matched_name
	FROM providers
	WHERE instr(lower(executable), lower(?1)) > 0 AND (?2 = '' OR channel = ?2)
	UNION
	SELECT project AS name, project AS matched_name
	FROM providers
	WHERE instr(lower(project), lower(?1)) > 0 AND (?2 = '' OR channel = ?2)
), ranked_names AS (
	SELECT name, matched_name,
		CASE
			WHEN matched_name = ?1 THEN 0
			WHEN lower(matched_name) = lower(?1) THEN 1
			WHEN instr(lower(matched_name), lower(?1)) = 1 THEN 2
			ELSE 3
		END AS match_rank
	FROM matching_names
), best_ranks AS (
	SELECT name, min(match_rank) AS match_rank FROM ranked_names GROUP BY name
), matches AS (
	SELECT name, match_rank, min(length(matched_name)) AS match_length
	FROM ranked_names JOIN best_ranks USING (name, match_rank)
	GROUP BY name, match_rank
)`;

const SEARCH_INITIAL_FILTER = `WHERE ?3 = ''
	OR (?3 = '#' AND substr(name, 1, 1) NOT GLOB '[a-zA-Z]')
	OR upper(substr(name, 1, 1)) = ?3`;

const REQUIRED_METADATA = [
	'dataset_schema_version',
	'dataset_release',
	'dataset_sha256',
	'provider_count',
	'generated_at',
	'generator_version',
	'generator_revision',
];

interface MetadataRow {
	key: string;
	value: string;
}

interface CountRow {
	count: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function requireNumber(value: unknown, description: string): number {
	if (typeof value !== 'number' || !Number.isSafeInteger(value)) {
		throw new Error(`${description} is not an integer`);
	}
	return value;
}

function requireString(value: unknown, description: string): string {
	if (typeof value !== 'string') throw new Error(`${description} is not a string`);
	return value;
}

function parseInteger(value: string, description: string): number {
	if (!/^\d+$/.test(value)) throw new Error(`${description} is not an integer`);
	const number = Number(value);
	if (!Number.isSafeInteger(number)) throw new Error(`${description} is not an integer`);
	return number;
}

export class WebsiteDatabase {
	readonly metadata: ReadonlyMap<string, string>;
	readonly stats: DatasetStats;
	readonly channels: ChannelStats[];
	readonly example: SearchResult | null;

	private constructor(
		private readonly database: Database,
		metadata: Map<string, string>,
		stats: DatasetStats,
		channels: ChannelStats[],
		example: SearchResult | null,
	) {
		this.metadata = metadata;
		this.stats = stats;
		this.channels = channels;
		this.example = example;
	}

	static async open(source: string): Promise<WebsiteDatabase> {
		const info = await stat(source);
		if (!info.isFile()) throw new Error(`database does not exist: ${source}`);

		const bytes = await Bun.file(source).bytes();
		const databaseBytes = extname(source) === '.zst' ? Bun.zstdDecompressSync(bytes) : bytes;
		const database = Database.deserialize(databaseBytes);
		database.exec('PRAGMA query_only = ON;');

		const schemaVersion = readUserVersion(database);
		const metadata = readMetadata(database);
		const metadataSchema = parseInteger(requiredMetadata(metadata, 'schema_version'), 'metadata schema_version');
		if (metadataSchema !== schemaVersion) {
			throw new Error('SQLite and metadata schema versions disagree');
		}
		for (const key of REQUIRED_METADATA) requiredMetadata(metadata, key);

		const stats = readStats(database);
		const expectedProviderCount = parseInteger(
			requiredMetadata(metadata, 'provider_count'),
			'metadata provider_count',
		);
		if (stats.providers !== expectedProviderCount) {
			throw new Error('provider count does not match database metadata');
		}

		const channels = readChannelStats(database);
		const example = readExample(database);
		return new WebsiteDatabase(database, metadata, stats, channels, example);
	}

	close(): void {
		this.database.close();
	}

	metadataValue(key: string): string {
		return this.metadata.get(key) ?? 'unknown';
	}

	listProjects(): string[] {
		return (this.database.query('SELECT DISTINCT project AS name FROM providers ORDER BY project')
			.all() as { name: string }[]).map((row) => row.name);
	}

	listExecutables(): string[] {
		return (this.database.query('SELECT DISTINCT executable AS name FROM providers ORDER BY executable')
			.all() as { name: string }[]).map((row) => row.name);
	}

	shortNameUsage(): Map<string, number> {
		const rows = this.database.query(
			`SELECT executable AS name, count(DISTINCT project) AS projects
			 FROM providers WHERE length(executable) IN (2, 3)
			 GROUP BY executable`,
		).all() as { name: string; projects: number }[];
		return new Map(rows.map((row) => [row.name, row.projects]));
	}

	projectProviders(project: string): ProjectProvider[] {
		return this.database
			.query('SELECT executable, channel, package FROM providers WHERE project = ?1 ORDER BY executable, channel, package')
			.all(project) as ProjectProvider[];
	}

	projectLinks(project: string): ProjectLink[] {
		return this.database.query(
			`SELECT type AS link_type, url FROM project_links
			 WHERE project = ?1
			 ORDER BY CASE type
				WHEN 'homepage' THEN 0
				WHEN 'repository' THEN 1
				WHEN 'documentation' THEN 2
				WHEN 'issue tracker' THEN 3
				ELSE 4
			 END, url`,
		).all(project) as ProjectLink[];
	}

	executableProviders(executable: string): ExecutableProvider[] {
		return this.database
			.query('SELECT project, channel, package FROM providers WHERE executable = ?1 ORDER BY project, channel, package')
			.all(executable) as ExecutableProvider[];
	}

	search(search: SearchQuery): SearchResults {
		const query = searchQueryText(search);
		const totalRow = this.database
			.query(`${SEARCH_MATCHES} SELECT count(*) AS count FROM matches ${SEARCH_INITIAL_FILTER}`)
			.get(query, search.channel, search.initial) as CountRow | null;
		const total = requireNumber(totalRow?.count, 'search total');

		const offset = Math.max(0, search.page - 1) * SEARCH_PAGE_SIZE;
		const names = this.database.query(
			`${SEARCH_MATCHES} SELECT name FROM matches ${SEARCH_INITIAL_FILTER}
			 ORDER BY match_rank, CASE WHEN ?1 != '' THEN match_length END, name
			 LIMIT ?4 OFFSET ?5`,
		).all(query, search.channel, search.initial, SEARCH_PAGE_SIZE, offset) as { name: string }[];
		const rows = names.map(({ name }) => this.readSearchResult(name, search));
		const end = offset + rows.length;
		const pages = Math.max(1, Math.ceil(total / SEARCH_PAGE_SIZE));
		const initialLinks = searchInitialLinks(search, this.readSearchInitials(search));
		return {
			initial_links: initialLinks,
			start: rows.length === 0 ? 0 : offset + 1,
			end,
			total,
			page: search.page,
			pages,
			page_links: searchPageLinks(search, pages),
			previous: search.page > 1 ? searchPageUrl(search, search.page - 1) : null,
			next: end < total ? searchPageUrl(search, search.page + 1) : null,
			rows,
		};
	}

	private readSearchInitials(search: SearchQuery): string[] {
		/* Derive available initials from all query/channel matches, before initial filtering and pagination. */
		return (this.database.query(
			`${SEARCH_MATCHES}
			 SELECT DISTINCT CASE WHEN substr(name, 1, 1) GLOB '[a-zA-Z]'
				THEN upper(substr(name, 1, 1)) ELSE '#' END AS initial
			 FROM matching_names ORDER BY initial`,
		).all(searchQueryText(search), search.channel) as { initial: string }[]).map((row) => row.initial);
	}

	private readSearchResult(name: string, search: SearchQuery): SearchResult {
		return { name, related: this.readSearchRelations(name, search) };
	}

	private readSearchRelations(name: string, search: SearchQuery): SearchResult['related'] {
		/* Keep matching commands in the preview, even for projects with many executables. */
		return readSearchRelations(this.database, name, search);
	}
}

function readSearchRelations(database: Database, name: string, search: SearchQuery): SearchResult['related'] {
	/* Keep matching commands in the preview, even for projects with many executables. */
	const rows = database.query(
		`SELECT DISTINCT executable, channel FROM providers
		 WHERE project = ?1 AND (?2 = '' OR channel = ?2)
		 ORDER BY CASE
			WHEN executable = ?3 THEN 0
			WHEN lower(executable) = lower(?3) THEN 1
			WHEN instr(lower(executable), lower(?3)) = 1 THEN 2
			WHEN instr(lower(executable), lower(?3)) > 0 THEN 3
			ELSE 4
		 END, CASE WHEN ?3 != '' THEN length(executable) END, executable, channel`,
	).all(name, search.channel, searchQueryText(search)) as { executable: string; channel: string }[];
	const related: SearchResult['related'] = [];
	for (const row of rows) {
		const previous = related.at(-1);
		if (previous?.name === row.executable) previous.channels.push(row.channel);
		else related.push({ name: row.executable, channels: [row.channel] });
	}
	return related;
}

function readUserVersion(database: Database): number {
	const row = database.query('PRAGMA user_version').get();
	if (!isRecord(row)) throw new Error('SQLite user_version is unavailable');
	return requireNumber(row.user_version, 'SQLite user_version');
}

function readMetadata(database: Database): Map<string, string> {
	const rows = database.query('SELECT key, value FROM metadata ORDER BY key').all() as MetadataRow[];
	return new Map(rows.map((row) => [row.key, row.value]));
}

function requiredMetadata(metadata: ReadonlyMap<string, string>, key: string): string {
	const value = metadata.get(key);
	if (!value) throw new Error(`database metadata is missing ${key}`);
	return value;
}

function readStats(database: Database): DatasetStats {
	const row = database.query(
		`SELECT count(*) AS providers, count(DISTINCT project) AS projects,
			count(DISTINCT executable) AS executables, count(DISTINCT channel) AS channels
		 FROM providers`,
	).get();
	if (!isRecord(row)) throw new Error('dataset statistics are unavailable');
	return {
		providers: requireNumber(row.providers, 'provider count'),
		projects: requireNumber(row.projects, 'project count'),
		executables: requireNumber(row.executables, 'executable count'),
		channels: requireNumber(row.channels, 'channel count'),
	};
}

function readChannelStats(database: Database): ChannelStats[] {
	return database.query(
		`SELECT channel AS name, count(DISTINCT project) AS projects, count(DISTINCT package) AS packages
		 FROM providers GROUP BY channel ORDER BY channel`,
	).all() as ChannelStats[];
}

function readExample(database: Database): SearchResult | null {
	const row = database.query(
		`SELECT project, executable FROM providers
		 ORDER BY CASE WHEN project = 'age' AND executable = 'age' THEN 0
			WHEN executable = 'rg' THEN 1 WHEN executable = 'hx' THEN 2 ELSE 3 END,
		 project, executable LIMIT 1`,
	).get();
	if (!isRecord(row)) return null;
	const search: SearchQuery = {
		q: requireString(row.executable, 'example executable'),
		channel: '',
		initial: '',
		page: 1,
	};
	const project = requireString(row.project, 'example project');
	return { name: project, related: readSearchRelations(database, project, search) };
}

function searchQueryText(search: SearchQuery): string {
	return (search.q ?? '').replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
}

function searchInitialLinks(search: SearchQuery, initials: string[]): SearchFilterLink[] {
	return ['', ...initials].map((initial) => ({
		label: initial === '' ? 'all' : initial,
		url: searchUrl(search, search.channel, initial, 1),
		is_current: initial === search.initial,
	}));
}

export function searchPageUrl(search: SearchQuery, page: number): string {
	return searchUrl(search, search.channel, search.initial, page);
}

function searchUrl(search: SearchQuery, channel: string, initial: string, page: number): string {
	return `/?q=${encodeSegment(searchQueryText(search))}&channel=${encodeSegment(channel)}` +
		`&initial=${encodeSegment(initial)}&page=${page}`;
}

/* Page numbers around the current page, with null marking an elided range. */
function searchPageLinks(search: SearchQuery, pages: number): SearchPageLink[] {
	if (pages < 2) return [];
	const start = Math.max(1, search.page - SEARCH_PAGE_SIBLINGS);
	const end = Math.min(pages, search.page + SEARCH_PAGE_SIBLINGS);
	const numbers = [...new Set([1, pages, ...Array.from({ length: end - start + 1 }, (_, index) => start + index)])]
		.sort((left, right) => left - right);
	const pageLink = (number: number): SearchPageLink => ({
		label: String(number),
		url: searchPageUrl(search, number),
		is_current: number === search.page,
	});
	const links: SearchPageLink[] = [];
	let previous = 0;
	for (const number of numbers) {
		if (previous !== 0 && number - previous > 1) {
			/* A single missing page is cheaper to show than to elide. */
			if (number - previous === 2) links.push(pageLink(previous + 1));
			else links.push({ label: '…', url: null, is_current: false });
		}
		links.push(pageLink(number));
		previous = number;
	}
	return links;
}
