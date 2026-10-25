import { createDatasetLoader, type DatasetLoader } from './dataset';
import { mountJsonExport } from './exports';
import {
	executableAssociations,
	executableContent,
	executableExportPayload,
	executableProviders,
	findProject,
	parseIdentityPath,
	projectContent,
	projectExportPayload,
} from './identity-view';
import { searchForm } from './components';
import { html, type Html } from './html';

export interface IdentityWindow {
	location: { href: string; pathname: string; hash: string };
}

export interface IdentityDocument {
	title: string;
	querySelector<T extends Element = Element>(selector: string): T | null;
	getElementById(id: string): HTMLElement | null;
}

export function mountIdentity(
	window: IdentityWindow,
	document: IdentityDocument,
	loadDataset: DatasetLoader,
): Promise<void> {
	const main = document.querySelector<HTMLElement>('#main');
	const status = document.querySelector<HTMLElement>('#identity-status');
	if (!main || !status) throw new Error('identity page is missing #main or #identity-status');
	const route = parseIdentityPath(window.location.pathname);
	if (!route) {
		display(document, main, status, errorContent('page not found',
			'This address does not identify a project or executable.'), 'page not found · bnck(1)',
		'page not found');
		return Promise.resolve();
	}

	display(document, main, status, html`<section class='page-heading detail-heading'>
	<a class='back-link' href='/?q='>← browse names</a>
	<p class='eyebrow'>${route.kind}</p>
	<h1><code>${route.name}</code></h1>
</section>
<p class='detail-note'>Loading ${route.kind} data…</p>`, `${route.kind}: ${route.name} · bnck(1)`, 'loading dataset');
	main.setAttribute('aria-busy', 'true');

	return loadDataset().then(({ dataset }) => {
		if (route.kind === 'project') {
			const project = findProject(dataset, route.name);
			if (!project) {
				display(document, main, status, notFoundContent(route.kind, route.name),
					`${route.kind}: ${route.name} · bnck(1)`, `${route.kind} not listed`);
				return;
			}
			display(document, main, status, projectContent(project), `project: ${route.name} · bnck(1)`,
				`project ${route.name}`);
			mountJsonExport(main, projectExportPayload(project, dataset.dataset_release));
			restoreFragment(window, document);
			return;
		}
		const providers = executableProviders(dataset, route.name);
		const associations = executableAssociations(dataset, route.name);
		if (!providers.length && !associations.length) {
			display(document, main, status, notFoundContent(route.kind, route.name),
				`${route.kind}: ${route.name} · bnck(1)`, `${route.kind} not listed`);
			return;
		}
		display(document, main, status, executableContent(route.name, providers, associations),
			`executable: ${route.name} · bnck(1)`, `executable ${route.name}`);
		mountJsonExport(main, executableExportPayload(route.name, providers, associations,
			dataset.dataset_release));
		restoreFragment(window, document);
	}).catch((error: unknown) => {
		const message = `Details could not be loaded. Check your connection and retry. ${(error as Error).message}`;
		display(document, main, status, errorContent('details unavailable', message, true),
			'details unavailable · bnck(1)', message);
		const retry = main.querySelector<HTMLButtonElement>('[data-retry-identity]');
		retry?.addEventListener('click', () => { void mountIdentity(window, document, loadDataset); });
	});
}

function display(document: IdentityDocument, main: HTMLElement, status: HTMLElement, content: Html,
	title: string, summary: string): void {
	main.innerHTML = content.value;
	main.removeAttribute('aria-busy');
	document.title = title;
	status.textContent = summary;
}

function restoreFragment(window: IdentityWindow, document: IdentityDocument): void {
	const hash = window.location.hash;
	if (!hash.startsWith('#')) return;
	let id: string;
	try {
		id = decodeURIComponent(hash.slice(1));
	} catch {
		return;
	}
	document.getElementById(id)?.scrollIntoView();
}

function notFoundContent(kind: string, name: string): Html {
	return html`<section class='error-page'>
	<p class='eyebrow'>not listed</p>
	<h1>${kind} not found</h1>
	<p class='lead'><code>${name}</code> is not listed in this dataset release.
		Check the spelling, or search for another name.</p>
	${searchForm()}
	<p class='search-hint' id='search-hint'>Search by full or partial name.
		<a href='/?q='>browse names →</a></p>
</section>`;
}

function errorContent(title: string, message: string, canRetry = false): Html {
	return html`<section class='error-page'>
	<p class='eyebrow'>error</p>
	<h1>${title}</h1>
	<p class='lead'>${message}</p>
	${canRetry ? html`<button class='text-link' type='button' data-retry-identity>retry</button>` : ''}
	${searchForm()}
</section>`;
}

if (typeof document !== 'undefined') {
	const metadata = document.querySelector<HTMLMetaElement>('meta[name=bnck-dataset]');
	if (metadata) {
		void mountIdentity(window, document,
			createDatasetLoader(metadata.content, metadata.dataset.release ?? ''));
	}
}
