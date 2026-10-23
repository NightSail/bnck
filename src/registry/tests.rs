use std::fs;

use tempfile::tempdir;

use super::{CuratedScope, MetadataPlan, normalize_pypi_name, plan_npm, plan_pypi_metadata};

fn curations(root: &std::path::Path) {
	fs::create_dir_all(root.join("projects")).unwrap();
	fs::write(root.join("manifest.toml"), "schema = 1\n").unwrap();
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

fn pypi_curation(root: &std::path::Path, name: &str) {
	fs::write(
		root.join("projects/required.toml"),
		format!("id = 'required-project'\nname = 'Required Project'\n[[package]]\nchannel = 'pypi'\nname = '{name}'\n"),
	)
	.unwrap();
}

#[test]
fn pypi_planning_selects_only_explicit_packages_and_overrides_without_a_catalog() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	pypi_curation(&curated, "required");
	let overrides = curated.join("packages/pypi");
	fs::create_dir_all(&overrides).unwrap();
	fs::write(overrides.join("required.toml"), "channel = 'pypi'\nname = 'required'\n").unwrap();
	fs::write(overrides.join("override.toml"), "channel = 'pypi'\nname = 'override-only'\n").unwrap();
	let output = root.path().join("plan.json");
	let directory = root.path().join("metadata");
	assert_eq!(plan_pypi_metadata(&curated, &directory, &output).unwrap(), 2);
	let plan = MetadataPlan::read(&output, "pypi").unwrap();
	assert_eq!(
		plan.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
		["override-only", "required"]
	);
	assert!(plan.inputs.is_empty());
	assert!(plan.packages.iter().all(|package| package.path.is_absolute()));
	let downloads = fs::read_to_string(output.with_extension("tsv")).unwrap();
	assert!(downloads.contains("https://pypi.org/simple/required/\t"));
	assert!(downloads.contains("https://pypi.org/simple/override-only/\t"));
	assert_eq!(downloads.lines().count(), 2);
	assert!(MetadataPlan::read(&output, "npm").is_err());
	let before = fs::read(&output).unwrap();
	plan_pypi_metadata(&curated, &directory, &output).unwrap();
	assert_eq!(before, fs::read(&output).unwrap());
}

#[test]
fn pypi_planning_handles_an_empty_selection_without_using_project_name_hints() {
	let root = tempdir().unwrap();
	curations(root.path());
	let output = root.path().join("plan.json");
	assert_eq!(plan_pypi_metadata(root.path(), &root.path().join("metadata"), &output).unwrap(), 0);
	assert_eq!(fs::read_to_string(output.with_extension("tsv")).unwrap(), "");
}

#[test]
fn pypi_curations_require_normalized_names_to_match_wheel_package_identities() {
	let root = tempdir().unwrap();
	curations(root.path());
	let output = root.path().join("plan.json");
	for name in ["Some.Tool", "some_tool", "some--tool", "../tool", "tool/other"] {
		pypi_curation(root.path(), name);
		assert!(plan_pypi_metadata(root.path(), &root.path().join("metadata"), &output).is_err());
	}
	assert_eq!(normalize_pypi_name("Some._-Tool").unwrap(), "some-tool");
	for name in ["", "../tool", "-tool", "tool/other", "tool\n"] {
		assert!(normalize_pypi_name(name).is_err());
	}
}
