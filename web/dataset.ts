import { BROWSER_DATASET_SCHEMA_VERSION } from './config';
import { buildIndex, type IndexedProject } from './query';
import type { BrowserDataset } from './types';

export interface LoadedDataset {
	dataset: BrowserDataset;
	index: IndexedProject[];
}

export type DatasetLoader = () => Promise<LoadedDataset>;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function validateDataset(dataset: unknown, release: string): BrowserDataset {
	if (!isRecord(dataset) || dataset.schema_version !== BROWSER_DATASET_SCHEMA_VERSION ||
		dataset.dataset_release !== release ||
		!Array.isArray(dataset.channels) || !Array.isArray(dataset.projects) ||
		!dataset.channels.every((channel) => isRecord(channel) && typeof channel.name === 'string' &&
			typeof channel.label === 'string')) {
		throw new Error('The site dataset is incompatible with this page. Reload the page and try again.');
	}
	const channels = new Set(dataset.channels.map((channel) => (channel as { name: string }).name));
	for (const project of dataset.projects) {
		if (!isRecord(project) || typeof project.name !== 'string' || !project.name ||
			!Array.isArray(project.providers) ||
			!project.providers.every((provider) => isRecord(provider) && typeof provider.executable === 'string' &&
				provider.executable && channels.has(provider.channel as string) && typeof provider.package === 'string') ||
			!Array.isArray(project.associations) ||
			!project.associations.every((association) => isRecord(association) &&
				typeof association.executable === 'string' && association.executable &&
				(association.role === 'reference' || association.role === 'implementation'))) {
			throw new Error('The site dataset contains invalid records. Reload the page and try again.');
		}
	}
	return dataset as unknown as BrowserDataset;
}

export function createDatasetLoader(
	url: string,
	release: string,
	fetchDataset: typeof fetch = globalThis.fetch,
): DatasetLoader {
	let pending: Promise<LoadedDataset> | undefined;
	return function loadDataset(): Promise<LoadedDataset> {
		if (!pending) {
			/* Reuse one fetch/index per document; immutable HTTP caching handles subsequent visits. */
			pending = Promise.resolve().then(() => fetchDataset(url, { signal: AbortSignal.timeout(10000) }))
				.then(async (response) => {
					if (!response.ok) throw new Error(`The site dataset could not be loaded (HTTP ${response.status}).`);
					const dataset = validateDataset(await response.json(), release);
					return { dataset, index: buildIndex(dataset) };
				}).catch((error: unknown) => {
					pending = undefined;
					throw error;
				});
		}
		return pending;
	};
}
