use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::acquisition::{PackageStatus, SnapshotStatus, SnapshotValidationMode, read_snapshot_directory_with_mode};
use crate::model::{
	Provider, read_candidates, validate_channel, validate_executable, validate_identity, validate_project_id,
	write_json_lines,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
	pub(crate) id: String,
	pub(crate) name: String,
	#[serde(default)]
	pub(crate) homepage: Option<String>,
	#[serde(default)]
	pub(crate) repository: Option<String>,
	#[serde(default)]
	pub(crate) documentation: Option<String>,
	#[serde(default)]
	pub(crate) issue_tracker: Option<String>,
	#[serde(default)]
	pub(crate) links: Vec<String>,
	#[serde(default, rename = "package")]
	pub(crate) packages: Vec<ProjectPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectPackage {
	pub(crate) channel: String,
	pub(crate) name: String,
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
type AcquiredPackages = BTreeMap<PackageKey, BTreeSet<String>>;

struct Acquisitions {
	packages: AcquiredPackages,
	selected_channels: BTreeSet<String>,
	complete_channels: BTreeSet<String>,
}

pub fn generate(acquisitions: &Path, curations: &Path, output: &Path, mode: SnapshotValidationMode) -> Result<usize> {
	validate_manifest(curations)?;
	let projects = load_projects(&curations.join("projects"))?;
	let owners = package_owners(&projects)?;
	let overrides = load_package_curations(&curations.join("packages"))?;
	validate_project_references(&overrides, &projects)?;
	let acquisitions = load_candidates(acquisitions, mode)?;
	let package_keys = owners.keys().chain(overrides.keys()).cloned().collect::<BTreeSet<_>>();
	let mut providers = BTreeSet::new();

	for key in package_keys {
		if !acquisitions.selected_channels.contains(&key.0) {
			continue;
		}
		let Some(acquired) = acquisitions.packages.get(&key) else {
			ensure!(
				!acquisitions.complete_channels.contains(&key.0),
				"curated package ({}, {}) is absent from complete acquisition",
				key.0,
				key.1
			);
			continue;
		};
		providers.extend(resolve_package(&key, owners.get(&key).map(String::as_str), overrides.get(&key), acquired)?);
	}

	write_json_lines(output, providers)
}

pub(crate) fn curated_packages(root: &Path, channel: &str) -> Result<BTreeSet<String>> {
	validate_manifest(root)?;
	let projects = load_projects(&root.join("projects"))?;
	let owners = package_owners(&projects)?;
	let exceptions = load_package_curations(&root.join("packages"))?;
	validate_project_references(&exceptions, &projects)?;
	Ok(owners
		.keys()
		.chain(exceptions.keys())
		.filter(|key| key.0 == channel)
		.map(|key| key.1.clone())
		.collect())
}

fn validate_manifest(root: &Path) -> Result<()> {
	#[derive(Deserialize)]
	#[serde(deny_unknown_fields)]
	struct Manifest {
		schema: u64,
	}

	let path = root.join("manifest.toml");
	let manifest: Manifest = read_toml(&path)?;
	ensure!(manifest.schema == 0, "unsupported curation schema {}", manifest.schema);
	Ok(())
}

pub(crate) fn load_projects(path: &Path) -> Result<BTreeMap<String, Project>> {
	let mut projects = BTreeMap::new();
	for path in toml_files(path)? {
		let project: Project = read_toml(&path)?;
		validate_project_id(&project.id)?;
		validate_identity("project name", &project.name)?;
		for link in project
			.homepage
			.iter()
			.chain(project.repository.iter())
			.chain(project.documentation.iter())
			.chain(project.issue_tracker.iter())
			.chain(project.links.iter())
		{
			validate_identity("project link", link)?;
		}
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
			validate_channel(&package.channel)?;
			validate_identity("package", &package.name)?;
			let key = (package.channel.clone(), package.name.clone());
			if let Some(owner) = owners.insert(key.clone(), project.id.clone()) {
				bail!("package ({}, {}) is claimed by projects {owner} and {}", key.0, key.1, project.id)
			}
		}
	}
	Ok(owners)
}

pub(crate) fn curated_package_projects(root: &Path, channel: &str, name: &str) -> Result<BTreeSet<String>> {
	let projects = load_projects(&root.join("projects"))?;
	let owners = package_owners(&projects)?;
	let packages = load_package_curations(&root.join("packages"))?;
	validate_project_references(&packages, &projects)?;
	let key = (channel.to_owned(), name.to_owned());
	let mut project_ids = owners.get(&key).cloned().into_iter().collect::<BTreeSet<_>>();
	if let Some(package) = packages.get(&key) {
		project_ids.extend(package.attribute.iter().map(|rule| rule.project.clone()));
		project_ids.extend(package.replace.iter().map(|rule| rule.project.clone()));
		project_ids.extend(package.add.iter().map(|rule| rule.project.clone()));
	}
	Ok(project_ids)
}

fn load_package_curations(packages_root: &Path) -> Result<BTreeMap<PackageKey, PackageCuration>> {
	let mut packages = BTreeMap::new();
	if !packages_root.exists() {
		return Ok(packages);
	}
	for path in toml_files(packages_root)? {
		let package: PackageCuration = read_toml(&path)?;
		validate_channel(&package.channel)?;
		validate_identity("package", &package.name)?;
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

fn load_candidates(path: &Path, mode: SnapshotValidationMode) -> Result<Acquisitions> {
	let snapshots = read_snapshot_directory_with_mode(path, mode)?;
	ensure!(!snapshots.is_empty(), "{} contains no acquisition snapshot manifests", path.display());
	let mut packages = AcquiredPackages::new();
	let mut selected_channels = BTreeSet::new();
	let mut candidate_paths = BTreeSet::new();
	let mut channel_statuses = BTreeMap::<String, Vec<SnapshotStatus>>::new();

	for (manifest_path, snapshot) in snapshots {
		selected_channels.insert(snapshot.channel.clone());
		channel_statuses
			.entry(snapshot.channel.clone())
			.or_default()
			.push(snapshot.status);
		for package in &snapshot.packages {
			if package.status == PackageStatus::Observed {
				packages.entry((snapshot.channel.clone(), package.name.clone())).or_default();
			}
		}
		let candidate_path = manifest_path.parent().unwrap_or(Path::new(".")).join(&snapshot.candidates.path);
		ensure!(
			candidate_paths.insert(candidate_path.clone()),
			"{} is referenced by multiple snapshot manifests",
			candidate_path.display()
		);
		for candidate in read_candidates(&candidate_path)? {
			let key = (candidate.channel, candidate.package);
			ensure!(
				packages.contains_key(&key),
				"candidate package ({}, {}) is not observed by its snapshot",
				key.0,
				key.1
			);
			packages.entry(key).or_default().insert(candidate.executable);
		}
	}
	for entry in WalkDir::new(path).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", path.display()))?;
		if entry.file_type().is_file() && entry.path().extension().is_some_and(|value| value == "jsonl") {
			ensure!(
				candidate_paths.contains(entry.path()),
				"{} has no acquisition snapshot manifest",
				entry.path().display()
			);
		}
	}
	let complete_channels = channel_statuses
		.into_iter()
		.filter(|(_, statuses)| statuses.iter().all(|status| *status == SnapshotStatus::Complete))
		.map(|(channel, _)| channel)
		.collect();
	Ok(Acquisitions { packages, selected_channels, complete_channels })
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
	acquired: &BTreeSet<String>,
) -> Result<BTreeSet<Provider>> {
	let mut assignments = BTreeMap::<String, String>::new();
	let mut removed = BTreeSet::new();
	if let Some(rules) = override_rules {
		validate_rules(rules, acquired)?;
		apply_rules(rules, &mut assignments, &mut removed)?;
	}

	for executable in acquired.iter().filter(|executable| !removed.contains(*executable)) {
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

fn validate_rules(package: &PackageCuration, acquired: &BTreeSet<String>) -> Result<()> {
	let mut acquired_rules = BTreeSet::new();
	for rule in &package.attribute {
		validate_rule(&rule.executable, &rule.reason, acquired, &mut acquired_rules, &package.name)?;
		validate_references(&rule.references, &package.name)?;
	}
	for rule in &package.exclude {
		validate_rule(&rule.executable, &rule.reason, acquired, &mut acquired_rules, &package.name)?;
		validate_references(&rule.references, &package.name)?;
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
		validate_references(&rule.references, &package.name)?;
	}
	for rule in &package.add {
		validate_executable(&rule.executable)?;
		ensure!(!rule.reason.trim().is_empty(), "package {} has an addition without a reason", package.name);
		ensure!(!rule.references.is_empty(), "package {} addition {} has no reference", package.name, rule.executable);
		validate_references(&rule.references, &package.name)?;
		ensure!(
			!acquired.contains(&rule.executable),
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

fn validate_references(references: &[String], package: &str) -> Result<()> {
	for reference in references {
		validate_identity("rule reference", reference)
			.with_context(|| format!("validate reference for package {package}"))?;
	}
	Ok(())
}

fn validate_rule(
	executable: &str,
	reason: &str,
	acquired: &BTreeSet<String>,
	seen: &mut BTreeSet<String>,
	package: &str,
) -> Result<()> {
	validate_executable(executable)?;
	ensure!(!reason.trim().is_empty(), "package {package} has a rule without a reason");
	ensure!(acquired.contains(executable), "package {package} rule refers to absent executable {executable}");
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
	use std::collections::BTreeMap;
	use std::fs;

	use tempfile::tempdir;

	use super::generate;
	use crate::acquisition::{PackageObservation, SnapshotMetadata, SnapshotValidationMode, write_snapshot_manifest};

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
			concat!(
				"id = 'tool'\nname = 'Tool'\nlinks = ['https://example.test/tool']\n",
				"\n[[package]]\nchannel = 'npm'\nname = 'tool'\n",
			),
		)
		.unwrap();
		fs::write(
			packages.join("tool.toml"),
			"channel = 'npm'\nname = 'tool'\n\n[[exclude]]\nexecutable = 'helper'\nreason = 'Not public'\n",
		)
		.unwrap();
		let candidate_path = acquisitions.join("npm.jsonl");
		fs::write(
			&candidate_path,
			concat!(
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"helper\",\"evidence\":\"declared-entry-point\"}\n",
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"tool\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		write_snapshot_manifest(
			&candidate_path,
			SnapshotMetadata::complete("npm", &[], BTreeMap::from([("registry".to_owned(), "test".to_owned())])),
			[PackageObservation::observed("tool", "1.0.0")],
		)
		.unwrap();
		let output = root.path().join("providers.jsonl");

		assert_eq!(generate(&acquisitions, &curations, &output, SnapshotValidationMode::Full).unwrap(), 1);
		assert_eq!(
			fs::read_to_string(output).unwrap(),
			"{\"project\":\"tool\",\"executable\":\"tool\",\"channel\":\"npm\",\"package\":\"tool\"}\n"
		);
	}

	#[test]
	fn distinguishes_observed_package_without_executables_from_missing_package() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = root.path().join("curations");
		fs::create_dir_all(&acquisitions).unwrap();
		fs::create_dir_all(curations.join("projects")).unwrap();
		fs::write(curations.join("manifest.toml"), "schema = 0\n").unwrap();
		fs::write(
			curations.join("projects/tool.toml"),
			"id = 'tool'\nname = 'Tool'\n\n[[package]]\nchannel = 'npm'\nname = 'tool'\n",
		)
		.unwrap();
		let candidate_path = acquisitions.join("npm.jsonl");
		fs::write(&candidate_path, "").unwrap();
		write_snapshot_manifest(
			&candidate_path,
			SnapshotMetadata::complete("npm", &[], BTreeMap::from([("registry".to_owned(), "test".to_owned())])),
			[PackageObservation::observed("tool", "1.0.0")],
		)
		.unwrap();
		let output = root.path().join("providers.jsonl");

		assert_eq!(generate(&acquisitions, &curations, &output, SnapshotValidationMode::Full).unwrap(), 0);
		assert_eq!(fs::read_to_string(output).unwrap(), "");
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
