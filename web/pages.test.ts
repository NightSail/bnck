import { expect, test } from 'bun:test';

import { WebsiteDatabase } from './database';
import { executablePage, projectPage } from './pages';
import { createFixture, provider } from './test-fixture';

const context = {
	applicationAssets: '/assets/app.test',
	datasetUrl: '/assets/dataset.test.json',
	release: '2026-04-16.1',
	liveReload: false,
};

const MATRIX_RECORDS = [
	provider('tools', 'tool', 'debian', 'tools'),
	provider('tools', 'helper', 'debian', 'tools'),
	provider('tools', 'tool', 'debian', 'tools-extra'),
	provider('tools', 'helper', 'arch', 'tools'),
	provider('tools', 'Tool', 'arch', 'tools'),
	provider('tools', 'tool', 'npm', '@scope/tools'),
	provider('tools', 'helper', 'homebrew', 'custom/tap/tools'),
];

test('project matrices group channels, sort packages, and mark exact coverage', async () => {
	const fixture = await createFixture(MATRIX_RECORDS);
	const database = await WebsiteDatabase.open(fixture.path);
	try {
		const page = projectPage(database, 'tools', context);
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

test('project and executable pages escape names and encode path segments', async () => {
	const executable = "<tool>&'";
	const fixture = await createFixture([provider('tools', executable, 'npm', "@scope/<package>&'")]);
	const database = await WebsiteDatabase.open(fixture.path);
	try {
		const project = projectPage(database, 'tools', context);
		expect(project).toContain("id='executable-&lt;tool&gt;&amp;&#39;'");
		expect(project).toContain("href='/executables/%3Ctool%3E%26%27'");
		expect(project).toContain('<code>&lt;tool&gt;&amp;&#39;</code>');
		expect(project).toContain('<code>@scope/&lt;package&gt;&amp;&#39;</code>');
		expect(project).toContain('%40scope/%3Cpackage%3E%26%27');
		expect(project).not.toContain('<tool>');
		expect(project).not.toContain('<package>');

		const page = executablePage(database, executable, context);
		expect(page).toContain('<code>&lt;tool&gt;&amp;&#39;</code>');
		expect(page).toContain("href='/projects/tools'");
		expect(page).not.toContain('<tool>');
	} finally {
		database.close();
		await fixture.cleanup();
	}
});

test('ambiguous executables list every owning project', async () => {
	const fixture = await createFixture([
		provider('first', 'tool', 'debian'),
		provider('second', 'tool', 'npm'),
	]);
	const database = await WebsiteDatabase.open(fixture.path);
	try {
		const page = executablePage(database, 'tool', context);
		expect(page).toContain('<strong>Multiple projects provide this name.</strong>');
		expect(page).toContain("id='project-first'");
		expect(page).toContain("id='project-second'");
		expect(page.match(/class='provider-group'/g)).toHaveLength(2);
	} finally {
		database.close();
		await fixture.cleanup();
	}
});
