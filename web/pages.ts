import {
	LICENSE_URL,
	RELEASES_URL,
	REPOSITORY_URL,
	SEARCH_RELATED_NAME_LIMIT,
	channelLabel,
} from './config';
import {
	associationList,
	countLabel,
	coverageNote,
	exportActions,
	projectCoverage,
	providerGroup,
	searchForm,
	searchResult,
	stats,
} from './components';
import { buildProjectCoverage, groupExecutableProjects, shortSectionSlug } from './data';
import { encodeSegment, formatNumber } from './naming';
import { html, raw, type Html } from './html';
import { renderPage } from './layout';
import type { WebsiteDatabase } from './database';
import type { ShortNameSection } from './types';

export interface PageContext {
	applicationAssets: string;
	datasetUrl: string;
	release: string;
	liveReload: boolean;
}

export function homePage(database: WebsiteDatabase, context: PageContext): string {
	const example = database.example;
	const featured = example
		? searchResult(example, 'featured-project', SEARCH_RELATED_NAME_LIMIT, '',
			`/projects/${encodeSegment(example.name)}`)
		: html`<p class='name-result muted'>No executables listed in this release.</p>`;
	return renderPage({
		title: '',
		applicationAssets: context.applicationAssets,
		datasetUrl: context.datasetUrl,
		clientSearch: true,
		release: context.release,
		section: 'index',
		hasHeaderSearch: false,
		liveReload: context.liveReload,
		content: html`<section class='hero'>
	<div class='hero-copy'>
		<h1>executable lookup</h1>
		<p class='lead'>Look up a command to find its projects and packages.
			Look up a project to list its commands.</p>
		${searchForm()}
		<p class='search-hint' id='search-hint'>Search by full or partial name.
			<a href='/?q='>browse names <span aria-hidden='true'>→</span></a></p>
	</div>
	<aside class='relation-card' aria-label='Featured project'>
		${featured}
	</aside>
</section>
${stats(database.stats)}
<section class='channel-section' aria-labelledby='channels-title'>
	<div><p class='eyebrow' id='channels-title'>channels in this release</p></div>
	<div class='channel-links'>
		${database.channels.map((channel) => html`<a href='/?q=&channel=${encodeSegment(channel.name)}'>
			${channelLabel(channel.name)} <span aria-hidden='true'>→</span>
		</a>`)}
	</div>
</section>`,
	});
}

export function aboutPage(context: PageContext): string {
	return renderPage({
		title: 'about',
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'about',
		hasHeaderSearch: false,
		liveReload: context.liveReload,
		content: html`<section class='page-heading doc-heading'>
	<p class='eyebrow'>about</p>
	<h1>bnck(1)</h1>
	<p class='lead'>bnck maps executable names to the projects and distribution packages that provide them.</p>
</section>
<div class='doc-layout'>
	<nav class='section-nav' aria-label='On this page'>
		<a href='#lookup'>lookup</a>
		<a href='#data'>data</a>
		<a href='#project'>project</a>
	</nav>
	<div class='doc-body'>
		<section id='lookup'>
			<h2>lookup</h2>
			<p>Search for a command to find its projects and packages, or search for a project to list its commands.
				Matches use full or partial executable names and project IDs; package names are not searched.</p>
			<p>Project pages show package coverage across channels. Executable pages show every project that provides
				the exact name. Package links open the current channel pages.</p>
			<p>bnck does not install, build, or execute packages, and the dataset does not include package versions,
				platform support, or installation instructions.</p>
		</section>
		<section id='data'>
			<h2>data</h2>
			<p>The dataset is built from Debian, Fedora, Arch Linux, Homebrew, crates.io, npm, and PyPI metadata,
				then resolved through curated project mappings.</p>
			<p>Coverage is limited to accepted records in each release. A missing command or package may still be
				available, and channel counts do not describe the full contents of a channel.</p>
			<p><a class='text-link' href='/dataset'>dataset fields, coverage, and releases
				<span aria-hidden='true'>→</span></a></p>
		</section>
		<section id='project'>
			<h2>project</h2>
			<p>bnck is open source under the ISC License. Source code, issues, documentation, and dataset releases
				are hosted on GitHub.</p>
			<dl class='definitions'>
				<div>
					<dt>source code</dt>
					<dd><a href='${REPOSITORY_URL}' rel='external'>
						github.com/NightSail/bnck <span aria-hidden='true'>↗</span></a></dd>
				</div>
				<div>
					<dt>issues</dt>
					<dd><a href='${REPOSITORY_URL}/issues' rel='external'>
						github.com/NightSail/bnck/issues <span aria-hidden='true'>↗</span></a></dd>
				</div>
				<div>
					<dt>releases</dt>
					<dd><a href='${RELEASES_URL}' rel='external'>
						GitHub Releases <span aria-hidden='true'>↗</span></a></dd>
				</div>
				<div>
					<dt>contributing</dt>
					<dd><a href='${REPOSITORY_URL}/blob/master/CONTRIBUTING.md' rel='external'>
						CONTRIBUTING.md <span aria-hidden='true'>↗</span></a></dd>
				</div>
				<div>
					<dt>license</dt>
					<dd><a href='${LICENSE_URL}' rel='external'>
						ISC License <span aria-hidden='true'>↗</span></a></dd>
				</div>
			</dl>
		</section>
	</div>
</div>`,
	});
}

