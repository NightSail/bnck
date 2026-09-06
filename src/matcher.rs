use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Result, ensure};

use crate::curation::{curated_package_projects, curated_packages, load_projects};
use crate::model::{Candidate, read_candidate_directory, validate_channel, validate_identity};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
	Weak,
	Possible,
	Strong,
	Exact,
}

impl Confidence {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Exact => "exact",
			Self::Strong => "strong",
			Self::Possible => "possible",
			Self::Weak => "weak",
		}
	}
}

#[derive(Debug)]
pub struct ProjectQuery {
	pub name: String,
	pub homepage: Option<String>,
	pub repository: Option<String>,
	pub links: Vec<String>,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize)]
pub struct PackageSuggestion {
	pub confidence: Confidence,
	pub channel: String,
	pub package: String,
	pub executables: Vec<String>,
	pub evidence: Vec<String>,
	pub homepages: Vec<String>,
	pub repositories: Vec<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProjectSuggestion {
	pub confidence: Confidence,
	pub id: Option<String>,
	pub name: String,
	pub evidence: Vec<String>,
	pub homepages: Vec<String>,
	pub repositories: Vec<String>,
}

#[derive(Default)]
struct PackageFacts {
	executables: BTreeSet<String>,
	source_packages: BTreeSet<String>,
	homepages: BTreeSet<String>,
	repositories: BTreeSet<String>,
}

#[derive(Debug)]
struct Match {
	confidence: Confidence,
	evidence: Vec<String>,
}

pub fn suggest_packages(acquisitions: &Path, query: &ProjectQuery, limit: usize) -> Result<Vec<PackageSuggestion>> {
	validate_query(query, limit)?;
	let packages = group_packages(read_candidate_directory(acquisitions)?);
	let mut suggestions = packages
		.into_iter()
		.filter_map(|((channel, package), facts)| {
			let matched = match_project(query, &package, &facts)?;
			Some(PackageSuggestion {
				confidence: matched.confidence,
				channel,
				package,
				executables: facts.executables.into_iter().collect(),
				evidence: matched.evidence,
				homepages: facts.homepages.into_iter().collect(),
				repositories: facts.repositories.into_iter().collect(),
			})
		})
		.collect::<Vec<_>>();
	suggestions.sort_by_key(|suggestion| {
		(Reverse(suggestion.confidence), suggestion.channel.clone(), suggestion.package.clone())
	});
	suggestions.truncate(limit);
	Ok(suggestions)
}

pub fn suggest_projects(
	acquisitions: &Path,
	curations: &Path,
	channel: &str,
	package: &str,
	limit: usize,
) -> Result<Vec<ProjectSuggestion>> {
	validate_channel(channel)?;
	validate_identity("package", package)?;
	ensure!(limit > 0, "limit must be greater than zero");

	let mut packages = group_packages(read_candidate_directory(acquisitions)?);
	let facts = packages
		.remove(&(channel.to_owned(), package.to_owned()))
		.ok_or_else(|| anyhow::anyhow!("package ({channel}, {package}) is absent from acquisition"))?;
	let projects = load_projects(&curations.join("projects"))?;
	let curated_project_ids = curated_package_projects(curations, channel, package)?;
	let mut suggestions = Vec::new();

	for project in projects.values() {
		let matched = if curated_project_ids.contains(&project.id) {
			Some(Match { confidence: Confidence::Exact, evidence: vec!["curated package attribution".to_owned()] })
		} else {
			match_project(
				&ProjectQuery {
					name: project.name.clone(),
					homepage: project.homepage.clone(),
					repository: project.repository.clone(),
					links: project.links.clone(),
				},
				package,
				&facts,
			)
		};
		if let Some(matched) = matched {
			suggestions.push(ProjectSuggestion {
				confidence: matched.confidence,
				id: Some(project.id.clone()),
				name: project.name.clone(),
				evidence: matched.evidence,
				homepages: project.homepage.iter().cloned().collect(),
				repositories: project.repository.iter().cloned().collect(),
			});
		}
	}

	if !suggestions.iter().any(|suggestion| suggestion.confidence >= Confidence::Strong) {
		suggestions.push(infer_project(channel, package, &facts));
	}
	suggestions.sort_by_key(|suggestion| {
		(Reverse(suggestion.confidence), suggestion.id.clone().unwrap_or_default(), suggestion.name.clone())
	});
	suggestions.truncate(limit);
	Ok(suggestions)
}

#[derive(serde::Serialize)]
pub struct RegistrySuggestion {
	pub project: String,
	#[serde(flatten)]
	pub package: PackageSuggestion,
}

pub fn suggest_registry_packages(acquisitions: &Path, curations: &Path) -> Result<Vec<RegistrySuggestion>> {
	let channels = ["crates.io", "npm", "pypi"];
	let mut owned = BTreeSet::new();
	for channel in channels {
		for name in curated_packages(curations, channel)? {
			owned.insert((channel.to_owned(), name));
		}
	}
	let packages = group_packages(read_candidate_directory(acquisitions)?);
	let projects = load_projects(&curations.join("projects"))?;
	let mut suggestions = Vec::new();
	for project in projects.into_values() {
		let query = ProjectQuery {
			name: project.name,
			homepage: project.homepage,
			repository: project.repository,
			links: project.links,
		};
		for ((channel, package), facts) in &packages {
			if !channels.contains(&channel.as_str()) || owned.contains(&(channel.clone(), package.clone())) {
				continue;
			}
			let Some(matched) = match_project(&query, package, facts) else {
				continue;
			};
			if matched.confidence < Confidence::Strong {
				continue;
			}
			suggestions.push(RegistrySuggestion {
				project: project.id.clone(),
				package: PackageSuggestion {
					confidence: matched.confidence,
					channel: channel.clone(),
					package: package.clone(),
					executables: facts.executables.iter().cloned().collect(),
					evidence: matched.evidence,
					homepages: facts.homepages.iter().cloned().collect(),
					repositories: facts.repositories.iter().cloned().collect(),
				},
			});
		}
	}
	Ok(suggestions)
}

fn infer_project(channel: &str, package: &str, facts: &PackageFacts) -> ProjectSuggestion {
	let repository_names = facts
		.repositories
		.iter()
		.filter_map(|repository| repository_name(repository).map(str::to_owned))
		.collect::<BTreeSet<_>>();
	let name = if repository_names.len() == 1 {
		repository_names.first().cloned().unwrap()
	} else if facts.source_packages.len() == 1 {
		facts.source_packages.first().cloned().unwrap()
	} else {
		package.rsplit('/').next().unwrap_or(package).to_owned()
	};
	let mut evidence = Vec::new();
	if !facts.repositories.is_empty() {
		evidence.push("repository from package metadata".to_owned());
	}
	if !facts.homepages.is_empty() {
		evidence.push("homepage from package metadata".to_owned());
	}
	if !facts.source_packages.is_empty() {
		evidence.push("source package name".to_owned());
	}
	if evidence.is_empty() {
		evidence.push("package name".to_owned());
	}
	let confidence = if !facts.repositories.is_empty() && matches!(channel, "crates.io" | "npm" | "pypi") {
		Confidence::Strong
	} else if !facts.repositories.is_empty() || !facts.homepages.is_empty() || !facts.source_packages.is_empty() {
		Confidence::Possible
	} else {
		Confidence::Weak
	};

	ProjectSuggestion {
		confidence,
		id: None,
		name,
		evidence,
		homepages: facts.homepages.iter().cloned().collect(),
		repositories: facts.repositories.iter().cloned().collect(),
	}
}

fn validate_query(query: &ProjectQuery, limit: usize) -> Result<()> {
	ensure!(!query.name.trim().is_empty(), "project name is empty");
	ensure!(limit > 0, "limit must be greater than zero");
	Ok(())
}

fn group_packages(candidates: Vec<Candidate>) -> BTreeMap<(String, String), PackageFacts> {
	let mut packages = BTreeMap::<(String, String), PackageFacts>::new();
	for candidate in candidates {
		let facts = packages.entry((candidate.channel, candidate.package)).or_default();
		facts.executables.insert(candidate.executable);
		if let Some(source_package) = candidate.source_package {
			facts.source_packages.insert(source_package);
		}
		if let Some(homepage) = candidate.homepage {
			facts.homepages.insert(homepage);
		}
		if let Some(repository) = candidate.repository {
			facts.repositories.insert(repository);
		}
	}
	packages
}

fn match_project(query: &ProjectQuery, package: &str, facts: &PackageFacts) -> Option<Match> {
	let mut evidence = BTreeSet::new();
	let has_repository_match = query
		.repository
		.as_deref()
		.is_some_and(|repository| urls_contain(&facts.repositories, repository));
	let has_repository_homepage_match = !has_repository_match
		&& query
			.repository
			.as_deref()
			.is_some_and(|repository| urls_contain(&facts.homepages, repository));
	let has_homepage_match = query
		.homepage
		.as_deref()
		.is_some_and(|homepage| urls_contain(&facts.homepages, homepage));
	let has_homepage_repository_match = !has_homepage_match
		&& query
			.homepage
			.as_deref()
			.is_some_and(|homepage| urls_contain(&facts.repositories, homepage));
	let has_link_match = query
		.links
		.iter()
		.any(|link| urls_contain(&facts.homepages, link) || urls_contain(&facts.repositories, link));
	let names = project_names(query);
	let has_package_match = names.iter().any(|name| package_names(package).contains(name));
	let has_source_package_match = facts
		.source_packages
		.iter()
		.any(|source_package| names.iter().any(|name| package_names(source_package).contains(name)));
	let has_executable_match = facts
		.executables
		.iter()
		.any(|executable| names.contains(&normalize_name(executable)));

	if has_repository_match {
		evidence.insert("repository URL".to_owned());
	}
	if has_repository_homepage_match {
		evidence.insert("repository URL in package homepage".to_owned());
	}
	if has_homepage_match {
		evidence.insert("homepage URL".to_owned());
	}
	if has_homepage_repository_match {
		evidence.insert("homepage URL in package repository".to_owned());
	}
	if has_link_match {
		evidence.insert("project link URL".to_owned());
	}
	if has_package_match {
		evidence.insert("package name".to_owned());
	}
	if has_source_package_match {
		evidence.insert("source package name".to_owned());
	}
	if has_executable_match {
		evidence.insert("executable name".to_owned());
	}

	let has_name_and_executable_match = (has_package_match || has_source_package_match) && has_executable_match;
	let confidence = if has_repository_match {
		Confidence::Exact
	} else if has_homepage_match
		|| has_repository_homepage_match
		|| has_homepage_repository_match
		|| has_link_match
		|| has_name_and_executable_match
	{
		Confidence::Strong
	} else if has_package_match || has_source_package_match {
		Confidence::Possible
	} else if has_executable_match {
		Confidence::Weak
	} else {
		return None;
	};

	Some(Match { confidence, evidence: evidence.into_iter().collect() })
}

fn project_names(query: &ProjectQuery) -> BTreeSet<String> {
	let mut names = BTreeSet::from([normalize_name(&query.name)]);
	if let Some(repository) = query.repository.as_deref().and_then(repository_name) {
		names.insert(normalize_name(repository));
	}
	names.retain(|name| !name.is_empty());
	names
}

fn package_names(package: &str) -> BTreeSet<String> {
	let mut names = BTreeSet::from([normalize_name(package)]);
	if let Some(name) = package.rsplit('/').next() {
		names.insert(normalize_name(name));
	}
	names.retain(|name| !name.is_empty());
	names
}

pub(crate) fn normalize_name(value: &str) -> String {
	value
		.chars()
		.filter(|character| character.is_ascii_alphanumeric())
		.flat_map(char::to_lowercase)
		.collect()
}

fn urls_contain(values: &BTreeSet<String>, expected: &str) -> bool {
	let expected = normalize_url(expected);
	!expected.is_empty() && values.iter().any(|value| normalize_url(value) == expected)
}

pub(crate) fn repository_name(repository: &str) -> Option<&str> {
	let repository = repository
		.split(['?', '#'])
		.next()?
		.trim_end_matches('/')
		.trim_end_matches(".git");
	repository.rsplit(['/', ':']).next().filter(|name| !name.is_empty())
}

pub(crate) fn normalize_url(value: &str) -> String {
	let mut value = value
		.trim()
		.split(['?', '#'])
		.next()
		.unwrap_or("")
		.trim_end_matches('/')
		.to_owned();
	if value.to_ascii_lowercase().starts_with("git+") {
		value = value[4..].to_owned();
	}
	for prefix in ["https://", "http://", "git://", "ssh://"] {
		if value.to_ascii_lowercase().starts_with(prefix) {
			value = value[prefix.len()..].to_owned();
			break;
		}
	}
	for prefix in ["git@", "www."] {
		if value.to_ascii_lowercase().starts_with(prefix) {
			value = value[prefix.len()..].to_owned();
		}
	}
	if let Some((host, path)) = value.split_once(':')
		&& (!path.contains('/') || matches!(host.to_ascii_lowercase().as_str(), "github.com" | "gitlab.com"))
	{
		value = format!("{host}/{path}");
	}
	value = value.trim_end_matches('/').trim_end_matches(".git").to_owned();
	let (host, path) = value.split_once('/').unwrap_or((&value, ""));
	let host = host.to_ascii_lowercase();
	if matches!(host.as_str(), "github.com" | "gitlab.com" | "codeberg.org") {
		format!("{host}/{}", path.to_ascii_lowercase()).trim_end_matches('/').to_owned()
	} else if path.is_empty() {
		host
	} else {
		format!("{host}/{path}")
	}
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::{Confidence, ProjectQuery, normalize_url, suggest_packages, suggest_projects};

	#[test]
	fn normalizes_common_repository_urls() {
		assert_eq!(normalize_url("https://www.example.com/tool/"), "example.com/tool");
		assert_eq!(normalize_url("git@github.com:Owner/Tool.git"), "github.com/owner/tool");
		assert_eq!(normalize_url("git+https://GitHub.com/Owner/Tool.git?ref=main"), "github.com/owner/tool");
		assert_eq!(normalize_url("ssh://git@github.com/Owner/Tool.git"), "github.com/owner/tool");
	}

	#[test]
	fn suggests_packages_with_explained_confidence() {
		let root = tempdir().unwrap();
		fs::write(
			root.path().join("packages.jsonl"),
			concat!(
				"{\"channel\":\"arch\",\"package\":\"ripgrep\",\"version\":\"1\",",
				"\"executable\":\"rg\",\"evidence\":\"repository-file-listing\",",
				"\"homepage\":\"https://github.com/BurntSushi/ripgrep\"}\n",
				"{\"channel\":\"crates.io\",\"package\":\"ripgrep\",\"version\":\"1\",",
				"\"executable\":\"rg\",\"evidence\":\"declared-entry-point\",",
				"\"repository\":\"git@github.com:BurntSushi/ripgrep.git\"}\n",
				"{\"channel\":\"debian\",\"package\":\"ripgrep\",\"version\":\"1\",",
				"\"executable\":\"rg\",\"evidence\":\"repository-file-listing\"}\n",
			),
		)
		.unwrap();

		let suggestions = suggest_packages(
			root.path(),
			&ProjectQuery {
				name: "ripgrep".to_owned(),
				homepage: None,
				repository: Some("https://github.com/BurntSushi/ripgrep".to_owned()),
				links: Vec::new(),
			},
			10,
		)
		.unwrap();

		assert_eq!(suggestions.len(), 3);
		assert_eq!(suggestions[0].confidence, Confidence::Exact);
		assert_eq!(suggestions[0].channel, "crates.io");
		assert_eq!(suggestions[0].evidence, ["package name", "repository URL"]);
		assert_eq!(suggestions[1].confidence, Confidence::Strong);
		assert_eq!(suggestions[1].channel, "arch");
		assert!(
			suggestions[1]
				.evidence
				.contains(&"repository URL in package homepage".to_owned())
		);
		assert_eq!(suggestions[2].confidence, Confidence::Possible);
		assert_eq!(suggestions[2].channel, "debian");
	}

	#[test]
	fn suggests_an_existing_project_for_a_package() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::create_dir_all(curations.join("projects")).unwrap();
		fs::write(
			acquisitions.join("debian.jsonl"),
			concat!(
				"{\"channel\":\"debian\",\"package\":\"ripgrep\",\"version\":\"1\",",
				"\"executable\":\"rg\",\"evidence\":\"repository-file-listing\"}\n",
			),
		)
		.unwrap();
		fs::write(
			curations.join("projects/ripgrep.toml"),
			"id = 'ripgrep'\nname = 'ripgrep'\n\n[[package]]\nchannel = 'debian'\nname = 'ripgrep'\n",
		)
		.unwrap();

		let suggestions = suggest_projects(&acquisitions, &curations, "debian", "ripgrep", 10).unwrap();
		assert_eq!(suggestions.len(), 1);
		assert_eq!(suggestions[0].confidence, Confidence::Exact);
		assert_eq!(suggestions[0].id.as_deref(), Some("ripgrep"));
		assert_eq!(suggestions[0].evidence, ["curated package attribution"]);
	}

