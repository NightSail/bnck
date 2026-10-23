export interface Provider {
	project: string;
	executable: string;
	channel: string;
	package: string;
}

export type ExportQuery = { project: string } | { executable: string };

export interface ProjectProvider {
	executable: string;
	channel: string;
	package: string;
}

export interface ExecutableProvider {
	project: string;
	channel: string;
	package: string;
}

export interface ProjectLink {
	link_type: string;
	url: string;
}

export type AssociationRole = 'reference' | 'implementation';

export interface ProjectAssociation {
	executable: string;
	role: AssociationRole;
}

export interface ExecutableAssociation {
	project: string;
	role: AssociationRole;
}

export interface DatasetStats {
	providers: number;
	projects: number;
	executables: number;
	channels: number;
}

export interface ChannelStats {
	name: string;
	projects: number;
	packages: number;
}

export interface SearchQuery {
	q?: string | null;
	channel: string;
	initial: string;
	page: number;
}

export interface SearchRelation {
	name: string;
	channels: string[];
	role?: AssociationRole;
}

export interface SearchResult {
	name: string;
	related: SearchRelation[];
}

export interface SearchFilterLink {
	label: string;
	url: string;
	is_current: boolean;
}

export interface SearchPageLink {
	label: string;
	url: string | null;
	is_current: boolean;
}

export interface SearchResults {
	rows: SearchResult[];
	initial_links: SearchFilterLink[];
	total: number;
	start: number;
	end: number;
	page: number;
	pages: number;
	page_links: SearchPageLink[];
	previous: string | null;
	next: string | null;
}

export interface PackageReference {
	name: string;
	url: string | null;
}

export interface ChannelGroup {
	channel: string;
	packages: PackageReference[];
}

export interface ProjectMatrix {
	channels: MatrixChannel[];
	executables: string[];
}

export interface MatrixChannel {
	channel: string;
	packages: MatrixPackage[];
}

export interface MatrixPackage {
	reference: PackageReference;
	observed: boolean[];
}

export interface ProjectGroup {
	project: string;
	channels: ChannelGroup[];
}

export interface ShortNameCell {
	name: string;
	projects: number;
}

export interface ShortNameSection {
	prefix: string;
	cells: ShortNameCell[];
	used: number;
}

export interface BrowserChannel {
	name: string;
	label: string;
}

export interface BrowserProject {
	name: string;
	links: ProjectLink[];
	providers: ProjectProvider[];
	associations: ProjectAssociation[];
}

export interface BrowserDataset {
	schema_version: number;
	dataset_release: string;
	generated_at: string;
	channels: BrowserChannel[];
	projects: BrowserProject[];
}
