use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use walkdir::WalkDir;

use crate::model::{Candidate, write_json_lines};

pub fn import(input: &Path, output: &Path) -> Result<usize> {
	let mut candidates = Vec::new();
	for entry in WalkDir::new(input).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", input.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "json") {
			continue;
		}
		candidates.extend(import_packument(entry.path())?);
	}
	write_json_lines(output, candidates)
}

fn import_packument(path: &Path) -> Result<Vec<Candidate>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let document: Value =
		serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", path.display()))?;
	let package = document
		.get("name")
		.and_then(Value::as_str)
		.or_else(|| document.get("_id").and_then(Value::as_str))
		.with_context(|| format!("{} has no package name", path.display()))?;
	let version = document
		.pointer("/dist-tags/latest")
		.and_then(Value::as_str)
		.with_context(|| format!("{} has no latest dist-tag", path.display()))?;
	let manifest = document
		.get("versions")
		.and_then(Value::as_object)
		.and_then(|versions| versions.get(version))
		.and_then(Value::as_object)
		.with_context(|| format!("{} has no manifest for {version}", path.display()))?;
	let executables = bin_names(package, manifest);
	let repository = repository_url(manifest.get("repository"));

	Ok(executables
		.into_iter()
		.map(|executable| Candidate {
			channel: "npm".into(),
			package: package.to_owned(),
			version: version.to_owned(),
			executable,
			evidence: "declared-entry-point".into(),
			source_package: None,
			homepage: optional_string(manifest, "homepage"),
			repository: repository.clone(),
			description: optional_string(manifest, "description"),
		})
		.collect())
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
