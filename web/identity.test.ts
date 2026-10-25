import { expect, test } from 'bun:test';

import { WebsiteDatabase } from './database';
import { browserDataset } from './data';
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
import { createFixture, provider, type Fixture } from './test-fixture';
import type { BrowserDataset } from './types';

const MATRIX_RECORDS = [
	provider('tools', 'tool', 'debian', 'tools'),
	provider('tools', 'helper', 'debian', 'tools'),
	provider('tools', 'tool', 'debian', 'tools-extra'),
	provider('tools', 'helper', 'arch', 'tools'),
	provider('tools', 'Tool', 'arch', 'tools'),
	provider('tools', 'tool', 'npm', '@scope/tools'),
	provider('tools', 'helper', 'homebrew', 'custom/tap/tools'),
];

async function openDataset(providers: Parameters<typeof createFixture>[0],
	options: Parameters<typeof createFixture>[2] = {}): Promise<{ database: WebsiteDatabase; dataset: BrowserDataset;
	fixture: Fixture }> {
	const fixture = await createFixture(providers, [], options);
	const database = await WebsiteDatabase.open(fixture.path);
	return { database, dataset: browserDataset(database, database.listProjects()), fixture };
}

function findProjectOrFail(dataset: BrowserDataset, name: string) {
	const project = findProject(dataset, name);
	if (!project) throw new Error(`missing fixture project ${name}`);
	return project;
}

test('identity paths preserve exact case and decode percent-encoded segments once', () => {
	expect(parseIdentityPath('/projects/helix')).toEqual({ kind: 'project', name: 'helix' });
	expect(parseIdentityPath('/executables/HX')).toEqual({ kind: 'executable', name: 'HX' });
	expect(parseIdentityPath('/executables/%C3%A9%25%26%27')).toEqual({
		kind: 'executable', name: "é%&'",
	});
	expect(parseIdentityPath('/projects/helix/')).toEqual({ kind: 'project', name: 'helix' });
	expect(parseIdentityPath('/projects/a/b')).toBeNull();
	expect(parseIdentityPath('/projects/%zz')).toBeNull();
	expect(parseIdentityPath('/about')).toBeNull();
});

