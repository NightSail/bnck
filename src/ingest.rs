use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::curation::{Project, load_projects, project_paths};
use crate::matcher::{
	Match, PackageFacts, ProjectQuery, group_packages, match_project, normalize_name, normalize_url, repository_name,
};
use crate::model::{read_candidate_directory, validate_channel, validate_identity};
use crate::registry::write_json;

type PackageKey = (String, String);

const URL_EVIDENCE: [&str; 5] = [
	"repository URL",
	"repository URL in package homepage",
	"homepage URL",
	"homepage URL in package repository",
	"project link URL",
];
const NAME_EVIDENCE: [&str; 3] = ["package name", "source package name", "executable name"];
const CODE_HOSTS: [&str; 13] = [
	"bitbucket.org",
	"codeberg.org",
	"git.dec05eba.com",
	"git.disroot.org",
	"git.sr.ht",
	"gitea.com",
	"github.com",
	"gitlab.com",
	"gitlab.gnome.org",
	"gitlab.mister-muffin.de",
	"invent.kde.org",
	"salsa.debian.org",
	"sr.ht",
];

#[derive(Debug, Deserialize)]
struct Hint {
	source: String,
	source_id: String,
	name: String,
	#[serde(default)]
	executable: Option<String>,
	#[serde(default)]
	repository: Option<String>,
	#[serde(default)]
	homepage: Option<String>,
	#[serde(default)]
	references: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RejectionFile {
	#[serde(default, rename = "rejection")]
	rejections: Vec<Rejection>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rejection {
	key: String,
	#[serde(default)]
	source: Option<String>,
	#[serde(default)]
	channel: Option<String>,
	#[serde(default)]
	package: Option<String>,
	reason: String,
}

#[derive(Debug, Default)]
struct RejectionSet {
	whole: bool,
	packages: BTreeSet<PackageKey>,
}

#[derive(Debug, Default)]
struct RejectionIndex {
	by_key: BTreeMap<String, RejectionSet>,
	by_source: BTreeMap<String, RejectionSet>,
}

struct PackageIndex {
	facts: BTreeMap<PackageKey, PackageFacts>,
	by_repository: BTreeMap<String, BTreeSet<PackageKey>>,
	by_homepage: BTreeMap<String, BTreeSet<PackageKey>>,
	by_package_name: BTreeMap<String, BTreeSet<PackageKey>>,
	by_source_package: BTreeMap<String, BTreeSet<PackageKey>>,
	by_executable: BTreeMap<String, BTreeSet<PackageKey>>,
}

#[derive(Default)]
struct Candidate {
	key: String,
	sources: BTreeSet<String>,
	source_ids: BTreeSet<String>,
	tt_names: BTreeSet<String>,
	tldr_names: BTreeSet<String>,
	packages: BTreeMap<PackageKey, BTreeSet<String>>,
	repository_urls: BTreeSet<String>,
	homepage_urls: BTreeSet<String>,
	repository_key: bool,
}

struct ProjectIndex {
	paths: BTreeMap<String, PathBuf>,
	claims: BTreeMap<PackageKey, String>,
	by_url: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Serialize)]
struct Plan {
	applied: bool,
	created: Vec<Action>,
	extended: Vec<Action>,
	conflicts: Vec<Conflict>,
	skipped: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
struct Action {
	key: String,
	project: String,
	name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	homepage: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	repository: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	path: Option<PathBuf>,
	packages: Vec<PackageKey>,
	sources: Vec<String>,
	source_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Conflict {
	key: String,
	reason: String,
}

pub fn ingest_hints(
	hints: &Path,
	acquisitions: &Path,
	curations: &Path,
	report_path: Option<&Path>,
	apply: bool,
) -> Result<()> {
	let packages = PackageIndex::load(acquisitions)?;
	let projects = load_projects(&curations.join("projects"))?;
	let project_index = ProjectIndex::load(curations, &projects)?;
	let rejections = load_rejections(&curations.join("rejections.toml"))?;
	let hints = read_hints(hints)?;
	let mut candidates = BTreeMap::new();
	for hint in &hints {
		match hint.source.as_str() {
			"terminaltrove" => add_terminaltrove(hint, &packages, &mut candidates),
			"tldr" => add_tldr(hint, &packages, &mut candidates),
			source => bail!("unknown hint source {source} in {}/{}", hint.source, hint.source_id),
		}
	}
	apply_rejections(&mut candidates, &rejections);
	apply_claims(&mut candidates, &project_index.claims);
	resolve_ambiguous_packages(&mut candidates);
	let (mut created, extended, mut conflicts, mut skipped) =
		plan_projects(candidates, &packages, &projects, &project_index);
	resolve_project_id_conflicts(&mut created, &mut conflicts, &mut skipped);
	let plan = Plan { applied: apply, created, extended, conflicts, skipped };

	if apply {
		apply_plan(&plan, curations)?;
	}
	print_summary(&plan);
	if let Some(path) = report_path {
		write_json(path, &plan)?;
	}
	Ok(())
}

impl PackageIndex {
	fn load(root: &Path) -> Result<Self> {
		let facts = group_packages(read_candidate_directory(root)?);
		let mut index = Self {
			facts,
			by_repository: BTreeMap::new(),
			by_homepage: BTreeMap::new(),
			by_package_name: BTreeMap::new(),
			by_source_package: BTreeMap::new(),
			by_executable: BTreeMap::new(),
		};
		for (key, facts) in &index.facts {
			for url in &facts.repositories {
				let normalized = normalize_url(url);
				if !normalized.is_empty() {
					index.by_repository.entry(normalized).or_default().insert(key.clone());
				}
			}
			for url in &facts.homepages {
				let normalized = normalize_url(url);
				if !normalized.is_empty() {
					index.by_homepage.entry(normalized).or_default().insert(key.clone());
				}
			}
			for name in package_names(&key.1) {
				if !name.is_empty() {
					index.by_package_name.entry(name).or_default().insert(key.clone());
				}
			}
			for source_package in &facts.source_packages {
				for name in package_names(source_package) {
					if !name.is_empty() {
						index.by_source_package.entry(name).or_default().insert(key.clone());
					}
				}
			}
			for executable in &facts.executables {
				let name = normalize_name(executable);
				if !name.is_empty() {
					index.by_executable.entry(name).or_default().insert(key.clone());
				}
			}
		}
		Ok(index)
	}

