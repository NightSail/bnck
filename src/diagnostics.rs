use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::acquisition::SnapshotValidationMode;
use crate::curation::{
	Acquisitions, PackageCuration, PackageKey, Project, load_candidates, load_package_curations, load_projects,
	package_owners, validate_manifest,
};
use crate::matcher::normalize_url;
use crate::model::{validate_channel, validate_executable, validate_identity};

const BASELINE_FORMAT: &str = "bnck-acquisition-baseline";
const BASELINE_FORMAT_VERSION: u32 = 1;
const REPORT_FORMAT: &str = "bnck-acquisition-diagnostics";
const REPORT_FORMAT_VERSION: u32 = 1;

pub struct Options {
	pub report: Option<PathBuf>,
	pub baseline: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningKind {
	NewExecutable,
	RemovedExecutable,
	ChangedExecutableSet,
	NewPackage,
	RemovedPackage,
	ProjectWithoutObservedPackages,
	ProjectMetadataConflict,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
	pub channel: String,
	pub package: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
	pub kind: WarningKind,
	pub message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub project: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub channel: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub package: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub executable: Option<String>,
	#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
	pub executables: BTreeSet<String>,
	#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
	pub added: BTreeSet<String>,
	#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
	pub removed: BTreeSet<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub packages: Vec<PackageRef>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub field: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub project_url: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub acquired_url: Option<String>,
}

pub struct Analysis {
	warnings: Vec<Warning>,
	report: Option<PathBuf>,
	baseline_update: Option<PathBuf>,
	baseline_created: bool,
}

impl Analysis {
	pub fn warning_count(&self) -> usize {
		self.warnings.len()
	}

