import { expect, mock, test } from 'bun:test';
import dataset from './fixtures/dataset.json';
import { createDatasetLoader } from './dataset.js';

const DATASET_URL = '/assets/dataset.hash.json';
const success = () => Response.json(dataset);

test('concurrent and subsequent lookups share one fetch and one index', async () => {
	const fetch = mock(async () => success());
	const load = createDatasetLoader(DATASET_URL, 'test.1', fetch);
	const first = load();
	expect(load()).toBe(first);
	const loaded = await first;
	expect(await load()).toBe(loaded);
	expect(fetch).toHaveBeenCalledTimes(1);
	expect(fetch.mock.calls[0][0]).toBe(DATASET_URL);
	expect(fetch.mock.calls[0][1].signal).toBeInstanceOf(AbortSignal);
	expect(loaded.index).toHaveLength(3);
});

test('network, HTTP, parsing, and schema failures are explicit and retryable', async () => {
	for (const fail of [
		() => { throw new Error('Network unavailable'); },
		() => new Response('Not found', { status: 404 }),
		() => new Response('{'),
		() => Response.json({ ...dataset, schema_version: 2 }),
		() => Response.json({ ...dataset, dataset_release: 'old' }),
		() => Response.json({ ...dataset, projects: [{ name: 'bad', providers: [{}] }] }),
	]) {
		let attempts = 0;
		const fetch = mock(async () => ++attempts === 1 ? fail() : success());
		const load = createDatasetLoader('/dataset.json', 'test.1', fetch);
		await expect(load()).rejects.toThrow();
		expect((await load()).dataset.dataset_release).toBe('test.1');
		expect(fetch).toHaveBeenCalledTimes(2);
	}
});
