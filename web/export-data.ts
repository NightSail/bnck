import type { AssociationRole, ExportQuery, Provider } from './types';
import type { WebsiteDatabase } from './database';

interface AssociationExport {
	executable?: string;
	project?: string;
	role: AssociationRole;
}

interface ProviderExport {
	dataset_release: string;
	query: ExportQuery;
	providers: Provider[];
	associations: AssociationExport[];
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
	const associations: AssociationExport[] = 'project' in query
		? database.projectAssociations(query.project).map(({ executable, role }) => ({ executable, role }))
		: database.executableAssociations(query.executable).map(({ project, role }) => ({ project, role }));
	const payload: ProviderExport = {
		dataset_release: `bnck:release:${database.metadataValue('dataset_release')}`,
		query,
		providers: exportProviders(database, query),
		associations,
	};
	return new TextEncoder().encode(`${JSON.stringify(payload, null, '\t')}\n`);
}