	pub fn emit(&self, current: &Acquisitions) -> Result<()> {
		let warning_count = self.warning_count();
		if self.warnings.is_empty() {
			eprintln!("acquisition diagnostics: no warnings");
		} else {
			for warning in &self.warnings {
				eprintln!("warning: {}", warning.message);
			}
			eprintln!("acquisition diagnostics: {warning_count} warning(s)");
		}
		if let Some(report) = &self.report {
			write_report(report, &self.warnings, self.baseline_created)?;
		}
		if let Some(baseline) = &self.baseline_update {
			write_baseline(baseline, &BaselineState::from_acquisitions(current))?;
		}
		Ok(())
	}
}

pub fn prepare(
	acquisitions: &Path,
	curations: &Path,
	mode: SnapshotValidationMode,
	options: &Options,
) -> Result<(Analysis, Acquisitions)> {
	validate_manifest(curations)?;
	let projects = load_projects(&curations.join("projects"))?;
	let owners = package_owners(&projects)?;
	let overrides = load_package_curations(&curations.join("packages"))?;
	let acquisitions = load_candidates(acquisitions, mode)?;
	let analysis = analyze(&acquisitions, &projects, &owners, &overrides, options)?;
	Ok((analysis, acquisitions))
}

pub fn analyze(
	acquisitions: &Acquisitions,
	projects: &BTreeMap<String, Project>,
	owners: &BTreeMap<PackageKey, String>,
	overrides: &BTreeMap<PackageKey, PackageCuration>,
	options: &Options,
) -> Result<Analysis> {
	let baseline_path = options.baseline.as_deref();
	let baseline = baseline_path.map(read_baseline).transpose()?.flatten();
	let mut warnings = Vec::new();
	if let Some(baseline) = &baseline {
		compare_packages(acquisitions, baseline, owners, overrides, &mut warnings);
	}
	inspect_projects(acquisitions, projects, &mut warnings);
	warnings.sort();
	warnings.dedup();

	let baseline_created = baseline_path.is_some_and(|path| !path.exists());
	let baseline_update = baseline_path.and_then(baseline_update_path);
	Ok(Analysis { warnings, report: options.report.clone(), baseline_update, baseline_created })
}

fn compare_packages(
	current: &Acquisitions,
	baseline: &BaselineState,
	owners: &BTreeMap<PackageKey, String>,
	overrides: &BTreeMap<PackageKey, PackageCuration>,
	warnings: &mut Vec<Warning>,
) {
	let channels = current
		.complete_channels
		.intersection(&baseline.complete_channels)
		.cloned()
		.collect::<BTreeSet<_>>();
	let keys = baseline
		.packages
		.keys()
		.chain(current.packages.keys())
		.cloned()
		.collect::<BTreeSet<_>>();

	for key in keys {
		if !channels.contains(&key.0) {
			continue;
		}
		let previous = baseline.packages.get(&key).cloned().unwrap_or_default();
		let observed = current.packages.get(&key).cloned().unwrap_or_default();
		let existed = baseline.packages.contains_key(&key);
		let exists = current.packages.contains_key(&key);
		let project = owners.get(&key).cloned();
		let ordinary = project.is_some() && !overrides.contains_key(&key);

		if !existed {
			if ordinary {
				for executable in &observed {
					warnings.push(new_executable(&key, project.as_deref(), executable));
				}
			} else {
				warnings.push(new_package(&key, project.as_deref(), &observed));
			}
			continue;
		}
		if !exists {
			if ordinary {
				for executable in &previous {
					warnings.push(removed_executable(&key, project.as_deref(), executable));
				}
			} else {
				warnings.push(removed_package(&key, project.as_deref(), &previous));
			}
			continue;
		}

		let added = observed.difference(&previous).cloned().collect::<BTreeSet<_>>();
		let removed = previous.difference(&observed).cloned().collect::<BTreeSet<_>>();
		if added.is_empty() && removed.is_empty() {
			continue;
		}
		if ordinary {
			for executable in &added {
				warnings.push(new_executable(&key, project.as_deref(), executable));
			}
			for executable in &removed {
				warnings.push(removed_executable(&key, project.as_deref(), executable));
			}
		} else {
			warnings.push(changed_executable_set(&key, project.as_deref(), &added, &removed));
		}
	}
}

fn inspect_projects(acquisitions: &Acquisitions, projects: &BTreeMap<String, Project>, warnings: &mut Vec<Warning>) {
	for project in projects.values() {
		let selected_packages = project
			.packages
			.iter()
			.filter(|package| acquisitions.selected_channels.contains(&package.channel))
			.collect::<Vec<_>>();
		let observed = selected_packages.iter().any(|package| {
			acquisitions
				.packages
				.contains_key(&(package.channel.clone(), package.name.clone()))
		});
		let has_no_packages = project.packages.is_empty();
		let has_selected_package = !selected_packages.is_empty();
		if (has_no_packages && !has_selected_package) || (has_selected_package && !observed) {
			let packages = project
				.packages
				.iter()
				.map(|package| PackageRef { channel: package.channel.clone(), package: package.name.clone() })
				.collect::<BTreeSet<_>>()
				.into_iter()
				.collect();
			warnings.push(Warning {
				kind: WarningKind::ProjectWithoutObservedPackages,
				message: format!("project {} has no currently observed packages", project.id),
				project: Some(project.id.clone()),
				channel: None,
				package: None,
				executable: None,
				executables: BTreeSet::new(),
				added: BTreeSet::new(),
				removed: BTreeSet::new(),
				packages,
				field: None,
				project_url: None,
				acquired_url: None,
			});
		}

		let project_urls = project_urls(project);
		for package in &project.packages {
			if !acquisitions.selected_channels.contains(&package.channel) {
				continue;
			}
			let key = (package.channel.clone(), package.name.clone());
			let Some(hints) = acquisitions.hints.get(&key) else {
				continue;
			};
			for (field, values, declared) in [
				("homepage", &hints.homepages, project.homepage.as_deref()),
				("repository", &hints.repositories, project.repository.as_deref()),
			] {
				let Some(declared) = declared else {
					continue;
				};
				for acquired in values {
					let normalized = normalize_url(acquired);
					if normalized.is_empty() || project_urls.contains(&normalized) {
						continue;
					}
					warnings.push(Warning {
						kind: WarningKind::ProjectMetadataConflict,
						message: format!(
							"project {} package ({}, {}) reports {field} {acquired}, not project {field} {declared}",
							project.id, key.0, key.1
						),
						project: Some(project.id.clone()),
						channel: Some(key.0.clone()),
						package: Some(key.1.clone()),
						executable: None,
						executables: BTreeSet::new(),
						added: BTreeSet::new(),
						removed: BTreeSet::new(),
						packages: Vec::new(),
						field: Some(field.to_owned()),
						project_url: Some(declared.to_owned()),
						acquired_url: Some(acquired.clone()),
					});
				}
			}
		}
	}
}

fn new_executable(key: &PackageKey, project: Option<&str>, executable: &str) -> Warning {
	Warning {
		kind: WarningKind::NewExecutable,
		message: format!(
			"new executable {executable} under ordinary package ({}, {}){}",
			key.0,
			key.1,
			project_suffix(project)
		),
		project: project.map(str::to_owned),
		channel: Some(key.0.clone()),
		package: Some(key.1.clone()),
		executable: Some(executable.to_owned()),
		executables: BTreeSet::new(),
		added: BTreeSet::new(),
		removed: BTreeSet::new(),
		packages: Vec::new(),
		field: None,
		project_url: None,
		acquired_url: None,
	}
}

fn removed_executable(key: &PackageKey, project: Option<&str>, executable: &str) -> Warning {
	Warning {
		kind: WarningKind::RemovedExecutable,
		message: format!(
			"executable {executable} disappeared from ordinary package ({}, {}){}",
			key.0,
			key.1,
			project_suffix(project)
		),
		project: project.map(str::to_owned),
		channel: Some(key.0.clone()),
		package: Some(key.1.clone()),
		executable: Some(executable.to_owned()),
		executables: BTreeSet::new(),
		added: BTreeSet::new(),
		removed: BTreeSet::new(),
		packages: Vec::new(),
		field: None,
		project_url: None,
		acquired_url: None,
	}
}

fn changed_executable_set(
	key: &PackageKey,
	project: Option<&str>,
	added: &BTreeSet<String>,
	removed: &BTreeSet<String>,
) -> Warning {
	Warning {
		kind: WarningKind::ChangedExecutableSet,
		message: format!(
			"acquired executable set changed for package ({}, {}){}: added {}, removed {}",
			key.0,
			key.1,
			project_suffix(project),
			display_set(added),
			display_set(removed)
		),
		project: project.map(str::to_owned),
		channel: Some(key.0.clone()),
		package: Some(key.1.clone()),
		executable: None,
		executables: BTreeSet::new(),
		added: added.clone(),
		removed: removed.clone(),
		packages: Vec::new(),
		field: None,
		project_url: None,
		acquired_url: None,
	}
}

fn new_package(key: &PackageKey, project: Option<&str>, executables: &BTreeSet<String>) -> Warning {
	Warning {
		kind: WarningKind::NewPackage,
		message: format!(
			"new acquired package ({}, {}){}: {}",
			key.0,
			key.1,
			project_suffix(project),
			display_set(executables)
		),
		project: project.map(str::to_owned),
		channel: Some(key.0.clone()),
		package: Some(key.1.clone()),
		executable: None,
		executables: executables.clone(),
		added: BTreeSet::new(),
		removed: BTreeSet::new(),
		packages: Vec::new(),
		field: None,
		project_url: None,
		acquired_url: None,
	}
}

fn removed_package(key: &PackageKey, project: Option<&str>, executables: &BTreeSet<String>) -> Warning {
	Warning {
		kind: WarningKind::RemovedPackage,
		message: format!(
			"acquired package ({}, {}) disappeared{}: {}",
			key.0,
			key.1,
			project_suffix(project),
			display_set(executables)
		),
		project: project.map(str::to_owned),
		channel: Some(key.0.clone()),
		package: Some(key.1.clone()),
		executable: None,
		executables: executables.clone(),
		added: BTreeSet::new(),
		removed: BTreeSet::new(),
		packages: Vec::new(),
		field: None,
		project_url: None,
		acquired_url: None,
	}
}

fn project_suffix(project: Option<&str>) -> String {
	project.map(|project| format!(" for project {project}")).unwrap_or_default()
}

fn display_set(values: &BTreeSet<String>) -> String {
	if values.is_empty() {
		"none".to_owned()
	} else {
		format!("[{}]", values.iter().cloned().collect::<Vec<_>>().join(", "))
	}
}

fn project_urls(project: &Project) -> BTreeSet<String> {
	project
		.homepage
		.iter()
		.chain(project.repository.iter())
		.chain(project.documentation.iter())
		.chain(project.issue_tracker.iter())
		.chain(project.links.iter())
		.map(|url| normalize_url(url))
		.filter(|url| !url.is_empty())
		.collect()
}

#[derive(Clone, Debug, Default)]
struct BaselineState {
	packages: BTreeMap<PackageKey, BTreeSet<String>>,
	complete_channels: BTreeSet<String>,
}

impl BaselineState {
	fn from_acquisitions(acquisitions: &Acquisitions) -> Self {
		Self { packages: acquisitions.packages.clone(), complete_channels: acquisitions.complete_channels.clone() }
	}
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselineDocument {
	format: String,
	format_version: u32,
	complete_channels: Vec<String>,
	packages: Vec<BaselinePackage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BaselinePackage {
	channel: String,
	package: String,
	executables: Vec<String>,
}

impl BaselineDocument {
	fn from_state(state: &BaselineState) -> Self {
		Self {
			format: BASELINE_FORMAT.to_owned(),
			format_version: BASELINE_FORMAT_VERSION,
			complete_channels: state.complete_channels.iter().cloned().collect(),
			packages: state
				.packages
				.iter()
				.map(|((channel, package), executables)| BaselinePackage {
					channel: channel.clone(),
					package: package.clone(),
					executables: executables.iter().cloned().collect(),
				})
				.collect(),
		}
	}

	fn into_state(self) -> Result<BaselineState> {
		ensure!(self.format == BASELINE_FORMAT, "unsupported acquisition baseline format {}", self.format);
		ensure!(
			self.format_version == BASELINE_FORMAT_VERSION,
			"unsupported acquisition baseline format version {}",
			self.format_version
		);
		let mut complete_channels = BTreeSet::new();
		for channel in self.complete_channels {
			validate_channel(&channel)?;
			ensure!(complete_channels.insert(channel.clone()), "duplicate baseline complete channel {channel}");
		}
		let mut packages = BTreeMap::new();
		for package in self.packages {
			validate_channel(&package.channel)?;
			validate_identity("baseline package", &package.package)?;
			let key = (package.channel.clone(), package.package.clone());
			let mut executables = BTreeSet::new();
			for executable in package.executables {
				validate_executable(&executable)?;
				ensure!(
					executables.insert(executable.clone()),
					"baseline package ({}, {}) has duplicate executable {executable}",
					key.0,
					key.1
				);
			}
			ensure!(
				packages.insert(key.clone(), executables).is_none(),
				"baseline has duplicate package ({}, {})",
				key.0,
				key.1
			);
		}
		Ok(BaselineState { packages, complete_channels })
	}
}

fn read_baseline(path: &Path) -> Result<Option<BaselineState>> {
	if !path.exists() {
		return Ok(None);
	}
	if path.is_dir() {
		let acquisitions = load_candidates(path, SnapshotValidationMode::CandidateOnly)
			.with_context(|| format!("read baseline acquisitions {}", path.display()))?;
		return Ok(Some(BaselineState::from_acquisitions(&acquisitions)));
	}
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let document = serde_json::from_reader(BufReader::new(file))
		.with_context(|| format!("parse acquisition baseline {}", path.display()))?;
	Ok(Some(BaselineDocument::into_state(document)?))
}

fn baseline_update_path(path: &Path) -> Option<PathBuf> {
	if !path.exists() || path.is_file() { Some(path.to_owned()) } else { None }
}

fn write_baseline(path: &Path, state: &BaselineState) -> Result<()> {
	write_json(path, &BaselineDocument::from_state(state))
}

#[derive(Serialize)]
struct Report<'a> {
	format: &'static str,
	format_version: u32,
	baseline_created: bool,
	warning_count: usize,
	warnings: &'a [Warning],
}

fn write_report(path: &Path, warnings: &[Warning], baseline_created: bool) -> Result<()> {
	write_json(
		path,
		&Report {
			format: REPORT_FORMAT,
			format_version: REPORT_FORMAT_VERSION,
			baseline_created,
			warning_count: warnings.len(),
			warnings,
		},
	)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
	if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
		fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	}
	let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
	let mut writer = BufWriter::new(file);
	serde_json::to_writer_pretty(&mut writer, value)?;
	writer.write_all(b"\n")?;
	writer.flush()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;
	use std::path::{Path, PathBuf};

