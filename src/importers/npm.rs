#[cfg(test)]
#[path = "npm_tests.rs"]
mod snapshot_tests;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};

use crate::acquisition::{PackageObservation, PackageStatus, SnapshotMetadata, SnapshotStatus, write_acquisition};
use crate::model::Candidate;
use crate::registry::{MetadataPlan, unavailable_metadata};

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
		let (observations, package) = import_manifest(&selected.path)?;
		ensure!(package.name == selected.name, "expected npm package {}, found {}", selected.name, package.name);
		candidates.extend(observations);
		packages.push(package);
	}
	let inputs = plan.input_paths(plan_path);
	scope.insert("package-selection".to_owned(), "curated-packages".to_owned());
	scope.insert("release-policy".to_owned(), "latest-dist-tag".to_owned());
	scope.insert("catalog-policy".to_owned(), "bounded-non-atomic-scan".to_owned());
	let mut metadata = SnapshotMetadata::complete("npm", &inputs, scope);
	if packages.iter().any(|package| package.status == PackageStatus::Failed) {
		metadata.status = SnapshotStatus::Partial;
	}
	write_acquisition(output, metadata, packages, candidates)
}

fn import_manifest(path: &Path) -> Result<(Vec<Candidate>, PackageObservation)> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let manifest: Map<String, Value> =
		serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", path.display()))?;
	let package = manifest
		.get("name")
		.and_then(Value::as_str)
		.with_context(|| format!("{} has no package name", path.display()))?;
	let version = manifest
		.get("version")
		.and_then(Value::as_str)
		.with_context(|| format!("{} has no release version", path.display()))?;
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
	let executables = bin_names(package, &manifest).with_context(|| format!("parse {} bin", path.display()))?;
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
			homepage: optional_string(&manifest, "homepage"),
			repository: repository.clone(),
			description: optional_string(&manifest, "description"),
		})
		.collect();
	Ok((candidates, PackageObservation::observed(package, version)))
}

fn bin_names(package: &str, manifest: &Map<String, Value>) -> Result<Vec<String>> {
	match manifest.get("bin") {
		Some(Value::String(_)) => Ok(vec![package.rsplit('/').next().unwrap_or(package).to_owned()]),
		Some(Value::Object(entries)) => {
			ensure!(entries.values().all(Value::is_string), "bin targets must be strings");
			Ok(entries.keys().cloned().collect())
		},
		None | Some(Value::Null) => Ok(Vec::new()),
		_ => anyhow::bail!("bin must be a string or an object"),
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
		assert_eq!(bin_names("eslint", object.as_object().unwrap()).unwrap(), ["eslint"]);

		let string = json!({"bin": "cli.js"});
		assert_eq!(bin_names("@scope/tool", string.as_object().unwrap()).unwrap(), ["tool"]);
	}
}
