export const PAGE_SIZE = 30;
export const RELATED_LIMIT = 3;

export function foldAscii(value) {
	return value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

export function compareNames(left, right) {
	/* SQLite BINARY order follows Unicode scalar values, not JavaScript's UTF-16 code units or locale. */
	const a = [...left];
	const b = [...right];
	for (let index = 0; index < Math.min(a.length, b.length); index++) {
		const difference = a[index].codePointAt(0) - b[index].codePointAt(0);
		if (difference) return difference;
	}
	return a.length - b.length;
}

export function encodeSegment(value) {
	return encodeURIComponent(value).replace(/[!'()*]/g, (character) =>
		`%${character.charCodeAt(0).toString(16).toUpperCase()}`);
}

export function parseSearch(parameters, channels) {
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

export function searchUrl(search, changes = {}) {
	const { query, channel, initial, page } = { ...search, ...changes };
	return `/?q=${encodeSegment(query)}&channel=${encodeSegment(channel)}` +
		`&initial=${encodeSegment(initial)}&page=${page}`;
}

function searchableName(name) {
	return { name, folded: foldAscii(name), length: [...name].length };
}

export function buildIndex(dataset) {
	return dataset.projects.map((project) => {
		const executables = new Map();
		for (const provider of project.providers) {
			if (!executables.has(provider.executable)) executables.set(provider.executable, new Set());
			executables.get(provider.executable).add(provider.channel);
		}
		return {
			...searchableName(project.name),
			related: [...executables].map(([name, channels]) => ({
				...searchableName(name), channels: [...channels].sort(compareNames),
			})),
		};
	});
}

function matchRank(name, query, folded) {
	if (name.name === query) return 0;
	if (name.folded === folded) return 1;
	if (name.folded.startsWith(folded)) return 2;
	if (name.folded.includes(folded)) return 3;
	return 4;
}

function initialOf(name) {
	return /^[a-zA-Z]/.test(name) ? name[0].toUpperCase() : '#';
}

function rankProject(project, search, folded) {
	const related = project.related
		.filter((relation) => !search.channel || relation.channels.includes(search.channel))
		.map((relation) => ({
			...relation, channels: search.channel ? [search.channel] : relation.channels,
			rank: matchRank(relation, search.query, folded),
		}));
	if (!related.length) return null;
	const ranked = [{ ...project, rank: matchRank(project, search.query, folded) }, ...related]
		.sort((a, b) => a.rank - b.rank || a.length - b.length);
	if (ranked[0].rank === 4) return null;
	related.sort((a, b) => a.rank - b.rank || (search.query ? a.length - b.length : 0) ||
		compareNames(a.name, b.name));
	return { name: project.name, related, rank: ranked[0].rank, length: ranked[0].length };
}

export function queryIndex(index, search) {
	const folded = foldAscii(search.query);
	const matches = index.map((project) => rankProject(project, search, folded)).filter(Boolean);
	const initials = [...new Set(matches.map((project) => initialOf(project.name)))].sort(compareNames);
	const filtered = matches.filter((project) => !search.initial || initialOf(project.name) === search.initial);
	filtered.sort((a, b) => a.rank - b.rank || (search.query ? a.length - b.length : 0) ||
		compareNames(a.name, b.name));
	const offset = (search.page - 1) * PAGE_SIZE;
	const rows = filtered.slice(offset, offset + PAGE_SIZE);
	if (search.page > 1 && !rows.length) throw new Error('No results on this page. Start a new search or browse names.');
	return {
		rows, initials, total: filtered.length,
		start: rows.length ? offset + 1 : 0,
		end: offset + rows.length,
		previous: search.page > 1 ? searchUrl(search, { page: search.page - 1 }) : null,
		next: offset + rows.length < filtered.length ? searchUrl(search, { page: search.page + 1 }) : null,
	};
}