	use tempfile::TempDir;

	use super::{Options, WarningKind, analyze};
	use crate::acquisition::{PackageObservation, SnapshotMetadata, SnapshotStatus, write_snapshot_manifest};
	use crate::curation::{load_candidates, load_package_curations, load_projects, package_owners};

	struct Fixture {
		root: TempDir,
		acquisitions: PathBuf,
		curations: PathBuf,
	}

	impl Fixture {
		fn new(project: &str, package: &str, channel: &str) -> Self {
			let root = tempfile::tempdir().unwrap();
			let acquisitions = root.path().join("acquired");
			let curations = root.path().join("curations");
			fs::create_dir_all(&acquisitions).unwrap();
			fs::create_dir_all(curations.join("projects")).unwrap();
			fs::write(curations.join("manifest.toml"), "schema = 1\n").unwrap();
			fs::write(
				curations.join("projects").join(format!("{project}.toml")),
				format!(
					"id = '{project}'\nname = '{project}'\nhomepage = 'https://example.test/{project}'\n\n[[package]]\nchannel = '{channel}'\nname = '{package}'\n"
				),
			)
			.unwrap();
			Self { root, acquisitions, curations }
		}

		fn write_snapshot(&self, channel: &str, status: SnapshotStatus, packages: &[(&str, &[&str])]) {
			let candidate_path = self.acquisitions.join(format!("{channel}.jsonl"));
			let mut candidates = String::new();
			for (package, executables) in packages {
				for executable in *executables {
					candidates.push_str(&format!(
						"{{\"channel\":\"{channel}\",\"package\":\"{package}\",\"version\":\"1\",\
						 \"executable\":\"{executable}\",\"evidence\":\"declared-entry-point\"}}\n"
					));
				}
			}
			fs::write(&candidate_path, candidates).unwrap();
			let mut metadata =
				SnapshotMetadata::complete(channel, &[], BTreeMap::from([("test".to_owned(), "fixture".to_owned())]));
			metadata.status = status;
			let observations = packages
				.iter()
				.map(|(package, _)| PackageObservation::observed(*package, "1"))
				.collect::<Vec<_>>();
			write_snapshot_manifest(&candidate_path, metadata, observations).unwrap();
		}

