import { RELEASES_URL, REPOSITORY_URL } from './config';
import { searchForm } from './components';
import { html, Html, raw } from './html';

export interface PageShell {
	title: string;
	applicationAssets: string;
	datasetUrl?: string;
	clientSearch: boolean;
	release: string;
	section: 'index' | 'short' | 'about' | 'dataset';
	hasHeaderSearch: boolean;
	scripts?: Html;
	liveReload?: boolean;
	content: Html;
}

function current(section: string, name: string): Html {
	return section === name ? raw("aria-current='page'") : raw('');
}

export function renderPage(shell: PageShell): string {
	const datasetMeta = shell.clientSearch && shell.datasetUrl
		? html`<meta name='bnck-dataset' content='${shell.datasetUrl}' data-release='${shell.release}'>
	<script type='module' src='${shell.applicationAssets}/browser.js'></script>`
		: '';
	const navigation = shell.hasHeaderSearch
		? html`<div class='header-search'>
			${searchForm()}
			<p class='visually-hidden' id='search-hint'>Search by full or partial name.</p>
		</div>
		<a href='/short' ${current(shell.section, 'short')}>short names</a>`
		: html`<a href='/' ${current(shell.section, 'index')}>search</a>
		<a href='/short' ${current(shell.section, 'short')}>short names</a>`;
	const clientNotices = shell.clientSearch
		? html`<noscript><p class='filter-notice'>Search and filtered browsing require JavaScript.
	Project pages, executable pages, and JSON exports can be opened directly without it.</p></noscript>
	<p id='search-status' class='visually-hidden' role='status' aria-live='polite'></p>`
		: '';
	const liveReload = shell.liveReload
		? html`<script type='module' src='/__reload.js'></script>`
		: '';
	const page = html`<!doctype html>
<html lang='en'>
<head>
	<meta charset='utf-8'>
	<meta name='viewport' content='width=device-width, initial-scale=1'>
	<meta name='description'
		content='bnck maps executable names to projects and distribution packages.'>
	<title>${shell.title} · bnck(1)</title>
	<link rel='stylesheet' href='${shell.applicationAssets}/style.css'>
	<script type='module' src='${shell.applicationAssets}/search.js'></script>
	${datasetMeta}
	${shell.scripts ?? ''}
	${liveReload}
</head>
<body>
	<a class='skip-link' href='#main'>skip to content</a>
	<header class='site-header${shell.hasHeaderSearch ? ' has-search' : ''}'>
		<a class='brand' href='/' aria-label='bnck(1): bin name check, home'>
			<span title='manual section 1: executable programs and shell commands'>bnck(1)</span>
			<span class='brand-description'>bin name check</span>
		</a>
		<nav aria-label='Main navigation'>
			${navigation}
			<a href='/dataset' ${current(shell.section, 'dataset')}>dataset</a>
			<a href='/about' ${current(shell.section, 'about')}>about</a>
		</nav>
		<a class='release-tag' href='/dataset#release'><span>release</span> ${shell.release}</a>
	</header>
	${clientNotices}
	<main id='main' tabindex='-1'>${shell.content}</main>
	<footer class='site-footer'>
		<p><strong>bnck(1)</strong> <span>bin name check</span></p>
		<nav class='footer-links' aria-label='Project links'>
			<a href='/short'>short names</a>
			<a href='/dataset'>dataset</a>
			<a href='/about'>about</a>
			<a href='${RELEASES_URL}' rel='external'>releases <span aria-hidden='true'>↗</span></a>
			<a href='${REPOSITORY_URL}' rel='external'>source <span aria-hidden='true'>↗</span></a>
		</nav>
	</footer>
</body>
</html>`;
	return page.value;
}

