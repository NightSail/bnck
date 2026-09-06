import { buildIndex } from './query.js';

function validateDataset(dataset, release) {
	if (dataset?.schema_version !== 1 || dataset.dataset_release !== release ||
		!Array.isArray(dataset.channels) || !Array.isArray(dataset.projects) ||
		!dataset.channels.every((channel) => typeof channel?.name === 'string' && typeof channel.label === 'string')) {
		throw new Error('The search dataset is incompatible with this page. Reload the page and try again.');
	}
	const channels = new Set(dataset.channels.map((channel) => channel.name));
	for (const project of dataset.projects) {
		if (typeof project?.name !== 'string' || !project.name || !Array.isArray(project.providers) ||
			!project.providers.every((provider) => typeof provider?.executable === 'string' &&
				provider.executable && channels.has(provider.channel) && typeof provider.package === 'string')) {
			throw new Error('The search dataset contains invalid records. Reload the page and try again.');
		}
	}
	return dataset;
}

export function createDatasetLoader(url, release, fetchDataset = globalThis.fetch) {
	let pending;
	return function loadDataset() {
		if (!pending) {
			/* Reuse one fetch/index per document; immutable HTTP caching handles subsequent visits. */
			pending = Promise.resolve().then(() => fetchDataset(url, { signal: AbortSignal.timeout(10000) }))
				.then(async (response) => {
					if (!response.ok) throw new Error(`The search dataset could not be loaded (HTTP ${response.status}).`);
					const dataset = validateDataset(await response.json(), release);
					return { dataset, index: buildIndex(dataset) };
				}).catch((error) => {
					pending = undefined;
					throw error;
				});
		}
		return pending;
	};
}