	#[test]
	fn suggests_projects_attributed_by_a_mixed_package_curation() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::create_dir_all(curations.join("projects")).unwrap();
		fs::create_dir_all(curations.join("packages/debian")).unwrap();
		fs::write(
			acquisitions.join("debian.jsonl"),
			concat!(
				"{\"channel\":\"debian\",\"package\":\"tool-bundle\",\"version\":\"1\",",
				"\"executable\":\"bar\",\"evidence\":\"repository-file-listing\"}\n",
				"{\"channel\":\"debian\",\"package\":\"tool-bundle\",\"version\":\"1\",",
				"\"executable\":\"foo\",\"evidence\":\"repository-file-listing\"}\n",
			),
		)
		.unwrap();
		for project in ["foo", "bar"] {
			fs::write(
				curations.join(format!("projects/{project}.toml")),
				format!("id = '{project}'\nname = '{project}'\n"),
			)
			.unwrap();
		}
		fs::write(
			curations.join("packages/debian/tool-bundle.toml"),
			concat!(
				"channel = 'debian'\nname = 'tool-bundle'\n",
				"\n[[attribute]]\nexecutable = 'foo'\nproject = 'foo'\nreason = 'Bundled'\n",
				"\n[[attribute]]\nexecutable = 'bar'\nproject = 'bar'\nreason = 'Bundled'\n",
			),
		)
		.unwrap();