export function datasetPage(database: WebsiteDatabase, context: PageContext): string {
	const release = database.metadataValue('dataset_release');
	const generatedAt = database.metadataValue('generated_at');
	return renderPage({
		title: 'dataset',
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'dataset',
		hasHeaderSearch: false,
		liveReload: context.liveReload,
		content: html`<section class='page-heading doc-heading'>
	<h1>dataset</h1>
	<p class='lead'>Fields, coverage, and release artifacts for the current bnck dataset.</p>
</section>
${stats(database.stats)}
<div class='doc-layout'>
	<nav class='section-nav' aria-label='On this page'>
		<a href='#fields'>fields</a>
		<a href='#coverage'>coverage</a>
		<a href='#release'>release</a>
		<a href='#downloads'>downloads</a>
	</nav>
	<div class='doc-body'>
		<section id='fields'>
			<h2>fields</h2>
			<dl class='definitions'>
				<div><dt>executable</dt><dd>Command name, preserving spelling and case.</dd></div>
				<div><dt>project</dt><dd>Software project ID used for lookup.</dd></div>
				<div><dt>channel</dt><dd>Package source, such as Debian or crates.io.</dd></div>
				<div><dt>package</dt><dd>Package name within that channel.</dd></div>
				<div><dt>association</dt><dd>Curated reference or implementation link between a name and a project.
					An association does not claim that a package installs the name.</dd></div>
			</dl>
		</section>
		<section id='coverage'>
			<h2>coverage</h2>
			<p>An unlisted command or package may still be available.</p>
			<p>This dataset does not specify package versions or platform support.</p>
			<table class='coverage-table'>
				<caption>projects and packages by channel</caption>
				<thead>
					<tr>
						<th scope='col'>channel</th><th scope='col'>projects</th>
						<th scope='col'>packages</th>
					</tr>
				</thead>
				<tbody>
					${database.channels.length ? database.channels.map((channel) => html`<tr>
						<th scope='row'>
							<a href='/?q=&channel=${encodeSegment(channel.name)}'>${channelLabel(channel.name)} →</a>
						</th>
						<td>${formatNumber(channel.projects)}</td>
						<td>${formatNumber(channel.packages)}</td>
					</tr>`) : html`<tr><td colspan='3'>No channels listed in this release.</td></tr>`}
				</tbody>
			</table>
			<p class='fine-print'>Counts refer to this dataset, not the full contents of each channel.
				Projects may appear in more than one channel.</p>
		</section>
		<section id='release'>
			<h2>release</h2>
			<dl class='definitions metadata'>
				<div><dt>identifier</dt><dd><code>${release}</code></dd></div>
				<div>
					<dt>generated</dt>
					<dd><time datetime='${generatedAt}'>
						${generatedAt.replace('T', ' ').replace('Z', ' UTC')}
					</time></dd>
				</div>
				<div>
					<dt>release link</dt>
					<dd><a href='/data/${release}'>/data/${release} <span aria-hidden='true'>→</span></a>,
						the versioned link to this dataset</dd>
				</div>
				<div>
					<dt>latest alias</dt>
					<dd><a href='/data/latest'>/data/latest <span aria-hidden='true'>→</span></a>,
						always the current release</dd>
				</div>
				<div>
					<dt>tag</dt>
					<dd><a href='${RELEASES_URL}/tag/bnck-dataset-${release}' rel='external'>
						<code>bnck-dataset-${release}</code> <span aria-hidden='true'>↗</span></a></dd>
				</div>
				<div>
					<dt>all releases</dt>
					<dd><a href='${RELEASES_URL}' rel='external'>
						GitHub Releases <span aria-hidden='true'>↗</span></a></dd>
				</div>
			</dl>
			<p class='fine-print'>bnck and its dataset are licensed under the
				<a href='${LICENSE_URL}' rel='external'>ISC License <span aria-hidden='true'>↗</span></a>.</p>
		</section>
		<section id='downloads'>
			<h2>downloads</h2>
			<p>Every link below points to this release. Use <code>/data/latest</code> for the current release.</p>
			<dl class='definitions'>
				<div>
					<dt>SQLite dataset</dt>
					<dd><a href='/data/${release}/bnck.sqlite.zst'><code>bnck.sqlite.zst</code></a>:
						names, projects, packages, associations, and project links</dd>
				</div>
				<div>
					<dt>release archive</dt>
					<dd><a href='/data/${release}/bnck-${release}.tar.zst'>
						<code>bnck-${release}.tar.zst</code></a>: complete release</dd>
				</div>
				<div>
					<dt>manifest</dt>
					<dd><a href='/data/${release}/manifest.json'><code>manifest.json</code></a>:
						release metadata and artifact digests</dd>
				</div>
				<div>
					<dt>checksums</dt>
					<dd><a href='/data/${release}/SHA256SUMS'><code>SHA256SUMS</code></a> and
						<a href='/data/${release}/SHA256SUMS.minisig'><code>SHA256SUMS.minisig</code></a></dd>
				</div>
			</dl>
			<p class='fine-print'>Verify checksums and signatures before using a downloaded artifact.</p>
		</section>
	</div>
</div>`,
	});
}

