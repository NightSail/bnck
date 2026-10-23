import {
	SEARCH_PAGE_SIBLINGS,
	SEARCH_PAGE_SIZE,
	SEARCH_RELATED_NAME_LIMIT,
} from './config';
import { compareNames, encodeSegment, foldAscii } from './naming';
import type { AssociationRole, BrowserDataset, SearchPageLink } from './types';

export { compareNames, encodeSegment, foldAscii };
export const PAGE_SIZE = SEARCH_PAGE_SIZE;
export const RELATED_LIMIT = SEARCH_RELATED_NAME_LIMIT;
export const PAGE_SIBLINGS = SEARCH_PAGE_SIBLINGS;

export interface Search {
	query: string;
	channel: string;
	initial: string;
	page: number;
	isHome: boolean;
}

export interface IndexedName {
	name: string;
	folded: string;
	length: number;
}

export interface IndexedRelation extends IndexedName {
	channels: string[];
	role?: AssociationRole;
}

export interface IndexedProject extends IndexedName {
	related: IndexedRelation[];
}

export interface RankedRelation extends IndexedRelation {
	rank: number;
}

export interface RankedProject extends IndexedName {
	related: RankedRelation[];
	rank: number;
}

export interface IndexedSearchResults {
	rows: RankedProject[];
	initials: string[];
	total: number;
	start: number;
	end: number;
	page: number;
	pages: number;
	page_links: SearchPageLink[];
	previous: string | null;
	next: string | null;
}

export function parseSearch(parameters: URLSearchParams, channels: readonly { name: string }[]): Search {
	for (const key of ['q', 'channel', 'initial', 'page']) {
		if (parameters.getAll(key).length > 1) throw new Error('Search parameters must not be repeated.');
	}
	const query = (parameters.get('q') ?? '').replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
	const channel = parameters.get('channel') ?? '';
	const initial = parameters.get('initial') ?? '';
	const pageText = parameters.get('page') ?? '1';
	const page = Number(pageText);
	if (!/^\+?\d+$/.test(pageText) || !Number.isSafeInteger(page) || page < 1 || page > 0xffffffff ||
		(channel && !channels.some((item) => item.name === channel)) || (initial && !/^[A-Z#]$/.test(initial))) {
		throw new Error('Check the initial, channel, and page number, or start a new search.');
	}
	if ([...query].length > 100) throw new Error('Use at most 100 characters.');
	return {
		query, channel, initial, page,
		isHome: !parameters.has('q') && !channel && !initial && page === 1,
	};
}

export function searchUrl(
	search: Search,
	changes: Partial<Pick<Search, 'query' | 'channel' | 'initial' | 'page'>> = {},
): string {
	const { query, channel, initial, page } = { ...search, ...changes };
	return `/?q=${encodeSegment(query)}&channel=${encodeSegment(channel)}` +
		`&initial=${encodeSegment(initial)}&page=${page}`;
}

/* Page numbers around the current page, with null marking an elided range. */
export function pageNumbers(page: number, pages: number, siblings = PAGE_SIBLINGS): (number | null)[] {
	if (pages < 2) return [];
	const visible = new Set([1, pages]);
	for (let number = page - siblings; number <= page + siblings; number++) {
		if (number >= 1 && number <= pages) visible.add(number);
	}
	const numbers = [...visible].sort((left, right) => left - right);
	const items: (number | null)[] = [];
	let previous = 0;
	for (const number of numbers) {
		if (previous && number - previous > 1) items.push(number - previous === 2 ? previous + 1 : null);
		items.push(number);
		previous = number;
	}
	return items;
}

function searchableName(name: string): IndexedName {
	return { name, folded: foldAscii(name), length: [...name].length };
}

export function buildIndex(dataset: BrowserDataset): IndexedProject[] {
	return dataset.projects.map((project) => {
		const executables = new Map<string, Set<string>>();
		for (const provider of project.providers) {
			const channels = executables.get(provider.executable) ?? new Set<string>();
			channels.add(provider.channel);
			executables.set(provider.executable, channels);
		}
		const related: IndexedRelation[] = [...executables].map(([name, channels]) => ({
			...searchableName(name),
			channels: [...channels].sort(compareNames),
		}));
		/* Associations supplement provider relations; they have no channel scope. */
		for (const association of project.associations) {
			if (!executables.has(association.executable)) {
				related.push({
					...searchableName(association.executable),
					channels: [],
					role: association.role,
				});
			}
		}
		return { ...searchableName(project.name), related };
	});
}

function matchRank(name: IndexedName, query: string, folded: string): number {
	if (name.name === query) return 0;
	if (name.folded === folded) return 1;
	if (name.folded.startsWith(folded)) return 2;
	if (name.folded.includes(folded)) return 3;
	return 4;
}

function initialOf(name: string): string {
	return /^[a-zA-Z]/.test(name) ? name[0].toUpperCase() : '#';
}

function rankProject(project: IndexedProject, search: Search, folded: string): RankedProject | null {
	const related = project.related
		.filter((relation) => !search.channel || relation.channels.includes(search.channel))
		.map((relation) => ({
			...relation,
			channels: search.channel ? [search.channel] : relation.channels,
			rank: matchRank(relation, search.query, folded),
		}));
	if (!related.length) return null;
	const candidates = [
		{ rank: matchRank(project, search.query, folded), length: project.length },
		...related.map((relation) => ({ rank: relation.rank, length: relation.length })),
	].sort((a, b) => a.rank - b.rank || a.length - b.length);
	const best = candidates[0];
	if (!best || best.rank === 4) return null;
	related.sort((a, b) => a.rank - b.rank || (search.query ? a.length - b.length : 0) ||
		compareNames(a.name, b.name));
	return { ...project, related, rank: best.rank, length: best.length };
}

export function queryIndex(index: IndexedProject[], search: Search): IndexedSearchResults {
	const folded = foldAscii(search.query);
	const matches = index.map((project) => rankProject(project, search, folded))
		.filter((project): project is RankedProject => project !== null);
	const initials = [...new Set(matches.map((project) => initialOf(project.name)))].sort(compareNames);
	const filtered = matches.filter((project) => !search.initial || initialOf(project.name) === search.initial);
	filtered.sort((a, b) => a.rank - b.rank || (search.query ? a.length - b.length : 0) ||
		compareNames(a.name, b.name));
	const offset = (search.page - 1) * PAGE_SIZE;
	const rows = filtered.slice(offset, offset + PAGE_SIZE);
	if (search.page > 1 && !rows.length) throw new Error('No results on this page. Start a new search or browse names.');
	const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
	const page_links = pageNumbers(search.page, pages).map((number) => number === null ? {
		label: '…', url: null, is_current: false,
	} : {
		label: String(number), url: searchUrl(search, { page: number }), is_current: number === search.page,
	});
	return {
		rows, initials, total: filtered.length,
		start: rows.length ? offset + 1 : 0,
		end: offset + rows.length,
		page: search.page,
		pages,
		page_links,
		previous: search.page > 1 ? searchUrl(search, { page: search.page - 1 }) : null,
		next: offset + rows.length < filtered.length ? searchUrl(search, { page: search.page + 1 }) : null,
	};
}