		let suggestions = suggest_projects(&acquisitions, &curations, "debian", "tool-bundle", 10).unwrap();
		assert_eq!(suggestions.len(), 2);
		assert!(suggestions.iter().all(|suggestion| suggestion.confidence == Confidence::Exact));
		assert_eq!(suggestions[0].id.as_deref(), Some("bar"));
		assert_eq!(suggestions[1].id.as_deref(), Some("foo"));
	}

	#[test]
	fn infers_an_uncurated_project_from_registry_metadata() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::create_dir_all(curations.join("projects")).unwrap();
		fs::write(
			acquisitions.join("crates.jsonl"),
			concat!(
				"{\"channel\":\"crates.io\",\"package\":\"helix-term\",\"version\":\"1\",",
				"\"executable\":\"hx\",\"evidence\":\"declared-entry-point\",",
				"\"repository\":\"https://github.com/helix-editor/helix\"}\n",
			),
		)
		.unwrap();

		let suggestions = suggest_projects(&acquisitions, &curations, "crates.io", "helix-term", 10).unwrap();
		assert_eq!(suggestions.len(), 1);
		assert_eq!(suggestions[0].confidence, Confidence::Strong);
		assert_eq!(suggestions[0].id, None);
		assert_eq!(suggestions[0].name, "helix");
		assert_eq!(suggestions[0].evidence, ["repository from package metadata"]);
	}
}
