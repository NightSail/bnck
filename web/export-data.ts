import type { ExportQuery, Provider } from './types';
import type { WebsiteDatabase } from './database';

interface ProviderExport {
	dataset_release: string;
	query: ExportQuery;
	providers: Provider[];
}

export function exportProviders(database: WebsiteDatabase, query: ExportQuery): Provider[] {
	if ('project' in query) {
		return database.projectProviders(query.project).map((provider) => ({
			project: query.project,
			executable: provider.executable,
			channel: provider.channel,
			package: provider.package,
		}));
	}
	return database.executableProviders(query.executable).map((provider) => ({
		project: provider.project,
		executable: query.executable,
		channel: provider.channel,
		package: provider.package,
	}));
}

export function exportBytes(database: WebsiteDatabase, query: ExportQuery): Uint8Array {
	const payload: ProviderExport = {
		dataset_release: `bnck:release:${database.metadataValue('dataset_release')}`,
		query,
		providers: exportProviders(database, query),
	};
	return new TextEncoder().encode(`${JSON.stringify(payload, null, '\t')}\n`);
}