		fn analyze(&self, options: &Options) -> (super::Analysis, crate::curation::Acquisitions) {
			let projects = load_projects(&self.curations.join("projects")).unwrap();
			let owners = package_owners(&projects).unwrap();
			let overrides = load_package_curations(&self.curations.join("packages")).unwrap();
			let acquisitions =
				load_candidates(&self.acquisitions, crate::acquisition::SnapshotValidationMode::Full).unwrap();
			let analysis = analyze(&acquisitions, &projects, &owners, &overrides, options).unwrap();
			(analysis, acquisitions)
		}
	}

	fn baseline_options(path: &Path) -> Options {
		Options { report: Some(path.with_file_name("report.json")), baseline: Some(path.to_owned()) }
	}

	#[test]
	fn creates_a_baseline_then_reports_new_and_removed_executables() {
		let fixture = Fixture::new("tool", "tool", "npm");
		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["tool"])]);
		let baseline = fixture.root.path().join("baseline.json");
		let (analysis, acquisitions) = fixture.analyze(&baseline_options(&baseline));
		assert_eq!(analysis.warning_count(), 0);
		analysis.emit(&acquisitions).unwrap();
		assert!(baseline.is_file());

		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["helper", "tool"])]);
		let (analysis, acquisitions) = fixture.analyze(&baseline_options(&baseline));
		assert_eq!(analysis.warning_count(), 1);
		assert_eq!(analysis.warnings[0].kind, WarningKind::NewExecutable);
		assert_eq!(analysis.warnings[0].executable.as_deref(), Some("helper"));
		analysis.emit(&acquisitions).unwrap();

		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["tool"])]);
		let (analysis, _) = fixture.analyze(&baseline_options(&baseline));
		assert_eq!(analysis.warning_count(), 1);
		assert_eq!(analysis.warnings[0].kind, WarningKind::RemovedExecutable);
		assert_eq!(analysis.warnings[0].executable.as_deref(), Some("helper"));
	}

	#[test]
	fn ignores_changes_from_partial_channels() {
		let fixture = Fixture::new("tool", "tool", "npm");
		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["tool"])]);
		let baseline = fixture.root.path().join("baseline.json");
		let (analysis, acquisitions) = fixture.analyze(&baseline_options(&baseline));
		analysis.emit(&acquisitions).unwrap();

		fixture.write_snapshot("npm", SnapshotStatus::Partial, &[("tool", &["helper", "tool"])]);
		let (analysis, _) = fixture.analyze(&baseline_options(&baseline));
		assert_eq!(analysis.warning_count(), 0);
	}

	#[test]
	fn reports_projects_without_observed_packages() {
		let fixture = Fixture::new("tool", "tool", "npm");
		let root = fixture.root.path();
		fs::create_dir_all(root.join("curations/packages")).unwrap();
		fs::write(
			root.join("curations/projects/tool.toml"),
			"id = 'tool'\nname = 'tool'\n\n[[package]]\nchannel = 'npm'\nname = 'absent'\n",
		)
		.unwrap();
		fixture.write_snapshot("npm", SnapshotStatus::Partial, &[]);
		let (analysis, _) = fixture.analyze(&Options { report: None, baseline: None });
		assert_eq!(analysis.warning_count(), 1);
		assert_eq!(analysis.warnings[0].kind, WarningKind::ProjectWithoutObservedPackages);
	}

	#[test]
	fn reports_metadata_conflicts_without_treating_project_links_as_conflicts() {
		let fixture = Fixture::new("tool", "tool", "npm");
		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["tool"])]);
		let candidate = fixture.acquisitions.join("npm.jsonl");
		fs::write(
			&candidate,
			"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1\",\"executable\":\"tool\",\
			 \"evidence\":\"declared-entry-point\",\"homepage\":\"https://elsewhere.test/tool\"}\n",
		)
		.unwrap();
		write_snapshot_manifest(
			&candidate,
			SnapshotMetadata::complete("npm", &[], BTreeMap::from([("test".to_owned(), "fixture".to_owned())])),
			[PackageObservation::observed("tool", "1")],
		)
		.unwrap();
		let (analysis, _) = fixture.analyze(&Options { report: None, baseline: None });
		assert_eq!(analysis.warning_count(), 1);
		assert_eq!(analysis.warnings[0].kind, WarningKind::ProjectMetadataConflict);
		assert_eq!(analysis.warnings[0].field.as_deref(), Some("homepage"));

		fs::write(
			fixture.curations.join("projects/tool.toml"),
			"id = 'tool'\nname = 'tool'\nhomepage = 'https://example.test/tool'\nlinks = ['https://elsewhere.test/tool']\n\n[[package]]\nchannel = 'npm'\nname = 'tool'\n",
		)
		.unwrap();
		let (analysis, _) = fixture.analyze(&Options { report: None, baseline: None });
		assert_eq!(analysis.warning_count(), 0);
	}

	#[test]
	fn reads_a_baseline_directory_using_candidate_only_validation() {
		let fixture = Fixture::new("tool", "tool", "npm");
		fixture.write_snapshot("npm", SnapshotStatus::Complete, &[("tool", &["tool"])]);
		let baseline = fixture.root.path().join("previous");
		fs::create_dir(&baseline).unwrap();
		fs::copy(fixture.acquisitions.join("npm.jsonl"), baseline.join("npm.jsonl")).unwrap();
		fs::copy(fixture.acquisitions.join("npm.jsonl.manifest.json"), baseline.join("npm.jsonl.manifest.json"))
			.unwrap();
		let state = super::read_baseline(&baseline).unwrap().unwrap();
		assert_eq!(state.packages[&("npm".to_owned(), "tool".to_owned())].len(), 1);
	}
}
