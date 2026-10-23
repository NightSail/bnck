import { createDatasetLoader, type DatasetLoader } from './dataset';
import { parseSearch, queryIndex } from './query';
import { renderMessage, renderResults, resultSummary } from './results';

export interface SearchWindow {
	location: { href: string };
	history: { pushState(data: unknown, unused: string, url?: string | URL | null): void };
	scrollTo(x: number, y: number): void;
	addEventListener(type: string, listener: () => void): void;
}

export interface SearchDocument {
	title: string;
	querySelector<T extends Element = Element>(selector: string): T | null;
	addEventListener(type: string, handler: (event: any) => void): void;
}

export function mountSearch(window: SearchWindow, document: SearchDocument, loadDataset: DatasetLoader): Promise<void> {
	const main = document.querySelector<HTMLElement>('#main');
	const status = document.querySelector<HTMLElement>('#search-status');
	if (!main || !status) throw new Error('search page is missing #main or #search-status');
	const mainElement = main;
	const statusElement = status;
	const homeHtml = mainElement.innerHTML;
	const homeTitle = document.title;
	let navigation = 0;

	function display(html: string, title: string, summary: string, options: { shouldFocus?: boolean }): void {
		mainElement.innerHTML = html;
		mainElement.removeAttribute('aria-busy');
		document.title = title;
		statusElement.textContent = summary;
		const input = document.querySelector<HTMLInputElement>('#q');
		if (input) {
			input.setAttribute('aria-keyshortcuts', '/');
			input.title = 'press / to focus search';
		}
		if (options.shouldFocus) {
			mainElement.focus();
			window.scrollTo(0, 0);
		}
	}

	async function showLocation(options: { shouldFocus?: boolean } = {}): Promise<void> {
		const current = ++navigation;
		const parameters = new URL(window.location.href).searchParams;
		if (!['q', 'channel', 'initial', 'page'].some((key) => parameters.has(key))) {
			display(homeHtml, homeTitle, '', options);
			return;
		}
		const searchInput = {
			query: parameters.get('q') ?? '', channel: parameters.get('channel') ?? '',
			initial: parameters.get('initial') ?? '',
		};
		const loading = 'Loading the search dataset…';
		display(renderMessage('loading search', loading, { search: searchInput }), 'search · bnck(1)', loading, {});
		mainElement.setAttribute('aria-busy', 'true');
		let loaded;
		try {
			loaded = await loadDataset();
		} catch (error) {
			if (current !== navigation) return;
			const message = `Search could not be loaded. Check your connection and retry. ${(error as Error).message}`;
			display(renderMessage('search unavailable', message, { canRetry: true, search: searchInput }),
				'search unavailable · bnck(1)', message, options);
			return;
		}
		/* A slow fetch must not overwrite a newer navigation, including returning to the home page. */
		if (current !== navigation) return;
		try {
			const search = parseSearch(parameters, loaded.dataset.channels);
			if (search.isHome) {
				display(homeHtml, homeTitle, '', options);
				return;
			}
			const results = queryIndex(loaded.index, search);
			display(renderResults(search, results, loaded.dataset.channels),
				`${search.query ? `search: ${search.query}` : 'browse'} · bnck(1)`, resultSummary(search, results), options);
		} catch (error) {
			const message = (error as Error).message;
			display(renderMessage('invalid search', message), 'invalid search · bnck(1)', message, options);
		}
	}

	function navigate(url: URL): Promise<void> {
		if (url.href !== window.location.href) window.history.pushState(null, '', url);
		return showLocation({ shouldFocus: true });
	}

	bindNavigation(window, document, navigate, showLocation);
	return showLocation();
}

function bindNavigation(
	window: SearchWindow,
	document: SearchDocument,
	navigate: (url: URL) => Promise<void>,
	showLocation: (options?: { shouldFocus?: boolean }) => Promise<void>,
): void {
	document.addEventListener('submit', (event: any) => {
		const form = event.target;
		if (event.defaultPrevented || !form.matches('form.search-form, form.page-jump')) return;
		event.preventDefault();
		const url = new URL('/', window.location.href);
		url.search = new URLSearchParams([...new FormData(form).entries()]
			.map(([name, value]) => [name, String(value)])).toString();
		void navigate(url);
	});
	document.addEventListener('click', (event: any) => {
		if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey ||
			event.shiftKey || event.altKey) return;
		if (event.target.closest('[data-retry-search]')) {
			void showLocation({ shouldFocus: true });
			return;
		}
		const anchor = event.target.closest('a[href]');
		if (!anchor || anchor.hasAttribute('download') || anchor.target || anchor.rel === 'external') return;
		const url = new URL(anchor.href, window.location.href);
		if (new URL(url.href).origin !== new URL(window.location.href).origin || url.pathname !== '/' || url.hash) return;
		event.preventDefault();
		void navigate(url);
	});
	window.addEventListener('popstate', () => { void showLocation(); });
}

if (typeof document !== 'undefined') {
	const metadata = document.querySelector<HTMLMetaElement>('meta[name=bnck-dataset]');
	if (metadata) {
		void mountSearch(window, document, createDatasetLoader(metadata.content, metadata.dataset.release ?? ''));
	}
}
