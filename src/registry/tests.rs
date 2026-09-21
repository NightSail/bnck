use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::{CuratedScope, MetadataPlan, normalize_pypi_name, plan_metadata, plan_npm, write_json};

fn curations(root: &std::path::Path) {
	fs::create_dir_all(root.join("projects")).unwrap();
	fs::write(root.join("manifest.toml"), "schema = 0\n").unwrap();
	fs::write(
		root.join("projects/tool.toml"),
		concat!(
			"id = 'tool'\nname = 'Tool'\nrepository = 'https://github.com/owner/other-name'\n",
			"[[package]]\nchannel = 'debian'\nname = 'tool-suite'\n",
			"[[package]]\nchannel = 'npm'\nname = '@different/official'\n",
		),
	)
	.unwrap();
}

#[test]
fn discovery_uses_curated_names_aliases_urls_and_explicit_packages_without_claiming_ownership() {
	let root = tempdir().unwrap();
	curations(root.path());
	let scope = CuratedScope::load(root.path(), "debian").unwrap();
	for name in ["tool", "tool_suite", "other-name"] {
		assert!(scope.matches_name(name), "did not select {name}");
	}
	assert!(!scope.matches_name("unrelated"));
	assert!(scope.matches_metadata("renamed", "", "git+https://github.com/owner/other-name.git"));
	assert!(!scope.matches_metadata("renamed", "", "https://github.com/fork/other-name"));
	assert_eq!(scope.packages.into_iter().collect::<Vec<_>>(), ["tool-suite"]);
}

#[test]
fn npm_planning_uses_only_explicit_packages_including_overrides_and_encodes_latest_urls() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	let overrides = curated.join("packages/npm");
	fs::create_dir_all(&overrides).unwrap();
	fs::write(overrides.join("official.toml"), "channel = 'npm'\nname = '@different/official'\n").unwrap();
	fs::write(overrides.join("override.toml"), "channel = 'npm'\nname = 'override-only'\n").unwrap();
	let output = root.path().join("plan.json");
	let directory = root.path().join("metadata");
	assert_eq!(plan_npm(&curated, &directory, &output).unwrap(), 2);
	let plan = MetadataPlan::read(&output, "npm").unwrap();
	assert_eq!(
		plan.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
		["@different/official", "override-only"]
	);
	assert!(plan.inputs.is_empty());
	assert!(plan.packages.iter().all(|package| package.path.is_absolute()));
	let downloads = fs::read_to_string(output.with_extension("tsv")).unwrap();
	assert!(downloads.contains("https://registry.npmjs.org/%40different%2Fofficial/latest\t"));
	assert!(downloads.contains("https://registry.npmjs.org/override-only/latest\t"));
	assert_eq!(downloads.lines().count(), 2);
	assert!(MetadataPlan::read(&output, "pypi").is_err());
	let before = fs::read(&output).unwrap();
	plan_npm(&curated, &directory, &output).unwrap();
	assert_eq!(before, fs::read(&output).unwrap());
}

#[test]
fn npm_planning_handles_an_empty_selection_without_a_catalog() {
	let root = tempdir().unwrap();
	curations(root.path());
	fs::write(root.path().join("projects/tool.toml"), "id = 'tool'\nname = 'Tool'\n").unwrap();
	let output = root.path().join("plan.json");
	assert_eq!(plan_npm(root.path(), &root.path().join("metadata"), &output).unwrap(), 0);
	assert_eq!(fs::read_to_string(output.with_extension("tsv")).unwrap(), "");
}

#[test]
fn unavailable_metadata_is_explicit() {
	let root = tempdir().unwrap();
	let response = root.path().join("response.json");
	for status in ["404", "410"] {
		fs::write(response.with_extension("status"), status).unwrap();
		assert_eq!(super::unavailable_metadata(&response).unwrap(), Some(format!("http-{status}")));
	}
	fs::write(response.with_extension("status"), "503").unwrap();
	assert!(super::unavailable_metadata(&response).is_err());
	fs::write(response.with_extension("status"), "200").unwrap();
	assert_eq!(super::unavailable_metadata(&response).unwrap(), None);
}

#[test]
fn a_missing_curated_pypi_package_or_malformed_index_is_an_error() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	fs::write(
		curated.join("projects/required.toml"),
		"id = 'required'\nname = 'Required'\n[[package]]\nchannel = 'pypi'\nname = 'required'\n",
	)
	.unwrap();
	let index = root.path().join("index.json");
	let output = root.path().join("plan.json");
	for value in [json!({"projects": []}), json!({"error": "unavailable"})] {
		write_json(&index, &value).unwrap();
		assert!(plan_metadata("pypi", &index, &curated, &root.path().join("metadata"), &output).is_err());
	}
}

#[test]
fn pypi_catalog_normalizes_names_and_does_not_depend_on_catalog_order() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	let index = root.path().join("index.json");
	write_json(
		&index,
		&json!({"projects": [
			{"name": "Tool__Suite"}, {"name": "unrelated"}, {"name": "TOOL"}, {"name": "tool"},
		]}),
	)
	.unwrap();
	let output = root.path().join("plan.json");
	assert_eq!(plan_metadata("pypi", &index, &curated, &root.path().join("metadata"), &output).unwrap(), 2);
	let plan = MetadataPlan::read(&output, "pypi").unwrap();
	assert_eq!(plan.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(), ["tool", "tool-suite"]);
	assert_eq!(normalize_pypi_name("Some._-Tool").unwrap(), "some-tool");
	for name in ["", "../tool", "-tool", "tool/other", "tool\n"] {
		assert!(normalize_pypi_name(name).is_err());
	}
}