	fn matching(&self, query: &ProjectQuery) -> Vec<(PackageKey, Match)> {
		let mut keys = BTreeSet::new();
		if let Some(repository) = query.repository.as_deref() {
			keys.extend(
				self.by_repository
					.get(&normalize_url(repository))
					.into_iter()
					.flatten()
					.cloned(),
			);
		}
		if let Some(homepage) = query.homepage.as_deref() {
			keys.extend(self.by_homepage.get(&normalize_url(homepage)).into_iter().flatten().cloned());
		}
		for link in &query.links {
			let url = normalize_url(link);
			keys.extend(self.by_repository.get(&url).into_iter().flatten().cloned());
			keys.extend(self.by_homepage.get(&url).into_iter().flatten().cloned());
		}
		let mut names = BTreeSet::from([normalize_name(&query.name)]);
		if let Some(repository) = query.repository.as_deref().and_then(repository_name) {
			names.insert(normalize_name(repository));
		}
		names.retain(|name| !name.is_empty());
		for name in names {
			keys.extend(self.by_package_name.get(&name).into_iter().flatten().cloned());
			keys.extend(self.by_source_package.get(&name).into_iter().flatten().cloned());
			keys.extend(self.by_executable.get(&name).into_iter().flatten().cloned());
		}
		keys.into_iter()
			.filter_map(|key| {
				let facts = self.facts.get(&key)?;
				let matched = match_project(query, &key.1, facts)?;
				Some((key, matched))
			})
			.collect()
	}
}

impl ProjectIndex {
	fn load(root: &Path, projects: &BTreeMap<String, Project>) -> Result<Self> {
		let paths = project_paths(&root.join("projects"))?;
		let mut claims = BTreeMap::new();
		let mut by_url = BTreeMap::<String, BTreeSet<String>>::new();
		for project in projects.values() {
			for package in &project.packages {
				let key = (package.channel.clone(), package.name.clone());
				if let Some(owner) = claims.insert(key.clone(), project.id.clone()) {
					bail!("package ({}, {}) is claimed by projects {owner} and {}", key.0, key.1, project.id)
				}
			}
			for url in project
				.homepage
				.iter()
				.chain(project.repository.iter())
				.chain(project.links.iter())
			{
				let normalized = normalize_url(url);
				if !normalized.is_empty() {
					by_url.entry(normalized).or_default().insert(project.id.clone());
				}
			}
		}
		Ok(Self { paths, claims, by_url })
	}
}

fn add_terminaltrove(hint: &Hint, packages: &PackageIndex, candidates: &mut BTreeMap<String, Candidate>) {
	let Some(url) = hint.repository.as_deref().or(hint.homepage.as_deref()) else {
		return;
	};
	let key = normalize_url(url);
	if key.is_empty() {
		return;
	}
	let query = ProjectQuery {
		name: hint.name.clone(),
		homepage: hint.homepage.clone(),
		repository: hint.repository.clone(),
		links: Vec::new(),
	};
	let matches = cross_checked(packages.matching(&query));
	if matches.is_empty() {
		return;
	}
	let candidate = candidates.entry(key.clone()).or_default();
	candidate.key = key.clone();
	candidate.sources.insert("terminaltrove".to_owned());
	candidate.source_ids.insert(format!("terminaltrove:{}", hint.source_id));
	candidate.tt_names.insert(hint.name.clone());
	if let Some(repository) = &hint.repository {
		candidate.repository_urls.insert(repository.clone());
		candidate.repository_key = true;
	}
	if let Some(homepage) = &hint.homepage {
		candidate.homepage_urls.insert(homepage.clone());
	}
	add_matches(candidate, &key, matches, packages);
}

fn add_tldr(hint: &Hint, packages: &PackageIndex, candidates: &mut BTreeMap<String, Candidate>) {
	let executable = hint.executable.as_deref().unwrap_or(&hint.name);
	let mut choices = BTreeMap::<String, (bool, bool, Vec<(PackageKey, Match)>)>::new();
	for reference in &hint.references {
		for key in reference_keys(reference) {
			let repository = packages.by_repository.contains_key(&key);
			let homepage = packages.by_homepage.contains_key(&key);
			if !repository && !homepage {
				continue;
			}
			let query = ProjectQuery {
				name: executable.to_owned(),
				homepage: Some(key.clone()),
				repository: Some(key.clone()),
				links: Vec::new(),
			};
			let matches = cross_checked(packages.matching(&query));
			if matches.is_empty() {
				continue;
			}
			let choice = choices.entry(key).or_insert((repository, homepage, Vec::new()));
			choice.0 |= repository;
			choice.1 |= homepage;
			choice.2.extend(matches);
		}
	}
	let Some((key, (repository, _homepage, matches))) = choices.into_iter().max_by(|left, right| {
		let left_rank = (left.1.0, left.1.2.len(), left.1.1);
		let right_rank = (right.1.0, right.1.2.len(), right.1.1);
		left_rank.cmp(&right_rank).then_with(|| right.0.cmp(&left.0))
	}) else {
		return;
	};
	let candidate = candidates.entry(key.clone()).or_default();
	candidate.key = key.clone();
	candidate.sources.insert("tldr".to_owned());
	candidate.source_ids.insert(format!("tldr:{}", hint.source_id));
	candidate.tldr_names.insert(executable.to_owned());
	if repository {
		candidate.repository_key = true;
	}
	add_matches(candidate, &key, matches, packages);
}

fn add_matches(candidate: &mut Candidate, key: &str, matches: Vec<(PackageKey, Match)>, packages: &PackageIndex) {
	for (package, matched) in matches {
		candidate.packages.entry(package.clone()).or_default().extend(matched.evidence);
		let Some(facts) = packages.facts.get(&package) else {
			continue;
		};
		for url in &facts.repositories {
			if normalize_url(url) == key {
				candidate.repository_urls.insert(url.clone());
				candidate.repository_key = true;
			}
		}
		for url in &facts.homepages {
			if normalize_url(url) == key {
				candidate.homepage_urls.insert(url.clone());
			}
		}
	}
}

fn cross_checked(matches: Vec<(PackageKey, Match)>) -> Vec<(PackageKey, Match)> {
	matches
		.into_iter()
		.filter(|(_, matched)| {
			matched.evidence.iter().any(|value| URL_EVIDENCE.contains(&value.as_str()))
				&& matched.evidence.iter().any(|value| NAME_EVIDENCE.contains(&value.as_str()))
		})
		.collect()
}

fn reference_keys(reference: &str) -> BTreeSet<String> {
	let exact = normalize_url(reference);
	let mut keys = BTreeSet::from([exact.clone()]);
	if let Some(root) = repository_root(&exact) {
		keys.insert(root);
	}
	keys.retain(|key| !key.is_empty());
	keys
}

fn repository_root(url: &str) -> Option<String> {
	let (host, path) = url.split_once('/')?;
	if !CODE_HOSTS.contains(&host) {
		return None;
	}
	let segments = path.split('/').filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
	if segments.len() <= 2 {
		return None;
	}
	Some(format!("{host}/{}", segments[..2].join("/")))
}

fn apply_rejections(candidates: &mut BTreeMap<String, Candidate>, rejections: &RejectionIndex) {
	for candidate in candidates.values_mut() {
		let matched = rejections.by_key.get(&candidate.key).into_iter().chain(
			candidate
				.source_ids
				.iter()
				.filter_map(|source| rejections.by_source.get(source)),
		);
		let mut packages = BTreeSet::new();
		let mut rejected = false;
		for rejection in matched {
			rejected |= rejection.whole;
			packages.extend(&rejection.packages);
		}
		if rejected {
			candidate.packages.clear();
		} else {
			candidate.packages.retain(|package, _| !packages.contains(package));
		}
	}
	candidates.retain(|_, candidate| !candidate.packages.is_empty());
}

fn apply_claims(candidates: &mut BTreeMap<String, Candidate>, claims: &BTreeMap<PackageKey, String>) {
	for candidate in candidates.values_mut() {
		candidate.packages.retain(|package, _| !claims.contains_key(package));
	}
	candidates.retain(|_, candidate| !candidate.packages.is_empty());
}

fn resolve_ambiguous_packages(candidates: &mut BTreeMap<String, Candidate>) {
	let mut contenders = BTreeMap::<PackageKey, Vec<String>>::new();
	for (key, candidate) in candidates.iter() {
		for package in candidate.packages.keys() {
			contenders.entry(package.clone()).or_default().push(key.clone());
		}
	}
	let mut keep = BTreeMap::<PackageKey, Option<String>>::new();
	for (package, keys) in contenders {
		let best_rank = keys
			.iter()
			.map(|key| candidate_rank(&candidates[key]))
			.max()
			.expect("a contended package has at least one candidate");
		let mut best = keys.into_iter().filter(|key| candidate_rank(&candidates[key]) == best_rank);
		let winner = best.next();
		if best.next().is_some() {
			keep.insert(package, None);
		} else {
			keep.insert(package, winner);
		}
	}
	for (key, candidate) in candidates.iter_mut() {
		candidate
			.packages
			.retain(|package, _| keep.get(package).is_some_and(|winner| winner.as_ref() == Some(key)));
	}
	candidates.retain(|_, candidate| !candidate.packages.is_empty());
}

fn candidate_rank(candidate: &Candidate) -> (bool, bool) {
	(!candidate.tt_names.is_empty(), candidate.repository_key)
}

fn plan_projects(
	candidates: BTreeMap<String, Candidate>,
	packages: &PackageIndex,
	projects: &BTreeMap<String, Project>,
	project_index: &ProjectIndex,
) -> (Vec<Action>, Vec<Action>, Vec<Conflict>, BTreeMap<String, usize>) {
	let mut created = Vec::new();
	let mut extended = Vec::new();
	let mut conflicts = Vec::new();
	let mut skipped = BTreeMap::new();
	for (key, candidate) in candidates {
		let package_keys = candidate.packages.keys().cloned().collect::<Vec<_>>();
		if let Some(ids) = project_index.by_url.get(&key) {
			if ids.len() != 1 {
				conflicts.push(Conflict {
					key,
					reason: format!(
						"URL is claimed by projects {}",
						ids.iter().cloned().collect::<Vec<_>>().join(", ")
					),
				});
				*skipped.entry("URL conflict".to_owned()).or_default() += 1;
				continue;
			}
			let project = ids.iter().next().expect("one project ID").clone();
			let Some(path) = project_index.paths.get(&project) else {
				conflicts.push(Conflict { key, reason: format!("project {project} has no curation path") });
				*skipped.entry("missing project file".to_owned()).or_default() += 1;
				continue;
			};
			extended.push(Action {
				key,
				project: project.clone(),
				name: project,
				homepage: None,
				repository: None,
				path: Some(path.clone()),
				packages: package_keys,
				sources: candidate.sources.into_iter().collect(),
				source_ids: candidate.source_ids.into_iter().collect(),
			});
			continue;
		}
		if package_keys.is_empty() {
			*skipped.entry("already curated".to_owned()).or_default() += 1;
			continue;
		}
		let name = candidate_name(&candidate, packages, projects);
		let id = project_id(&name);
		if id.is_empty() {
			conflicts.push(Conflict { key, reason: "project name produces an empty ID".to_owned() });
			*skipped.entry("invalid project ID".to_owned()).or_default() += 1;
			continue;
		}
		if projects.contains_key(&id) || project_index.paths.contains_key(&id) {
			conflicts.push(Conflict { key, reason: format!("project ID {id} is already used by a different URL") });
			*skipped.entry("project ID conflict".to_owned()).or_default() += 1;
			continue;
		}
		let repository = candidate.repository_urls.iter().next().map(|url| clean_url(url));
		let homepage = candidate
			.homepage_urls
			.iter()
			.map(|url| clean_url(url))
			.find(|url| Some(url) != repository.as_ref());
		created.push(Action {
			key,
			project: id,
			name,
			homepage,
			repository,
			path: None,
			packages: package_keys,
			sources: candidate.sources.into_iter().collect(),
			source_ids: candidate.source_ids.into_iter().collect(),
		});
	}
	(created, extended, conflicts, skipped)
}

fn resolve_project_id_conflicts(
	created: &mut Vec<Action>,
	conflicts: &mut Vec<Conflict>,
	skipped: &mut BTreeMap<String, usize>,
) {
	let mut counts = BTreeMap::<String, usize>::new();
	for action in created.iter() {
		*counts.entry(action.project.clone()).or_default() += 1;
	}
	let mut retained = Vec::new();
	for action in created.drain(..) {
		if counts.get(&action.project).copied().unwrap_or_default() > 1 {
			conflicts.push(Conflict {
				key: action.key,
				reason: format!("multiple new candidates produce project ID {}", action.project),
			});
			*skipped.entry("project ID conflict".to_owned()).or_default() += 1;
		} else {
			retained.push(action);
		}
	}
	*created = retained;
}

fn candidate_name(candidate: &Candidate, packages: &PackageIndex, _curated: &BTreeMap<String, Project>) -> String {
	if let Some(name) = candidate.tt_names.iter().next() {
		return name.clone();
	}
	let url = candidate
		.repository_urls
		.iter()
		.next()
		.or_else(|| candidate.homepage_urls.iter().next());
	let base = url.and_then(|url| url_name(url));
	let package_names = candidate
		.packages
		.keys()
		.map(|(_, package)| package.rsplit('/').next().unwrap_or(package).to_owned())
		.collect::<BTreeSet<_>>();
	let source_names = candidate
		.packages
		.keys()
		.filter_map(|package| packages.facts.get(package))
		.flat_map(|facts| facts.source_packages.iter().cloned())
		.collect::<BTreeSet<_>>();
	let executable_names = candidate
		.packages
		.keys()
		.filter_map(|package| packages.facts.get(package))
		.flat_map(|facts| facts.executables.iter().cloned())
		.collect::<BTreeSet<_>>();
	let targets = package_names
		.iter()
		.chain(source_names.iter())
		.chain(executable_names.iter())
		.map(|name| normalize_name(name))
		.filter(|name| !name.is_empty())
		.collect::<BTreeSet<_>>();
	let hints = candidate.tldr_names.iter().cloned().collect::<Vec<_>>();
	let base_normalized = base.as_deref().map(normalize_name).unwrap_or_default();
	if !base_normalized.is_empty() {
		if let Some(name) = package_names
			.iter()
			.find(|name| normalize_name(name) == base_normalized)
			.or_else(|| source_names.iter().find(|name| normalize_name(name) == base_normalized))
		{
			return name.clone();
		}
		if package_names.len() > 1
			&& package_names.iter().all(|name| {
				normalize_name(name).starts_with(&base_normalized) || normalize_name(name).ends_with(&base_normalized)
			}) {
			return base.expect("base exists");
		}
		if let Some(name) = source_names
			.iter()
			.chain(package_names.iter())
			.filter(|name| {
				let normalized = normalize_name(name);
				!normalized.is_empty()
					&& (base_normalized.starts_with(&normalized) || base_normalized.ends_with(&normalized))
			})
			.min_by_key(|name| (name.len(), name.as_str()))
		{
			return name.clone();
		}
		if !is_generic_name(&base_normalized)
			&& source_names
				.iter()
				.chain(package_names.iter())
				.any(|name| normalize_name(name).contains(&base_normalized))
		{
			return base.expect("base exists");
		}
		if let Some(name) = package_names.iter().find(|name| {
			let normalized = normalize_name(name);
			hints.iter().any(|hint| normalize_name(hint) == normalized)
		}) {
			return name.clone();
		}
		if let Some(name) = source_names
			.iter()
			.chain(package_names.iter())
			.filter(|name| {
				let normalized = normalize_name(name);
				!normalized.is_empty() && hints.iter().any(|hint| normalize_name(hint).starts_with(&normalized))
			})
			.min_by_key(|name| (name.len(), name.as_str()))
		{
			return name.clone();
		}
		let hints_related_to_base = hints.iter().any(|hint| {
			let normalized = normalize_name(hint);
			normalized.starts_with(&base_normalized)
				|| normalized.ends_with(&base_normalized)
				|| base_normalized.starts_with(&normalized)
				|| base_normalized.ends_with(&normalized)
		});
		if !hints_related_to_base && base_normalized.chars().any(|character| character.is_ascii_digit()) {
			return fallback_name(&package_names, &source_names).unwrap_or_else(|| base.expect("base exists"));
		}
		if let Some(name) = hints.iter().find(|hint| targets.contains(&normalize_name(hint))) {
			return name.clone();
		}
		if targets
			.iter()
			.any(|target| target.starts_with(&base_normalized) || target.ends_with(&base_normalized))
			|| hints.iter().any(|hint| {
				let normalized = normalize_name(hint);
				normalized.starts_with(&base_normalized) || normalized.ends_with(&base_normalized)
			}) {
			return base.expect("base exists");
		}
		return fallback_name(&package_names, &source_names).unwrap_or_else(|| base.expect("base exists"));
	}
	if let Some(name) = package_names.iter().find(|name| {
		let normalized = normalize_name(name);
		hints.iter().any(|hint| normalize_name(hint) == normalized)
	}) {
		return name.clone();
	}
	if let Some(name) = hints.iter().find(|hint| targets.contains(&normalize_name(hint))) {
		return name.clone();
	}
	hints
		.first()
		.cloned()
		.or_else(|| fallback_name(&package_names, &source_names))
		.unwrap_or_else(|| candidate.key.clone())
}

fn is_generic_name(name: &str) -> bool {
	matches!(name, "api" | "app" | "apps" | "bin" | "cli" | "doc" | "docs" | "index" | "tool" | "tools" | "web")
}

fn fallback_name(package_names: &BTreeSet<String>, source_names: &BTreeSet<String>) -> Option<String> {
	if source_names.len() == 1 {
		return source_names.iter().next().cloned();
	}
	package_names.iter().min_by_key(|name| (name.len(), name.as_str())).cloned()
}

fn url_name(url: &str) -> Option<String> {
	let normalized = normalize_url(url);
	let (host, path) = normalized.split_once('/')?;
	let mut segments = path.split('/').filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
	if let Some(name) = host.strip_suffix(".readthedocs.io")
		&& !name.is_empty()
		&& name != "www"
	{
		return Some(name.to_owned());
	}
	while let Some(segment) = segments.last() {
		if matches!(
			segment.to_ascii_lowercase().as_str(),
			"about"
				| "doc" | "docs"
				| "documentation"
				| "htm" | "html"
				| "index" | "latest"
				| "man" | "project"
				| "projects" | "reference"
				| "start"
		) {
			segments.pop();
		} else {
			break;
		}
	}
	let mut name = segments.last()?.to_string();
	for suffix in [".git", ".htm", ".html", ".php"] {
		if name.to_ascii_lowercase().ends_with(suffix) {
			name.truncate(name.len() - suffix.len());
		}
	}
	if name.is_empty() { None } else { Some(name) }
}

fn clean_url(url: &str) -> String {
	url.trim_end_matches('/').trim_end_matches(".git").to_owned()
}

fn project_id(name: &str) -> String {
	let mut id = String::new();
	let mut separator = false;
	for character in name.chars() {
		if character.is_ascii_alphanumeric() {
			id.push(character.to_ascii_lowercase());
			separator = false;
		} else if !id.is_empty() && !separator {
			id.push('-');
			separator = true;
		}
	}
	id.trim_matches('-').to_owned()
}

fn package_names(package: &str) -> BTreeSet<String> {
	let mut names = BTreeSet::from([normalize_name(package)]);
	if let Some(name) = package.rsplit('/').next() {
		names.insert(normalize_name(name));
	}
	names.retain(|name| !name.is_empty());
	names
}

fn read_hints(root: &Path) -> Result<Vec<Hint>> {
	let mut hints = Vec::new();
	for entry in WalkDir::new(root).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", root.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "jsonl") {
			continue;
		}
		let path = entry.path();
		let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
		for (index, line) in BufReader::new(file).lines().enumerate() {
			let line = line.with_context(|| format!("read {} line {}", path.display(), index + 1))?;
			if line.trim().is_empty() {
				continue;
			}
			let hint: Hint =
				serde_json::from_str(&line).with_context(|| format!("parse {} line {}", path.display(), index + 1))?;
			ensure!(
				matches!(hint.source.as_str(), "terminaltrove" | "tldr"),
				"unknown hint source {} in {}",
				hint.source,
				path.display()
			);
			ensure!(!hint.source_id.is_empty(), "{} line {} has an empty source ID", path.display(), index + 1);
			ensure!(!hint.name.is_empty(), "{} line {} has an empty name", path.display(), index + 1);
			hints.push(hint);
		}
	}
	hints.sort_by(|left, right| (&left.source, &left.source_id).cmp(&(&right.source, &right.source_id)));
	Ok(hints)
}

fn load_rejections(path: &Path) -> Result<RejectionIndex> {
	if !path.exists() {
		return Ok(RejectionIndex::default());
	}
	let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
	let file: RejectionFile = toml::from_str(&contents).with_context(|| format!("parse {}", path.display()))?;
	let mut rejections = RejectionIndex::default();
	for rejection in file.rejections {
		let key = canonical_key(&rejection.key);
		ensure!(!key.is_empty(), "rejection key is empty");
		ensure!(
			!key.chars().any(|character| character.is_control() || character.is_whitespace()),
			"rejection key {key:?} contains whitespace or a control character"
		);
		ensure!(!rejection.reason.trim().is_empty(), "rejection {key} has no reason");
		let source = rejection.source.as_deref().map(|source| source.trim().to_owned());
		if let Some(source) = &source {
			ensure!(!source.is_empty(), "rejection {key} has an empty source");
			ensure!(
				!source
					.chars()
					.any(|character| character.is_control() || character.is_whitespace()),
				"rejection source {source:?} contains whitespace or a control character"
			);
		}
		let key_set = rejections.by_key.entry(key.clone()).or_default();
		add_rejection(key_set, &rejection, &key)?;
		if let Some(source) = source {
			let source_set = rejections.by_source.entry(source.clone()).or_default();
			add_rejection(source_set, &rejection, &source)?;
		}
	}
	Ok(rejections)
}

fn add_rejection(set: &mut RejectionSet, rejection: &Rejection, label: &str) -> Result<()> {
	match (&rejection.channel, &rejection.package) {
		(None, None) => {
			ensure!(!set.whole, "rejection {label} is duplicated");
			set.whole = true;
		},
		(Some(channel), Some(package)) => {
			validate_channel(channel)?;
			validate_identity("package", package)?;
			let package = (channel.clone(), package.clone());
			ensure!(set.packages.insert(package), "rejection {label} has a duplicate package");
		},
		_ => bail!("rejection {label} must specify both channel and package, or neither"),
	}
	Ok(())
}

fn canonical_key(value: &str) -> String {
	let value = value.trim();
	if value.starts_with("terminaltrove:") || value.starts_with("tldr:") {
		value.to_owned()
	} else {
		normalize_url(value)
	}
}

fn apply_plan(plan: &Plan, curations: &Path) -> Result<()> {
	let projects = curations.join("projects");
	for action in &plan.created {
		let path = projects.join(format!("{}.toml", action.project));
		let content = render_project(action);
		fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
	}
	for action in &plan.extended {
		let path = action
			.path
			.as_deref()
			.with_context(|| format!("project {} has no path", action.project))?;
		append_packages(path, &action.packages)?;
	}
	Ok(())
}

fn render_project(action: &Action) -> String {
	let mut content = format!("id = {}\nname = {}\n", toml_string(&action.project), toml_string(&action.name));
	if action.homepage.is_some() || action.repository.is_some() {
		content.push('\n');
		if let Some(homepage) = &action.homepage {
			content.push_str(&format!("homepage = {}\n", toml_string(homepage)));
		}
		if let Some(repository) = &action.repository {
			content.push_str(&format!("repository = {}\n", toml_string(repository)));
		}
	}
	content.push_str(&render_packages(&action.packages));
	content
}

fn append_packages(path: &Path, packages: &[PackageKey]) -> Result<()> {
	let mut content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
	if !content.ends_with('\n') {
		content.push('\n');
	}
	content.push_str(&render_packages(packages));
	fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

fn render_packages(packages: &[PackageKey]) -> String {
	let mut content = String::new();
	for (channel, package) in packages {
		content.push_str(&format!(
			"\n[[package]]\nchannel = {}\nname = {}\n",
			toml_string(channel),
			toml_string(package)
		));
	}
	content
}

fn toml_string(value: &str) -> String {
	if !value.contains('\'') && !value.contains('\n') && !value.contains('\r') {
		format!("'{value}'")
	} else {
		serde_json::to_string(value).expect("serialize TOML string")
	}
}

fn print_summary(plan: &Plan) {
	let verb = if plan.applied { "Created" } else { "Would create" };
	for action in &plan.created {
		println!(
			"{verb} {} ({}) with {} packages from {}",
			action.project,
			action.key,
			action.packages.len(),
			action.source_ids.join(", ")
		);
	}
	let verb = if plan.applied { "Extended" } else { "Would extend" };
	for action in &plan.extended {
		println!(
			"{verb} {} with {} packages from {}",
			action.project,
			action.packages.len(),
			action.source_ids.join(", ")
		);
	}
	for conflict in &plan.conflicts {
		eprintln!("conflict {}: {}", conflict.key, conflict.reason);
	}
	for (reason, count) in &plan.skipped {
		eprintln!("skipped {count} ({reason})");
	}
	println!(
		"{} {} projects and {} existing projects; {} conflicts",
		if plan.applied { "wrote" } else { "planned" },
		plan.created.len(),
		plan.extended.len(),
		plan.conflicts.len()
	);
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::{ingest_hints, reference_keys};

	fn write_curations(root: &std::path::Path) {
		fs::create_dir_all(root.join("projects")).unwrap();
		fs::write(root.join("manifest.toml"), "schema = 1\n").unwrap();
	}

	fn write_candidate(root: &std::path::Path, package: &str, executable: &str, repository: &str) {
		fs::create_dir_all(root).unwrap();
		fs::write(
			root.join("packages.jsonl"),
			format!(
				"{{\"channel\":\"arch\",\"package\":\"{package}\",\"version\":\"1\",\
				 \"executable\":\"{executable}\",\"evidence\":\"repository-file-listing\",\
				 \"repository\":\"{repository}\"}}\n"
			),
		)
		.unwrap();
	}

	#[test]
	fn creates_cross_checked_terminaltrove_project_and_is_idempotent() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		write_candidate(&acquisitions, "foo", "foo", "https://github.com/example/foo");
		write_curations(&curations);
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("terminaltrove.jsonl"),
			"{\"source\":\"terminaltrove\",\"source_id\":\"foo\",\"name\":\"Foo\",\
			 \"repository\":\"https://github.com/example/foo\"}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		let path = curations.join("projects/foo.toml");
		let first = fs::read_to_string(&path).unwrap();
		assert!(first.contains("id = 'foo'"));
		assert!(first.contains("repository = 'https://github.com/example/foo'"));
		assert!(first.contains("channel = 'arch'\nname = 'foo'"));

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		assert_eq!(fs::read_to_string(path).unwrap(), first);
	}

	#[test]
	fn tldr_subcommands_use_the_executable_and_repository_root() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		write_candidate(&acquisitions, "tool", "tool", "https://github.com/example/tool");
		write_curations(&curations);
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("tldr.jsonl"),
			"{\"source\":\"tldr\",\"source_id\":\"common/tool-sub\",\"name\":\"tool sub\",\
			 \"executable\":\"tool\",\"references\":[\"https://github.com/example/tool/blob/main/README.md\"]}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		let content = fs::read_to_string(curations.join("projects/tool.toml")).unwrap();
		assert!(content.contains("id = 'tool'"));
		assert!(content.contains("name = 'tool'"));
	}

