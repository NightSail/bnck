import {
	associationList,
	countLabel,
	coverageNote,
	exportActions,
	projectCoverage,
	providerGroup,
} from './components';
import { buildProjectCoverage, groupExecutableProjects } from './data';
import { html, type Html } from './html';
import { compareNames } from './naming';
import type {
	AssociationRole,
	BrowserDataset,
	BrowserProject,
	ExecutableAssociation,
	ExecutableProvider,
	ProviderExport,
} from './types';

export type IdentityRoute = { kind: 'project'; name: string } | { kind: 'executable'; name: string };

export function parseIdentityPath(pathname: string): IdentityRoute | null {
	const normalized = pathname.length > 1 && pathname.endsWith('/') ? pathname.slice(0, -1) : pathname;
	const segments = normalized.split('/');
	if (segments.length !== 3 || segments[0] !== '' || !segments[2]) return null;
	const kind = segments[1] === 'projects' ? 'project' : segments[1] === 'executables' ? 'executable' : null;
	if (!kind) return null;
	try {
		return { kind, name: decodeURIComponent(segments[2]) };
	} catch {
		return null;
	}
}

export function findProject(dataset: BrowserDataset, name: string): BrowserProject | undefined {
	return dataset.projects.find((project) => project.name === name);
}

export function executableProviders(dataset: BrowserDataset, executable: string): ExecutableProvider[] {
	const providers: ExecutableProvider[] = [];
	for (const project of dataset.projects) {
		for (const provider of project.providers) {
			if (provider.executable === executable) providers.push({ project: project.name, ...provider });
		}
	}
	return providers.sort((left, right) => compareNames(left.project, right.project) ||
		compareNames(left.channel, right.channel) || compareNames(left.package, right.package));
}

export function executableAssociations(dataset: BrowserDataset, executable: string): ExecutableAssociation[] {
	const associations: ExecutableAssociation[] = [];
	for (const project of dataset.projects) {
		for (const association of project.associations) {
			if (association.executable === executable) {
				associations.push({ project: project.name, role: association.role });
			}
		}
	}
	return associations.sort((left, right) => roleRank(left.role) - roleRank(right.role) ||
		compareNames(left.project, right.project));
}

function roleRank(role: AssociationRole): number {
	return role === 'reference' ? 0 : 1;
}

export function projectContent(project: BrowserProject): Html {
	const coverage = buildProjectCoverage(project.providers);
	const references = project.associations
		.filter((association) => association.role === 'reference')
		.map((association) => ({ name: association.executable, role: association.role }));
	const implementations = project.associations
		.filter((association) => association.role === 'implementation')
		.map((association) => ({ name: association.executable, role: association.role }));
	const projectLinks = project.links.length
		? html`<ul class='project-links'>
			${project.links.map((link) => html`<li>
				${link.link_type}:
				<a href='${link.url}' rel='external'>${link.url}</a>
			</li>`)}
		</ul>`
		: html`<p class='project-links'>No project links listed.</p>`;
	const associationSection = project.associations.length
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
	return html`<section class='page-heading detail-heading'>
	<a class='back-link' href='/?q='>← browse names</a>
	<p class='eyebrow'>project</p>
	<h1><code>${project.name}</code></h1>
	${projectLinks}
</section>
${associationSection}
<section aria-labelledby='providers-title'>
	<div class='section-heading export-heading'>
		<h2 id='providers-title'>executables</h2>
		${exportActions()}
	</div>
	${projectCoverage(project.name, coverage)}
</section>
${coverageNote()}`;
}

export function executableContent(executable: string, providers: ExecutableProvider[],
	associations: ExecutableAssociation[]): Html {
	const projects = groupExecutableProjects(providers);
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
	return html`<section class='page-heading detail-heading'>
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
		${exportActions()}
	</div>
	${providerSection}
</section>
${implementationSection}
<p class='detail-note'>Links open current package pages. Check versions and platform support there.</p>
${coverageNote()}`;
}

export function projectExportPayload(project: BrowserProject, release: string): ProviderExport {
	return {
		dataset_release: `bnck:release:${release}`,
		query: { project: project.name },
		providers: project.providers.map((provider) => ({
			project: project.name,
			executable: provider.executable,
			channel: provider.channel,
			package: provider.package,
		})),
		associations: project.associations.map(({ executable, role }) => ({ executable, role })),
	};
}

export function executableExportPayload(executable: string, providers: ExecutableProvider[],
	associations: ExecutableAssociation[], release: string): ProviderExport {
	return {
		dataset_release: `bnck:release:${release}`,
		query: { executable },
		providers: providers.map((provider) => ({
			project: provider.project,
			executable,
			channel: provider.channel,
			package: provider.package,
		})),
		associations: associations.map(({ project, role }) => ({ project, role })),
	};
}