test('project matrices group channels, sort packages, and mark exact coverage', async () => {
	const { database, dataset, fixture } = await openDataset(MATRIX_RECORDS);
	try {
		const page = projectContent(findProjectOrFail(dataset, 'tools')).value;
		expect(page.match(/<table /g)).toHaveLength(1);
		expect(page.match(/scope='col'>package<\/th>/g)).toHaveLength(1);
		expect(page.match(/class='matrix-executable' scope='col'/g)).toHaveLength(3);
		expect(page.match(/class='matrix-name' scope='row'/g)).toHaveLength(5);
		expect(page.match(/scope='rowgroup'/g)).toHaveLength(4);
		expect(page).toContain("colspan='4'");
		expect(page).toContain("<span class='matrix-channel-label'>Arch Linux</span>");
		expect(page).toContain("<span class='matrix-channel-label'>Debian</span>");
		expect(page).toContain("href='https://packages.debian.org/tools-extra'");
		expect(page).toContain("href='https://www.npmjs.com/package/%40scope/tools'");
		expect(page).toContain("href='/executables/Tool'");
		expect(page).toContain("href='/executables/helper'");
		expect(page).toContain("href='/executables/tool'");
		expect(page.match(/class='visually-hidden'>observed<\/span>/g)).toHaveLength(7);
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('large project coverage lists packages instead of a dense matrix', async () => {
	const records = Array.from({ length: 101 }, (_, packageIndex) =>
		Array.from({ length: 101 }, (_, executableIndex) =>
			provider('large', `cmd-${executableIndex}`, 'debian', `pkg-${packageIndex}`))).flat();
	const { database, dataset, fixture } = await openDataset(records);
	try {
		const page = projectContent(findProjectOrFail(dataset, 'large')).value;
		expect(page).not.toContain("class='matrix-executable'");
		expect(page).not.toContain("class='matrix-scroll'");
		expect(page).toContain("class='coverage-note'");
		expect(page).toContain("class='provider-coverage'");
		expect(page).toContain("id='executable-cmd-0'");
		expect(page).toContain("href='/executables/cmd-100'");
		expect(page.match(/class='coverage-package'/g)).toHaveLength(101);
		expect(page.match(/class='coverage-executables'/g)).toHaveLength(101);
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('project and executable views escape names and encode path segments', async () => {
	const executable = "<tool>&'";
	const { database, dataset, fixture } = await openDataset([
		provider('tools', executable, 'npm', "@scope/<package>&'"),
	]);
	try {
		const project = projectContent(findProjectOrFail(dataset, 'tools')).value;
		expect(project).toContain("id='executable-&lt;tool&gt;&amp;&#39;'");
		expect(project).toContain("href='/executables/%3Ctool%3E%26%27'");
		expect(project).toContain('<code>&lt;tool&gt;&amp;&#39;</code>');
		expect(project).toContain('<code>@scope/&lt;package&gt;&amp;&#39;</code>');
		expect(project).toContain('%40scope/%3Cpackage%3E%26%27');
		expect(project).not.toContain('<tool>');
		expect(project).not.toContain('<package>');

		const providers = executableProviders(dataset, executable);
		const page = executableContent(executable, providers, executableAssociations(dataset, executable)).value;
		expect(page).toContain('<code>&lt;tool&gt;&amp;&#39;</code>');
		expect(page).toContain("href='/projects/tools'");
		expect(page).not.toContain('<tool>');
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('ambiguous executables list every owning project', async () => {
	const { database, dataset, fixture } = await openDataset([
		provider('first', 'tool', 'debian'),
		provider('second', 'tool', 'npm'),
	]);
	try {
		const providers = executableProviders(dataset, 'tool');
		const page = executableContent('tool', providers, executableAssociations(dataset, 'tool')).value;
		expect(page).toContain('<strong>Multiple projects provide this name.</strong>');
		expect(page).toContain("id='project-first'");
		expect(page).toContain("id='project-second'");
		expect(page.match(/class='provider-group'/g)).toHaveLength(2);
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('executable and project views show name associations separately from packages', async () => {
	const { database, dataset, fixture } = await openDataset(
		[provider('coreutils', 'ls', 'debian', 'coreutils')],
		{
			associations: [
				{ project: 'coreutils', executable: 'ls', role: 'reference' },
				{ project: 'uutils-coreutils', executable: 'ls', role: 'implementation' },
			],
		},
	);
	try {
		const executable = executableContent('ls', executableProviders(dataset, 'ls'),
			executableAssociations(dataset, 'ls')).value;
		expect(executable).toContain("class='reference-note'");
		expect(executable).toContain("href='/projects/coreutils'");
		expect(executable).toContain("href='/projects/uutils-coreutils'");
		expect(executable).toContain("<h2 id='implementations-title'>known implementations</h2>");
		expect(executable.match(/class='provider-group'/g)).toHaveLength(1);

		const project = projectContent(findProjectOrFail(dataset, 'coreutils')).value;
		expect(project).toContain("<h2 id='associations-title'>name associations</h2>");
		expect(project).toContain('<h3>reference</h3>');
		expect(project).toContain("href='/executables/ls'");

		const implementation = projectContent(findProjectOrFail(dataset, 'uutils-coreutils')).value;
		expect(implementation).toContain('No current channel packages are curated for this project.');
		expect(implementation).toContain('<h3>implements</h3>');
		expect(implementation).toContain("href='/executables/ls'");
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('browser exports preserve the release prefix, query, order, and associations', async () => {
	const { database, dataset, fixture } = await openDataset(
		[
			provider('helix', 'hx', 'debian', 'helix'),
			provider('helix', 'hx', 'crates.io', 'helix-term'),
			provider('other', 'hx', 'npm'),
		],
		{ associations: [{ project: 'helix', executable: 'cat', role: 'implementation' }] },
	);
	try {
		const project = projectExportPayload(findProjectOrFail(dataset, 'helix'), dataset.dataset_release);
		expect(project).toEqual({
			dataset_release: 'bnck:release:2026-04-16.1',
			query: { project: 'helix' },
			providers: [
				{ project: 'helix', executable: 'hx', channel: 'crates.io', package: 'helix-term' },
				{ project: 'helix', executable: 'hx', channel: 'debian', package: 'helix' },
			],
			associations: [{ executable: 'cat', role: 'implementation' }],
		});
		const executable = executableExportPayload('hx', executableProviders(dataset, 'hx'),
			executableAssociations(dataset, 'hx'), dataset.dataset_release);
		expect(executable).toEqual({
			dataset_release: 'bnck:release:2026-04-16.1',
			query: { executable: 'hx' },
			providers: [
				{ project: 'helix', executable: 'hx', channel: 'crates.io', package: 'helix-term' },
				{ project: 'helix', executable: 'hx', channel: 'debian', package: 'helix' },
				{ project: 'other', executable: 'hx', channel: 'npm', package: 'other-package' },
			],
			associations: [],
		});
	} finally {
		database.close();
		await fixture.cleanup();
	}
});
