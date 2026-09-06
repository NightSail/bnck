use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::{CuratedScope, MetadataPlan, NPM_PAGE_SIZE, normalize_pypi_name, plan_metadata, plan_npm_page, write_json};

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
	let scope = CuratedScope::load(root.path(), "npm").unwrap();
	for name in ["tool", "tool_suite", "other-name", "@tool/cli", "@different/official"] {
		assert!(scope.matches_name(name), "did not select {name}");
	}
	assert!(!scope.matches_name("unrelated"));
	assert!(!scope.matches_name("@unrelated/tool"));
	assert!(!scope.matches_name("@tool/unrelated-library"));
	assert!(scope.matches_name("@owner/cli"));
	assert!(scope.matches_metadata("renamed", "", "git+https://github.com/owner/other-name.git"));
	assert!(!scope.matches_metadata("renamed", "", "https://github.com/fork/other-name"));
	assert_eq!(scope.packages.into_iter().collect::<Vec<_>>(), ["@different/official"]);
}

#[test]
fn npm_catalog_planning_deduplicates_pages_encodes_scopes_and_excludes_unrelated_packages() {
	let root = tempdir().unwrap();
	let curations_path = root.path().join("curations");
	curations(&curations_path);
	let index = root.path().join("index");
	fs::create_dir(&index).unwrap();
	fs::write(index.join("complete"), "").unwrap();
	write_json(
		&index.join("1.json"),
		&json!({"rows": [
			{"id": "tool"}, {"id": "unrelated"}, {"id": "@different/official"},
		]}),
	)
	.unwrap();
	write_json(&index.join("2.json"), &json!({"rows": [{"id": "tool"}, {"id": "_design/tool"}]})).unwrap();
	let output = root.path().join("plan.json");
	assert_eq!(plan_metadata("npm", &index, &curations_path, &root.path().join("metadata"), &output).unwrap(), 2);
	let plan = MetadataPlan::read(&output, "npm").unwrap();
	assert_eq!(
		plan.packages.iter().map(|package| package.name.as_str()).collect::<Vec<_>>(),
		["@different/official", "tool"]
	);
	assert!(plan.packages.iter().all(|package| package.path.is_absolute()));
	let downloads = fs::read_to_string(output.with_extension("tsv")).unwrap();
	assert!(downloads.contains("https://registry.npmjs.org/%40different%2Fofficial\t"));
	assert!(!downloads.contains("unrelated"));
	assert!(MetadataPlan::read(&output, "pypi").is_err());
}

#[test]
fn incomplete_catalogs_and_unavailable_metadata_are_explicit() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	let index = root.path().join("index");
	fs::create_dir(&index).unwrap();
	let result = plan_metadata("npm", &index, &curated, &root.path().join("metadata"), &root.path().join("plan.json"));
	assert!(result.unwrap_err().to_string().contains("incomplete"));
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
fn a_missing_curated_package_or_malformed_index_is_an_error() {
	let root = tempdir().unwrap();
	let curated = root.path().join("curations");
	curations(&curated);
	let index = root.path().join("index.json");
	let output = root.path().join("plan.json");
	for value in [json!({"rows": []}), json!({"error": "unavailable"})] {
		write_json(&index, &value).unwrap();
		assert!(plan_metadata("npm", &index, &curated, &root.path().join("metadata"), &output).is_err());
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

#[test]
fn npm_pagination_is_inclusive_and_stops_on_a_short_page() {
	let root = tempdir().unwrap();
	let input = root.path().join("page.json");
	let output = root.path().join("next-url");
	let rows = (0..NPM_PAGE_SIZE)
		.map(|index| json!({"id": format!("@scope/tool-{index:05}")}))
		.collect::<Vec<_>>();
	write_json(&input, &json!({"rows": rows})).unwrap();
	assert_eq!(plan_npm_page(&input, &output).unwrap(), 1);
	let next = fs::read_to_string(&output).unwrap();
	assert!(next.contains("startkey=%22%40scope%2Ftool-09999%22"));
	assert!(!next.contains("skip="));
	for rows in [json!([]), json!([{"id": "last"}])] {
		write_json(&input, &json!({"rows": rows})).unwrap();
		assert_eq!(plan_npm_page(&input, &output).unwrap(), 0);
		assert_eq!(fs::read_to_string(&output).unwrap(), "");
	}
}
