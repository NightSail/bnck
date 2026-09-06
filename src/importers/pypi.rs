mod selection;

#[cfg(test)]
#[path = "pypi/tests.rs"]
mod snapshot_tests;

pub use selection::{plan, plan_snapshot};

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use pep440_rs::Version;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::acquisition::{
	PackageObservation, PackageStatus, SnapshotMetadata, SnapshotStatus, file_identity, write_acquisition,
};
use crate::model::{Candidate, validate_sha256};
use crate::registry::{json_files, normalize_pypi_name, read_json};
use selection::{Selection, WheelPlan};

const MAX_METADATA_SIZE: u64 = 4 * 1024 * 1024;

pub fn import(input: &Path, output: &Path, mut scope: BTreeMap<String, String>) -> Result<usize> {
	scope.insert("package-selection".to_owned(), "explicit-packages".to_owned());
	if let Some(simple) = input.parent().map(|parent| parent.join("simple")).filter(|path| path.is_dir()) {
		let inputs = json_files(&simple)?;
		let packages = inputs
			.iter()
			.map(|path| selection::select(path, input))
			.collect::<Result<Vec<_>>>()?;
		return import_selections(WheelPlan { inputs, packages }, output, scope);
	}
	let mut candidates = Vec::new();
	let mut package_observations = Vec::new();
	for entry in WalkDir::new(input).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", input.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "whl") {
			continue;
		}
		let (wheel_candidates, package_observation) = import_wheel(entry.path())?;
		candidates.extend(wheel_candidates);
		package_observations.push(package_observation);
	}
	let inputs = [input.to_owned()];
	let mut metadata = SnapshotMetadata::complete("pypi", &inputs, scope);
	metadata.status = SnapshotStatus::Partial;
	write_acquisition(output, metadata, package_observations, candidates)
}

pub fn import_plan(path: &Path, output: &Path, mut scope: BTreeMap<String, String>) -> Result<usize> {
	let mut plan: WheelPlan = read_json(path)?;
	plan.inputs.push(path.to_owned());
	scope.insert("package-selection".to_owned(), "curation-discovery".to_owned());
	import_selections(plan, output, scope)
}

fn import_selections(mut plan: WheelPlan, output: &Path, scope: BTreeMap<String, String>) -> Result<usize> {
	let mut candidates = Vec::new();
	let mut packages = Vec::new();
	let mut names = BTreeSet::new();
	for selected in plan.packages {
		match selected {
			Selection::Wheel { package, version, sha256, path, filename, .. } => {
				validate_sha256("planned wheel", &sha256)?;
				ensure!(file_identity(&path)?.1 == sha256, "wheel SHA-256 mismatch: {}", path.display());
				let (mut observed, mut observation) = import_wheel(&path)?;
				ensure!(observation.name == package, "expected PyPI package {package}, found {}", observation.name);
				ensure!(
					Version::from_str(observation.version.as_deref().context("wheel has no version")?)?
						== Version::from_str(&version)?,
					"wheel version does not match selected release {package} {version}"
				);
				observation.variant.insert("wheel".to_owned(), filename);
				for candidate in &mut observed {
					candidate.variant = observation.variant.clone();
				}
				candidates.extend(observed);
				packages.push(observation);
				plan.inputs.push(path);
			},
			Selection::Unsupported { package, version, reason } => {
				packages.push(PackageObservation {
					name: package,
					version,
					artifact_sha256: None,
					status: PackageStatus::Failed,
					variant: BTreeMap::from([("reason".to_owned(), reason)]),
				});
			},
		}
		let package = packages.last().context("missing package observation")?;
		ensure!(names.insert(package.name.clone()), "duplicate selected PyPI package {}", package.name);
	}
	let mut metadata = SnapshotMetadata::complete("pypi", &plan.inputs, scope);
	if metadata
		.scope
		.get("package-selection")
		.is_some_and(|selection| selection == "explicit-packages")
		|| packages.iter().any(|package| package.status == PackageStatus::Failed)
	{
		metadata.status = SnapshotStatus::Partial;
	}
	write_acquisition(output, metadata, packages, candidates)
}