export function shortPage(section: ShortNameSection, sections: ShortNameSection[],
	context: PageContext): string {
	const total = sections.reduce((count, candidate) => count + candidate.cells.length, 0);
	const used = sections.reduce((count, candidate) => count + candidate.used, 0);
	const label = section.prefix === '' ? '2 letters' : `3 letters: ${section.prefix}`;
	const description = section.prefix === '' ? 'two-letter names'
		: `three-letter names starting with ${section.prefix}`;
	return renderPage({
		title: `short names: ${section.prefix === '' ? '2 letters' : section.prefix}`,
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'short',
		hasHeaderSearch: false,
		liveReload: context.liveReload,
		content: html`<section class='page-heading short-heading'>
	<p class='eyebrow'>short names</p>
	<h1>short name map</h1>
	<p class='lead'>All 2- and 3-letter lowercase command names.
		Linked names are seen in at least 1 project in this dataset;
		plain grey names are unseen.</p>
	<p class='short-summary'>${countLabel(used, 'name')} seen of ${formatNumber(total)}.</p>
</section>
<nav class='short-nav' aria-label='Short name sections'>
	${sections.map((candidate) => shortNavLink(candidate, section))}
</nav>
${renderShortSection(section, label, description)}`,
	});
}

export function projectPage(database: WebsiteDatabase, project: string, context: PageContext): string {
	const links = database.projectLinks(project);
	const coverage = buildProjectCoverage(database.projectProviders(project));
	const associations = database.projectAssociations(project);
	const references = associations
		.filter((association) => association.role === 'reference')
		.map((association) => ({ name: association.executable, role: association.role }));
	const implementations = associations
		.filter((association) => association.role === 'implementation')
		.map((association) => ({ name: association.executable, role: association.role }));
	const projectLinks = links.length
		? html`<ul class='project-links'>
			${links.map((link) => html`<li>
				${link.link_type}:
				<a href='${link.url}' rel='external'>${link.url}</a>
			</li>`)}
		</ul>`
		: html`<p class='project-links'>No project links listed.</p>`;
	const associationSection = associations.length
		? html`<section class='association-section' aria-labelledby='associations-title'>
	<h2 id='associations-title'>name associations</h2>
	<p class='fine-print'>Curated context about names this project is known for, not package relations.</p>
	${references.length ? html`<div class='association-group'>
		<h3>reference</h3>
		${associationList(references, 'executable')}
	</div>` : ''}
	${implementations.length ? html`<div class='association-group'>
		<h3>implements</h3>
		${associationList(implementations, 'executable')}
	</div>` : ''}
</section>`
		: '';
	return renderPage({
		title: `project: ${project}`,
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'index',
		hasHeaderSearch: true,
		scripts: html`<script type='module' src='${context.applicationAssets}/exports.js'></script>`,
		liveReload: context.liveReload,
		content: html`<section class='page-heading detail-heading'>
	<a class='back-link' href='/?q='>← browse names</a>
	<p class='eyebrow'>project</p>
	<h1><code>${project}</code></h1>
	${projectLinks}
</section>
${associationSection}
<section aria-labelledby='providers-title'>
	<div class='section-heading export-heading'>
		<h2 id='providers-title'>executables</h2>
		${exportActions(`/projects/${encodeSegment(project)}/providers.json`)}
	</div>
	${projectCoverage(project, coverage)}
</section>
${coverageNote()}`,
	});
}

