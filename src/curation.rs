use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::model::{Candidate, Provider, read_candidates, write_json_lines};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
	id: String,
	name: String,
	#[serde(default)]
	homepage: Option<String>,
	#[serde(default)]
	repository: Option<String>,
	#[serde(default)]
	documentation: Option<String>,
	#[serde(default)]
	issue_tracker: Option<String>,
	#[serde(default, rename = "package")]
	packages: Vec<ProjectPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPackage {
	channel: String,
	name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageCuration {
	channel: String,
	name: String,
	#[serde(default)]
	attribute: Vec<Attribution>,
	#[serde(default)]
	exclude: Vec<Exclusion>,
	#[serde(default)]
	replace: Vec<Replacement>,
	#[serde(default)]
	add: Vec<Addition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attribution {
	executable: String,
	project: String,
	reason: String,
	#[serde(default)]
	references: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exclusion {
	executable: String,
	reason: String,
	#[serde(default)]
	references: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Replacement {
	acquired: String,
	executable: String,
	project: String,
	reason: String,
	#[serde(default)]
	references: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Addition {
	executable: String,
	project: String,
	reason: String,
	references: Vec<String>,
}

type PackageKey = (String, String);
type AcquiredPackages = BTreeMap<PackageKey, BTreeMap<String, Candidate>>;

struct Acquisitions {
	packages: AcquiredPackages,
	channels: BTreeSet<String>,
}

pub fn generate(acquisitions: &Path, curations: &Path, output: &Path) -> Result<usize> {
	validate_manifest(curations)?;
	let projects = load_projects(&curations.join("projects"))?;
	let owners = package_owners(&projects)?;
	let overrides = load_package_curations(&curations.join("packages"))?;
	validate_project_references(&overrides, &projects)?;
	let acquisitions = load_candidates(acquisitions)?;
	let package_keys = owners.keys().chain(overrides.keys()).cloned().collect::<BTreeSet<_>>();
	let mut providers = BTreeSet::new();

	for key in package_keys {
		if !acquisitions.channels.contains(&key.0) {
			continue;
		}
		let acquired = acquisitions
			.packages
			.get(&key)
			.with_context(|| format!("curated package ({}, {}) is absent from acquisition", key.0, key.1))?;
		providers.extend(resolve_package(&key, owners.get(&key).map(String::as_str), overrides.get(&key), acquired)?);
	}

	write_json_lines(output, providers)
}

fn validate_manifest(root: &Path) -> Result<()> {
	#[derive(Deserialize)]
	struct Manifest {
		schema: u64,
	}

	let path = root.join("manifest.toml");
	let manifest: Manifest = read_toml(&path)?;
	ensure!(manifest.schema == 0, "unsupported curation schema {}", manifest.schema);
	Ok(())
}

fn load_projects(path: &Path) -> Result<BTreeMap<String, Project>> {
	let mut projects = BTreeMap::new();
	for path in toml_files(path)? {
		let project: Project = read_toml(&path)?;
		validate_identifier("project ID", &project.id)?;
		ensure!(!project.name.trim().is_empty(), "{} has an empty project name", path.display());
		let _ = (&project.homepage, &project.repository, &project.documentation, &project.issue_tracker);
		let id = project.id.clone();
		if projects.insert(id.clone(), project).is_some() {
			bail!("duplicate project ID {id}")
		}
	}
	Ok(projects)
}

fn package_owners(projects: &BTreeMap<String, Project>) -> Result<BTreeMap<PackageKey, String>> {
	let mut owners = BTreeMap::new();
	for project in projects.values() {
		for package in &project.packages {
			validate_identifier("channel", &package.channel)?;
			validate_identifier("package", &package.name)?;
			let key = (package.channel.clone(), package.name.clone());
			if let Some(owner) = owners.insert(key.clone(), project.id.clone()) {
				bail!("package ({}, {}) is claimed by projects {owner} and {}", key.0, key.1, project.id)
			}
		}
	}
	Ok(owners)
}

fn load_package_curations(packages_root: &Path) -> Result<BTreeMap<PackageKey, PackageCuration>> {
	let mut packages = BTreeMap::new();
	if !packages_root.exists() {
		return Ok(packages);
	}
	for path in toml_files(packages_root)? {
		let package: PackageCuration = read_toml(&path)?;
		validate_identifier("channel", &package.channel)?;
		validate_identifier("package", &package.name)?;
		validate_channel_directory(&path, &package.channel, packages_root)?;
		let identity = (package.channel.clone(), package.name.clone());
		if packages.insert(identity.clone(), package).is_some() {
			bail!("duplicate package curation ({}, {})", identity.0, identity.1)
		}
	}
	Ok(packages)
}

fn validate_channel_directory(path: &Path, channel: &str, packages_root: &Path) -> Result<()> {
	let relative = path.strip_prefix(packages_root).unwrap_or(path);
	let directory = relative.components().next().and_then(|value| value.as_os_str().to_str());
	ensure!(directory == Some(channel), "{} is not under its channel directory {channel}", path.display());
	Ok(())
}

fn load_candidates(path: &Path) -> Result<Acquisitions> {
	let mut packages = AcquiredPackages::new();
	let mut channels = BTreeSet::new();
	for entry in WalkDir::new(path).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", path.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "jsonl") {
			continue;
		}
		for candidate in read_candidates(entry.path())? {
			channels.insert(candidate.channel.clone());
			let key = (candidate.channel.clone(), candidate.package.clone());
			packages
				.entry(key)
				.or_default()
				.entry(candidate.executable.clone())
				.or_insert(candidate);
		}
	}
	Ok(Acquisitions { packages, channels })
}

fn validate_project_references(
	packages: &BTreeMap<PackageKey, PackageCuration>,
	projects: &BTreeMap<String, Project>,
) -> Result<()> {
	for package in packages.values() {
		let references = package
			.attribute
			.iter()
			.map(|rule| &rule.project)
			.chain(package.replace.iter().map(|rule| &rule.project))
			.chain(package.add.iter().map(|rule| &rule.project));
		for project in references {
			ensure!(projects.contains_key(project), "package {} references unknown project {project}", package.name);
		}
	}
	Ok(())
}

fn resolve_package(
	key: &PackageKey,
	default_project: Option<&str>,
	override_rules: Option<&PackageCuration>,
	acquired: &BTreeMap<String, Candidate>,
) -> Result<BTreeSet<Provider>> {
	let mut assignments = BTreeMap::<String, String>::new();
	let mut removed = BTreeSet::new();
	if let Some(rules) = override_rules {
		validate_rules(rules, acquired)?;
		apply_rules(rules, &mut assignments, &mut removed)?;
	}

	for executable in acquired.keys().filter(|executable| !removed.contains(*executable)) {
		if assignments.contains_key(executable) {
			continue;
		}
		let project = default_project
			.with_context(|| format!("mixed package {} does not attribute executable {executable}", key.1))?;
		insert_assignment(&mut assignments, executable, project, &key.1)?;
	}
	if let Some(rules) = override_rules {
		for rule in &rules.add {
			insert_assignment(&mut assignments, &rule.executable, &rule.project, &key.1)?;
		}
	}

	Ok(assignments
		.into_iter()
		.map(|(executable, project)| Provider { project, executable, channel: key.0.clone(), package: key.1.clone() })
		.collect())
}

fn apply_rules(
	rules: &PackageCuration,
	assignments: &mut BTreeMap<String, String>,
	removed: &mut BTreeSet<String>,
) -> Result<()> {
	for rule in &rules.exclude {
		removed.insert(rule.executable.clone());
	}
	for rule in &rules.replace {
		removed.insert(rule.acquired.clone());
		insert_assignment(assignments, &rule.executable, &rule.project, &rules.name)?;
	}
	for rule in &rules.attribute {
		insert_assignment(assignments, &rule.executable, &rule.project, &rules.name)?;
	}
	Ok(())
}

fn validate_rules(package: &PackageCuration, acquired: &BTreeMap<String, Candidate>) -> Result<()> {
	let mut acquired_rules = BTreeSet::new();
	for rule in &package.attribute {
		validate_rule(&rule.executable, &rule.reason, acquired, &mut acquired_rules, &package.name)?;
		let _ = &rule.references;
	}
	for rule in &package.exclude {
		validate_rule(&rule.executable, &rule.reason, acquired, &mut acquired_rules, &package.name)?;
		let _ = &rule.references;
	}
	let mut produced = BTreeSet::new();
	for rule in &package.replace {
		validate_rule(&rule.acquired, &rule.reason, acquired, &mut acquired_rules, &package.name)?;
		validate_executable(&rule.executable)?;
		ensure!(
			produced.insert(rule.executable.clone()),
			"package {} produces {} more than once",
			package.name,
			rule.executable
		);
		let _ = &rule.references;
	}
	for rule in &package.add {
		validate_executable(&rule.executable)?;
		ensure!(!rule.reason.trim().is_empty(), "package {} has an addition without a reason", package.name);
		ensure!(!rule.references.is_empty(), "package {} addition {} has no reference", package.name, rule.executable);
		ensure!(
			!acquired.contains_key(&rule.executable),
			"package {} adds already acquired executable {}",
			package.name,
			rule.executable
		);
		ensure!(
			produced.insert(rule.executable.clone()),
			"package {} produces {} more than once",
			package.name,
			rule.executable
		);
	}
	Ok(())
}

fn validate_rule(
	executable: &str,
	reason: &str,
	acquired: &BTreeMap<String, Candidate>,
	seen: &mut BTreeSet<String>,
	package: &str,
) -> Result<()> {
	validate_executable(executable)?;
	ensure!(!reason.trim().is_empty(), "package {package} has a rule without a reason");
	ensure!(acquired.contains_key(executable), "package {package} rule refers to absent executable {executable}");
	ensure!(seen.insert(executable.to_owned()), "package {package} has contradictory rules for {executable}");
	Ok(())
}

fn insert_assignment(
	assignments: &mut BTreeMap<String, String>,
	executable: &str,
	project: &str,
	package: &str,
) -> Result<()> {
	validate_executable(executable)?;
	if assignments.insert(executable.to_owned(), project.to_owned()).is_some() {
		bail!("package {package} attributes executable {executable} more than once")
	}
	Ok(())
}

fn validate_identifier(kind: &str, value: &str) -> Result<()> {
	ensure!(!value.trim().is_empty(), "{kind} is empty");
	ensure!(!value.contains('\0'), "{kind} contains NUL");
	Ok(())
}

fn validate_executable(value: &str) -> Result<()> {
	validate_identifier("executable", value)?;
	ensure!(!value.contains('/'), "executable {value} contains /");
	Ok(())
}

fn toml_files(path: &Path) -> Result<Vec<PathBuf>> {
	let mut paths = Vec::new();
	for entry in WalkDir::new(path).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", path.display()))?;
		if entry.file_type().is_file() && entry.path().extension().is_some_and(|value| value == "toml") {
			paths.push(entry.into_path());
		}
	}
	paths.sort();
	Ok(paths)
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
	let value = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
	toml::from_str(&value).with_context(|| format!("parse {}", path.display()))
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::generate;

	#[test]
	fn resolves_project_package_and_exception() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		let projects = curations.join("projects");
		let packages = curations.join("packages/npm");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::create_dir_all(&projects).unwrap();
		fs::create_dir_all(&packages).unwrap();
		fs::write(curations.join("manifest.toml"), "schema = 0\n").unwrap();
		fs::write(
			projects.join("tool.toml"),
			"id = 'tool'\nname = 'Tool'\n\n[[package]]\nchannel = 'npm'\nname = 'tool'\n",
		)
		.unwrap();
		fs::write(
			packages.join("tool.toml"),
			"channel = 'npm'\nname = 'tool'\n\n[[exclude]]\nexecutable = 'helper'\nreason = 'Not public'\n",
		)
		.unwrap();
		fs::write(
			acquisitions.join("npm.jsonl"),
			concat!(
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"helper\",\"evidence\":\"declared-entry-point\"}\n",
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"tool\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		let output = root.path().join("providers.jsonl");

		assert_eq!(generate(&acquisitions, &curations, &output).unwrap(), 1);
		assert_eq!(
			fs::read_to_string(output).unwrap(),
			"{\"project\":\"tool\",\"executable\":\"tool\",\"channel\":\"npm\",\"package\":\"tool\"}\n"
		);
	}

	#[test]
	fn rejects_package_claimed_by_two_projects() {
		let root = tempdir().unwrap();
		let projects = root.path().join("projects");
		fs::create_dir_all(&projects).unwrap();
		for id in ["one", "two"] {
			fs::write(
				projects.join(format!("{id}.toml")),
				format!("id = '{id}'\nname = '{id}'\n\n[[package]]\nchannel = 'npm'\nname = 'tool'\n"),
			)
			.unwrap();
		}

		let projects = super::load_projects(&projects).unwrap();
		let error = super::package_owners(&projects).unwrap_err();
		assert!(error.to_string().contains("claimed by projects"));
	}
}