fn import_wheel(path: &Path) -> Result<(Vec<Candidate>, PackageObservation)> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut archive = ZipArchive::new(file).with_context(|| format!("open wheel {}", path.display()))?;
	let metadata_name = member_name(&mut archive, ".dist-info/METADATA")?
		.with_context(|| format!("{} has no METADATA", path.display()))?;
	let metadata = read_member(&mut archive, &metadata_name)?;
	let fields = parse_metadata(&metadata);
	let package = normalize_pypi_name(&field(&fields, "Name")?)?;
	let version = field(&fields, "Version")?;
	let homepage = first_field(&fields, "Home-page").or_else(|| project_url(&fields, &["homepage"]));
	let repository = project_url(&fields, &["source", "source code", "repository"]);
	let description = first_field(&fields, "Summary");
	let mut executable_evidence = Vec::new();

	let entry_points_name = metadata_name.replace("/METADATA", "/entry_points.txt");
	if archive.file_names().any(|name| name == entry_points_name) {
		let entry_points = read_member(&mut archive, &entry_points_name)?;
		for executable in parse_entry_points(&entry_points) {
			executable_evidence.push((executable, "declared-entry-point"));
		}
	}

	let scripts_prefix = metadata_name.replace(".dist-info/METADATA", ".data/scripts/");
	for index in 0..archive.len() {
		let member = archive.by_index(index)?;
		if !member.is_dir()
			&& let Some(executable) = member.name().strip_prefix(&scripts_prefix)
			&& !executable.is_empty()
			&& !executable.contains('/')
		{
			executable_evidence.push((executable.to_owned(), "artifact-metadata"));
		}
	}

	let artifact_sha256 = file_identity(path)?.1;
	let candidates = executable_evidence
		.into_iter()
		.map(|(executable, evidence)| Candidate {
			channel: "pypi".into(),
			package: package.clone(),
			version: version.clone(),
			variant: std::collections::BTreeMap::new(),
			artifact_sha256: Some(artifact_sha256.clone()),
			executable,
			evidence: evidence.into(),
			source_package: None,
			homepage: homepage.clone(),
			repository: repository.clone(),
			description: description.clone(),
		})
		.collect();
	let mut package_observation = PackageObservation::observed(package, version);
	package_observation.artifact_sha256 = Some(artifact_sha256);
	Ok((candidates, package_observation))
}

fn member_name(archive: &mut ZipArchive<File>, suffix: &str) -> Result<Option<String>> {
	let mut selected = None;
	for index in 0..archive.len() {
		let member = archive.by_index(index)?;
		if member.name().strip_suffix(suffix).is_some_and(|prefix| !prefix.contains('/')) {
			ensure!(selected.is_none(), "wheel contains multiple {suffix} members");
			selected = Some(member.name().to_owned());
		}
	}
	Ok(selected)
}

fn read_member(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
	let member = archive.by_name(name)?;
	if member.size() > MAX_METADATA_SIZE {
		bail!("wheel metadata member {name} exceeds size limit")
	}
	let mut value = String::new();
	member.take(MAX_METADATA_SIZE + 1).read_to_string(&mut value)?;
	ensure!(value.len() as u64 <= MAX_METADATA_SIZE, "wheel metadata member {name} exceeds size limit");
	Ok(value)
}

fn parse_metadata(value: &str) -> HashMap<String, Vec<String>> {
	let mut fields = HashMap::<String, Vec<String>>::new();
	let mut current = None::<(String, usize)>;
	for line in value.lines() {
		if line.is_empty() {
			break;
		}
		if line.starts_with([' ', '\t']) {
			if let Some((field, index)) = current.as_ref()
				&& let Some(value) = fields.get_mut(field).and_then(|values| values.get_mut(*index))
			{
				value.push_str(line.trim());
			}
			continue;
		}
		let Some((name, value)) = line.split_once(':') else {
			continue;
		};
		let values = fields.entry(name.to_owned()).or_default();
		values.push(value.trim().to_owned());
		current = Some((name.to_owned(), values.len() - 1));
	}
	fields
}

fn field(fields: &HashMap<String, Vec<String>>, name: &str) -> Result<String> {
	first_field(fields, name).with_context(|| format!("METADATA has no {name}"))
}

fn first_field(fields: &HashMap<String, Vec<String>>, name: &str) -> Option<String> {
	fields.get(name)?.first().filter(|value| !value.is_empty()).cloned()
}

fn project_url(fields: &HashMap<String, Vec<String>>, labels: &[&str]) -> Option<String> {
	for value in fields.get("Project-URL")? {
		let Some((label, url)) = value.split_once(',') else {
			continue;
		};
		if labels.contains(&label.trim().to_ascii_lowercase().as_str()) {
			return Some(url.trim().to_owned());
		}
	}
	None
}

fn parse_entry_points(value: &str) -> Vec<String> {
	let mut is_script_group = false;
	let mut names = Vec::new();
	for line in value.lines().map(str::trim) {
		if line.starts_with('[') && line.ends_with(']') {
			is_script_group = matches!(line, "[console_scripts]" | "[gui_scripts]");
			continue;
		}
		if is_script_group && let Some((name, _)) = line.split_once('=') {
			let name = name.trim();
			if !name.is_empty() {
				names.push(name.to_owned());
			}
		}
	}
	names
}

#[cfg(test)]
mod tests {
	use super::parse_entry_points;

	#[test]
	fn reads_script_entry_point_groups() {
		let value = "[console_scripts]\nblack = black:main\n[other]\nignored = package:main\n";
		assert_eq!(parse_entry_points(value), ["black"]);
	}
}
