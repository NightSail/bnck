import { createDatasetLoader } from './dataset.js';
import { parseSearch, queryIndex } from './query.js';
import { renderMessage, renderResults, resultSummary } from './results.js';

export function mountSearch(window, document, loadDataset) {
	const main = document.querySelector('#main');
	const status = document.querySelector('#search-status');
	const homeHtml = main.innerHTML;
	const homeTitle = document.title;
	let navigation = 0;

	function display(html, title, summary, options) {
		main.innerHTML = html;
		main.removeAttribute('aria-busy');
		document.title = title;
		status.textContent = summary;
		const input = document.querySelector('#q');
		input?.setAttribute('aria-keyshortcuts', '/');
		if (input) input.title = 'Press / to focus search';
		if (options.shouldFocus) {
			main.focus();
			window.scrollTo(0, 0);
		}
	}

	async function showLocation(options = {}) {
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
		display(renderMessage('Loading search', loading, { search: searchInput }), 'Search · bnck(1)', loading, {});
		main.setAttribute('aria-busy', 'true');
		let loaded;
		try {
			loaded = await loadDataset();
		} catch (error) {
			if (current !== navigation) return;
			const message = `Search could not be loaded. Check your connection and retry. ${error.message}`;
			display(renderMessage('Search unavailable', message, { canRetry: true, search: searchInput }),
				'Search unavailable · bnck(1)', message, options);
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
				`${search.query ? `Search: ${search.query}` : 'Browse'} · bnck(1)`, resultSummary(search, results), options);
		} catch (error) {
			display(renderMessage('Invalid search', error.message), 'Invalid search · bnck(1)', error.message, options);
		}
	}

	function navigate(url) {
		if (url.href !== window.location.href) window.history.pushState(null, '', url);
		return showLocation({ shouldFocus: true });
	}

	bindNavigation(window, document, navigate, showLocation);
	return showLocation();
}

function bindNavigation(window, document, navigate, showLocation) {
	document.addEventListener('submit', (event) => {
		const form = event.target;
		if (event.defaultPrevented || !form.matches('form.search-form, form.page-jump')) return;
		event.preventDefault();
		const url = new URL('/', window.location.href);
		url.search = new URLSearchParams(new FormData(form)).toString();
		void navigate(url);
	});
	document.addEventListener('click', (event) => {
		if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey ||
			event.shiftKey || event.altKey) return;
		if (event.target.closest('[data-retry-search]')) {
			void showLocation({ shouldFocus: true });
			return;
		}
		const anchor = event.target.closest('a[href]');
		if (!anchor || anchor.hasAttribute('download') || anchor.target || anchor.rel === 'external') return;
		const url = new URL(anchor.href, window.location.href);
		if (url.origin !== window.location.origin || url.pathname !== '/' || url.hash) return;
		event.preventDefault();
		void navigate(url);
	});
	window.addEventListener('popstate', () => { void showLocation(); });
}

if (typeof document !== 'undefined') {
	const metadata = document.querySelector('meta[name=bnck-dataset]');
	if (metadata) {
		void mountSearch(window, document, createDatasetLoader(metadata.content, metadata.dataset.release));
	}
}