export function executablePage(database: WebsiteDatabase, executable: string, context: PageContext): string {
	const providers = database.executableProviders(executable);
	const projects = groupExecutableProjects(providers);
	const associations = database.executableAssociations(executable);
	const references = associations
		.filter((association) => association.role === 'reference')
		.map((association) => ({ name: association.project, role: association.role }));
	const implementations = associations
		.filter((association) => association.role === 'implementation' &&
			!projects.some((project) => project.project === association.project))
		.map((association) => ({ name: association.project, role: association.role }));
	const projectCount = new Set([
		...projects.map((project) => project.project),
		...associations.map((association) => association.project),
	]).size;
	const referenceSection = references.length
		? html`<div class='reference-note'>
	<span class='eyebrow'>reference</span>
	${associationList(references, 'project')}
</div>`
		: '';
	const providerSection = projects.length
		? html`<div class='provider-groups'>
	${projects.map((project) => providerGroup(project.project, 'project', project.channels))}
</div>`
		: html`<p class='detail-note'>No current channel packages are curated as providers for this name.</p>`;
	const implementationSection = implementations.length
		? html`<section class='association-section' aria-labelledby='implementations-title'>
	<h2 id='implementations-title'>known implementations</h2>
	<p class='fine-print'>These projects are associated with the name but are not listed as providing it
		as an installed command.</p>
	${associationList(implementations, 'project')}
</section>`
		: '';
	return renderPage({
		title: `executable: ${executable}`,
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'index',
		hasHeaderSearch: true,
		scripts: html`<script type='module' src='${context.applicationAssets}/exports.js'></script>`,
		liveReload: context.liveReload,
		content: html`<section class='page-heading detail-heading'>
	<a class='back-link' href='/?q='>← browse names</a>
	<p class='eyebrow'>executable</p>
	<h1><span class='command-prefix' aria-hidden='true'>$ </span><code>${executable}</code></h1>
	<p class='lead'>${countLabel(projectCount, 'project', 'projects')}.</p>
</section>
${referenceSection}
<section aria-labelledby='providers-title'>
	${projects.length > 1 ? html`<div class='ambiguity-note'>
		<strong>Multiple projects provide this name.</strong>
		Check the project before choosing a package.
	</div>` : ''}
	<div class='section-heading export-heading'>
		<h2 id='providers-title'>projects</h2>
		${exportActions(`/executables/${encodeSegment(executable)}/providers.json`)}
	</div>
	${providerSection}
</section>
${implementationSection}
<p class='detail-note'>Links open current package pages. Check versions and platform support there.</p>
${coverageNote()}`,
	});
}

function shortNavLink(section: ShortNameSection, active: ShortNameSection): Html {
	const label = section.prefix === '' ? '2 letters' : section.prefix;
	const current = section === active ? raw("aria-current='page'") : '';
	return html`<a href='/short/${shortSectionSlug(section)}' ${current}>${label}</a>`;
}

function renderShortSection(section: ShortNameSection, label: string, description: string): Html {
	const id = section.prefix === '' ? 'short-2' : `short-${section.prefix}`;
	return html`<section class='short-section' id='${id}' aria-labelledby='${id}-title'>
	<header class='short-section-heading'>
		<h2 id='${id}-title'>${label}</h2>
		<p class='short-section-count'>${formatNumber(section.used)} of ${formatNumber(section.cells.length)} seen</p>
	</header>
	<div class='short-grid-scroll' role='region' aria-label='${description}' tabindex='0'>
		<div class='short-grid'>
			${section.cells.map((cell) => cell.projects > 0
				? html`<a class='short-name seen' href='/executables/${encodeSegment(cell.name)}'
					aria-label='${cell.name}: ${countLabel(cell.projects, 'project')}'>${cell.name}</a>`
				: html`<span class='short-name'>${cell.name}</span>`)}
		</div>
	</div>
</section>`;
}

export function notFoundPage(context: PageContext): string {
	return renderPage({
		title: 'page not found',
		applicationAssets: context.applicationAssets,
		clientSearch: false,
		release: context.release,
		section: 'index',
		hasHeaderSearch: false,
		liveReload: context.liveReload,
		content: html`<section class='error-page'>
	<p class='eyebrow'>error 404</p>
	<h1>page not found</h1>
	<p class='lead'>Check the address or use the search below.</p>
	${searchForm()}
	<p class='search-hint' id='search-hint'>Search by full or partial name.
		<a href='/?q='>browse names →</a></p>
</section>`,
	});
}
