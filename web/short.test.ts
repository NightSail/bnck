import { expect, test } from 'bun:test';

import { buildShortNameSections } from './data';
import { WebsiteDatabase } from './database';
import { shortPage } from './pages';
import { createFixture, provider } from './test-fixture';

const context = {
	applicationAssets: '/assets/app.test',
	datasetUrl: '/assets/dataset.test.json',
	release: '2026-04-16.1',
	liveReload: false,
};

test('short name sections enumerate every two- and three-letter name', () => {
	const sections = buildShortNameSections(new Map([['aa', 2], ['zz', 1], ['aba', 3]]));
	const names = sections.flatMap((section) => section.cells.map((cell) => cell.name));
	expect(sections).toHaveLength(27);
	expect(names).toHaveLength(18_252);
	expect(new Set(names).size).toBe(names.length);
	expect(sections[0]).toMatchObject({ prefix: '', used: 2 });
	expect(sections[0].cells.slice(0, 2).map((cell) => cell.name)).toEqual(['aa', 'ab']);
	expect(sections[0].cells.at(-1)?.name).toBe('zz');
	expect(sections[1].prefix).toBe('a');
	expect(sections[1].cells[0]).toEqual({ name: 'aaa', projects: 0 });
	expect(sections[1].cells.at(-1)?.name).toBe('azz');
	expect(sections[1].used).toBe(1);
	expect(sections[26].prefix).toBe('z');
	expect(sections[26].cells.at(-1)?.name).toBe('zzz');
});

test('short name map highlights used names and leaves the rest plain', async () => {
	const fixture = await createFixture([
		provider('first', 'aa', 'debian'),
		provider('second', 'aa', 'npm'),
		provider('first', 'zz', 'debian'),
		provider('third', 'aba', 'crates.io'),
		provider('third', 'HX', 'npm'),
		provider('third', 'ug+', 'npm'),
		provider('third', 'tool', 'npm'),
	]);
	const database = await WebsiteDatabase.open(fixture.path);
	try {
		const page = shortPage(database, context);
		expect(page.match(/class='short-section'/g)).toHaveLength(27);
		expect(page.match(/class='short-name seen'/g)).toHaveLength(3);
		expect(page).toContain("id='short-2'");
		expect(page).toContain("id='short-a'");
		expect(page).toContain("href='/executables/aa'");
		expect(page).toContain("aria-label='aa: 2 projects'");
		expect(page).toContain("href='/executables/zz'");
		expect(page).toContain("href='/executables/aba'");
		expect(page).toContain("<span class='short-name'>ab</span>");
		expect(page).toContain('3 names used of 18,252.');
		expect(page).toContain('2 of 676 used');
		expect(page).toContain('1 of 676 used');
		expect(page).not.toContain('HX');
		expect(page).not.toContain('ug+');
	} finally {
		database.close();
		await fixture.cleanup();
	}
});
