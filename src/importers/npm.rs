#[cfg(test)]
#[path = "npm_tests.rs"]
mod snapshot_tests;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};
use walkdir::WalkDir;

use crate::acquisition::{PackageObservation, PackageStatus, SnapshotMetadata, SnapshotStatus, write_acquisition};
use crate::model::Candidate;
use crate::registry::{MetadataPlan, unavailable_metadata};

pub fn import(input: &Path, output: &Path, mut scope: std::collections::BTreeMap<String, String>) -> Result<usize> {
	let mut candidates = Vec::new();
	let mut package_observations = Vec::new();
	for entry in WalkDir::new(input).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", input.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "json") {
			continue;
		}
		let (package_candidates, package_observation) = import_packument(entry.path())?;
		candidates.extend(package_candidates);
		package_observations.push(package_observation);
	}
	let inputs = [input.to_owned()];
	scope.insert("package-selection".to_owned(), "explicit-packages".to_owned());
	let mut metadata = SnapshotMetadata::complete("npm", &inputs, scope);
	metadata.status = SnapshotStatus::Partial;
	write_acquisition(output, metadata, package_observations, candidates)
}

pub fn import_plan(
	plan_path: &Path,
	output: &Path,
	mut scope: std::collections::BTreeMap<String, String>,
) -> Result<usize> {
	let plan = MetadataPlan::read(plan_path, "npm")?;
	let mut candidates = Vec::new();
	let mut packages = Vec::new();
	for selected in &plan.packages {
		if let Some(reason) = unavailable_metadata(&selected.path)? {
			packages.push(PackageObservation {
				name: selected.name.clone(),
				version: None,
				artifact_sha256: None,
				status: PackageStatus::Failed,
				variant: std::collections::BTreeMap::from([("reason".to_owned(), reason)]),
			});
			continue;
		}
		let (observations, package) = import_packument(&selected.path)?;
		ensure!(package.name == selected.name, "expected npm package {}, found {}", selected.name, package.name);
		candidates.extend(observations);
		packages.push(package);
	}
	let inputs = plan.input_paths(plan_path);
	scope.insert("package-selection".to_owned(), "curation-discovery".to_owned());
	let mut metadata = SnapshotMetadata::complete("npm", &inputs, scope);
	if packages.iter().any(|package| package.status == PackageStatus::Failed) {
		metadata.status = SnapshotStatus::Partial;
	}
	write_acquisition(output, metadata, packages, candidates)
}

fn import_packument(path: &Path) -> Result<(Vec<Candidate>, PackageObservation)> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let document: Value =
		serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", path.display()))?;
	let package = document
		.get("name")
		.and_then(Value::as_str)
		.or_else(|| document.get("_id").and_then(Value::as_str))
		.with_context(|| format!("{} has no package name", path.display()))?;
	let Some(version) = document.pointer("/dist-tags/latest").and_then(Value::as_str) else {
		let status = if document.pointer("/time/unpublished").is_some() {
			PackageStatus::Removed
		} else {
			PackageStatus::Failed
		};
		return Ok((
			Vec::new(),
			PackageObservation {
				name: package.to_owned(),
				version: None,
				artifact_sha256: None,
				status,
				variant: std::collections::BTreeMap::from([("reason".to_owned(), "no-latest-dist-tag".to_owned())]),
			},
		));
	};
	let manifest = document
		.get("versions")
		.and_then(Value::as_object)
		.and_then(|versions| versions.get(version))
		.and_then(Value::as_object)
		.with_context(|| format!("{} has no manifest for {version}", path.display()))?;
	if manifest.get("bin").is_none_or(Value::is_null)
		&& manifest.contains_key("directories")
		&& manifest["directories"].get("bin").is_some()
	{
		let mut observation = PackageObservation::observed(package, version);
		observation.status = PackageStatus::Failed;
		observation
			.variant
			.insert("reason".to_owned(), "directories.bin-needs-artifact-inspection".to_owned());
		return Ok((Vec::new(), observation));
	}
	let executables = bin_names(package, manifest);
	let repository = repository_url(manifest.get("repository"));

	let candidates = executables
		.into_iter()
		.map(|executable| Candidate {
			channel: "npm".into(),
			package: package.to_owned(),
			version: version.to_owned(),
			variant: std::collections::BTreeMap::new(),
			artifact_sha256: None,
			executable,
			evidence: "declared-entry-point".into(),
			source_package: None,
			homepage: optional_string(manifest, "homepage"),
			repository: repository.clone(),
			description: optional_string(manifest, "description"),
		})
		.collect();
	Ok((candidates, PackageObservation::observed(package, version)))
}

fn bin_names(package: &str, manifest: &Map<String, Value>) -> Vec<String> {
	match manifest.get("bin") {
		Some(Value::String(_)) => vec![package.rsplit('/').next().unwrap_or(package).to_owned()],
		Some(Value::Object(entries)) => entries.keys().cloned().collect(),
		_ => Vec::new(),
	}
}

fn repository_url(value: Option<&Value>) -> Option<String> {
	match value {
		Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
		Some(Value::Object(value)) => value.get("url").and_then(Value::as_str).map(str::to_owned),
		_ => None,
	}
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Option<String> {
	object
		.get(field)
		.and_then(Value::as_str)
		.filter(|value| !value.is_empty())
		.map(str::to_owned)
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::bin_names;

	#[test]
	fn reads_object_and_string_bin_declarations() {
		let object = json!({"bin": {"eslint": "bin/eslint.js"}});
		assert_eq!(bin_names("eslint", object.as_object().unwrap()), ["eslint"]);

		let string = json!({"bin": "cli.js"});
		assert_eq!(bin_names("@scope/tool", string.as_object().unwrap()), ["tool"]);
	}
}
