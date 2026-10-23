import { expect, test } from 'bun:test';

import { buildShortNameSections, shortSectionSlug } from './data';
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
	expect(sections.map(shortSectionSlug)).toEqual(['2', ...'abcdefghijklmnopqrstuvwxyz']);
});

test('short name pages highlight seen names and leave the rest plain', async () => {
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
		const sections = buildShortNameSections(database.shortNameUsage());
		const twoLetter = shortPage(sections[0], sections, context);
		expect(twoLetter.match(/class='short-section'/g)).toHaveLength(1);
		expect(twoLetter.match(/class='short-name seen'/g)).toHaveLength(2);
		expect(twoLetter).toContain("id='short-2'");
		expect(twoLetter).not.toContain("id='short-a'");
		expect(twoLetter).toContain("href='/short/2' aria-current='page'");
		expect(twoLetter).toContain("href='/short/a'");
		expect(twoLetter).toContain("href='/executables/aa'");
		expect(twoLetter).toContain("aria-label='aa: 2 projects'");
		expect(twoLetter).toContain("href='/executables/zz'");
		expect(twoLetter).toContain("<span class='short-name'>ab</span>");
		expect(twoLetter).toContain('<title>short names: 2 letters · bnck(1)</title>');
		expect(twoLetter).toContain('3 names seen of 18,252.');
		expect(twoLetter).toContain('2 of 676 seen');
		expect(twoLetter).not.toContain("href='/executables/aba'");

		const aSection = shortPage(sections[1], sections, context);
		expect(aSection.match(/class='short-section'/g)).toHaveLength(1);
		expect(aSection.match(/class='short-name seen'/g)).toHaveLength(1);
		expect(aSection).toContain("id='short-a'");
		expect(aSection).not.toContain("id='short-2'");
		expect(aSection).toContain("href='/short/a' aria-current='page'");
		expect(aSection).toContain("href='/executables/aba'");
		expect(aSection).not.toContain("href='/executables/aa'");
		expect(aSection).toContain('<title>short names: a · bnck(1)</title>');
		expect(aSection).toContain('1 of 676 seen');
		expect(aSection + twoLetter).not.toContain('HX');
		expect(aSection + twoLetter).not.toContain('ug+');
	} finally {
		database.close();
		await fixture.cleanup();
	}
});
