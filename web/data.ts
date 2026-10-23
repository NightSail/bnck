import { BROWSER_DATASET_SCHEMA_VERSION, CHANNELS, channelLabel } from './config';
import { canonicalPackageUrl, compareNames } from './naming';
import type {
	BrowserDataset,
	BrowserProject,
	ChannelGroup,
	ExecutableProvider,
	PackageReference,
	ProjectGroup,
	ProjectMatrix,
	ProjectProvider,
} from './types';
import type { WebsiteDatabase } from './database';

export function buildProjectMatrix(providers: ProjectProvider[]): ProjectMatrix {
	const packages = new Map<string, Set<string>>();
	for (const provider of providers) {
		const key = JSON.stringify([provider.channel, provider.package]);
		const executables = packages.get(key) ?? new Set<string>();
		executables.add(provider.executable);
		packages.set(key, executables);
	}

	/* Executable columns and package rows share one exact ordering. */
	const executables = [...new Set([...packages.values()].flatMap((set) => [...set]))].sort(compareNames);
	const channels = new Map<string, ProjectMatrix['channels'][number]['packages']>();
	for (const [key, observedExecutables] of packages) {
		const [channel, packageName] = JSON.parse(key) as [string, string];
		const packagesInChannel = channels.get(channel) ?? [];
		packagesInChannel.push({
			reference: { url: canonicalPackageUrl(channel, packageName), name: packageName },
			observed: executables.map((executable) => observedExecutables.has(executable)),
		});
		channels.set(channel, packagesInChannel);
	}

	return {
		channels: [...channels.entries()].sort(([left], [right]) => compareNames(left, right))
			.map(([channel, packages]) => ({
				channel,
				packages: packages.sort((left, right) => compareNames(left.reference.name, right.reference.name)),
			})),
		executables,
	};
}

export function groupExecutableProjects(providers: ExecutableProvider[]): ProjectGroup[] {
	const projects = new Map<string, [string, string][]>();
	for (const provider of providers) {
		const grouped = projects.get(provider.project) ?? [];
		grouped.push([provider.channel, provider.package]);
		projects.set(provider.project, grouped);
	}
	return [...projects.entries()].sort(([left], [right]) => compareNames(left, right))
		.map(([project, grouped]) => ({ project, channels: groupChannels(grouped) }));
}

function groupChannels(providers: [string, string][]): ChannelGroup[] {
	const channels = new Map<string, string[]>();
	for (const [channel, packageName] of providers) {
		const packages = channels.get(channel) ?? [];
		packages.push(packageName);
		channels.set(channel, packages);
	}
	return [...channels.entries()].sort(([left], [right]) => compareNames(left, right)).map(([channel, packages]) => ({
		channel,
		packages: packages.sort(compareNames).map((name): PackageReference => ({
			name,
			url: canonicalPackageUrl(channel, name),
		})),
	}));
}

export function browserDataset(database: WebsiteDatabase, projects: string[]): BrowserDataset {
	const browserProjects: BrowserProject[] = projects.map((name) => ({
		name,
		links: database.projectLinks(name),
		providers: database.projectProviders(name),
	}));
	return {
		schema_version: BROWSER_DATASET_SCHEMA_VERSION,
		dataset_release: database.metadataValue('dataset_release'),
		generated_at: database.metadataValue('generated_at'),
		channels: CHANNELS.map((name) => ({ name, label: channelLabel(name) })),
		projects: browserProjects,
	};
}