	#[test]
	fn extends_an_existing_project_without_duplicating_packages() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		write_candidate(&acquisitions, "tool", "tool", "https://github.com/example/tool");
		write_curations(&curations);
		fs::write(
			curations.join("projects/tool.toml"),
			"id = 'tool'\nname = 'tool'\n\nrepository = 'https://github.com/example/tool'\n",
		)
		.unwrap();
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("terminaltrove.jsonl"),
			"{\"source\":\"terminaltrove\",\"source_id\":\"tool\",\"name\":\"tool\",\
			 \"repository\":\"https://github.com/example/tool\"}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		let content = fs::read_to_string(curations.join("projects/tool.toml")).unwrap();
		assert_eq!(content.matches("[[package]]").count(), 1);
		assert!(content.contains("channel = 'arch'\nname = 'tool'"));
	}

	#[test]
	fn rejects_a_whole_candidate_by_key() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		write_candidate(&acquisitions, "foo", "foo", "https://github.com/example/foo");
		write_curations(&curations);
		fs::write(
			curations.join("rejections.toml"),
			"[[rejection]]\nkey = 'github.com/example/foo'\nreason = 'Wrong project'\n",
		)
		.unwrap();
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("terminaltrove.jsonl"),
			"{\"source\":\"terminaltrove\",\"source_id\":\"foo\",\"name\":\"Foo\",\
			 \"repository\":\"https://github.com/example/foo\"}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		assert!(!curations.join("projects/foo.toml").exists());
	}

	#[test]
	fn rejects_by_source_when_the_candidate_url_changes() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		write_candidate(&acquisitions, "foo", "foo", "https://github.com/example/foo-new");
		write_curations(&curations);
		fs::write(
			curations.join("rejections.toml"),
			"[[rejection]]\nkey = 'github.com/example/foo-old'\nsource = 'terminaltrove:foo'\n\
			 reason = 'Wrong project'\n",
		)
		.unwrap();
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("terminaltrove.jsonl"),
			"{\"source\":\"terminaltrove\",\"source_id\":\"foo\",\"name\":\"Foo\",\
			 \"repository\":\"https://github.com/example/foo-new\"}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		assert!(!curations.join("projects/foo.toml").exists());
	}

	#[test]
	fn rejects_one_package_from_an_otherwise_accepted_candidate() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let hints = root.path().join("hints");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::write(
			acquisitions.join("packages.jsonl"),
			concat!(
				"{\"channel\":\"arch\",\"package\":\"a\",\"version\":\"1\",\"executable\":\"tool\",\
				 \"evidence\":\"repository-file-listing\",\"repository\":\"https://github.com/example/tool\"}\n",
				"{\"channel\":\"arch\",\"package\":\"b\",\"version\":\"1\",\"executable\":\"tool\",\
				 \"evidence\":\"repository-file-listing\",\"repository\":\"https://github.com/example/tool\"}\n",
			),
		)
		.unwrap();
		write_curations(&curations);
		fs::write(
			curations.join("rejections.toml"),
			"[[rejection]]\nkey = 'github.com/example/tool'\nchannel = 'arch'\npackage = 'a'\n\
			 reason = 'Package a is a different project'\n",
		)
		.unwrap();
		fs::create_dir_all(&hints).unwrap();
		fs::write(
			hints.join("tldr.jsonl"),
			"{\"source\":\"tldr\",\"source_id\":\"common/tool\",\"name\":\"tool\",\
			 \"executable\":\"tool\",\"references\":[\"https://github.com/example/tool\"]}\n",
		)
		.unwrap();

		ingest_hints(&hints, &acquisitions, &curations, None, true).unwrap();
		let content = fs::read_to_string(curations.join("projects/tool.toml")).unwrap();
		assert!(content.contains("name = 'b'"));
		assert!(!content.contains("name = 'a'"));
	}

	#[test]
	fn expands_code_host_references_to_their_repository_root() {
		let keys = reference_keys("https://github.com/Owner/Repo/blob/main/README.md");
		assert!(keys.contains("github.com/owner/repo"));
	}
}
