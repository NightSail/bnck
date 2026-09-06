use std::collections::BTreeMap;
use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::import_plan;
use crate::acquisition::{PackageStatus, SnapshotStatus, read_snapshot_directory};
use crate::model::read_candidates;
use crate::registry::{MetadataPackage, MetadataPlan, write_json};

#[test]
fn snapshot_import_follows_latest_tag_preserves_scopes_and_ignores_unplanned_cache_files() {
	let root = tempdir().unwrap();
	let metadata = root.path().join("package.json");
	write_json(
		&metadata,
		&json!({
			"name": "@scope/tool", "dist-tags": {"latest": "1.0.0"},
			"versions": {
				"1.0.0": {"bin": {"tool": "bin/tool.js"}, "repository": {"url": "https://github.com/owner/tool"}},
				"2.0.0-beta.1": {"bin": {"future": "future.js"}},
				"0.9.0": {"bin": {"removed-command": "old.js"}}
			}
		}),
	)
	.unwrap();
	fs::write(root.path().join("stale.json"), "not selected").unwrap();
	let plan_path = root.path().join("plan.json");
	write_json(
		&plan_path,
		&MetadataPlan {
			channel: "npm".to_owned(),
			inputs: Vec::new(),
			packages: vec![MetadataPackage { name: "@scope/tool".to_owned(), path: metadata }],
		},
	)
	.unwrap();
	let output = root.path().join("acquired");
	fs::create_dir(&output).unwrap();
	let candidates_path = output.join("npm.jsonl");
	let scope = BTreeMap::from([("registry".to_owned(), "npm".to_owned())]);
	assert_eq!(import_plan(&plan_path, &candidates_path, scope.clone()).unwrap(), 1);
	let candidates = read_candidates(&candidates_path).unwrap();
	assert_eq!(candidates[0].package, "@scope/tool");
	assert_eq!(candidates[0].executable, "tool");
	assert_eq!(candidates[0].version, "1.0.0");
	assert_eq!(read_snapshot_directory(&output).unwrap()[0].1.status, SnapshotStatus::Complete);
	let mut plan = MetadataPlan::read(&plan_path, "npm").unwrap();
	plan.packages[0].name = "other-tool".to_owned();
	write_json(&plan_path, &plan).unwrap();
	assert!(import_plan(&plan_path, &candidates_path, scope).is_err());
}

#[test]
fn unknown_executable_extraction_is_not_an_observed_library_only_package() {
	let root = tempdir().unwrap();
	let path = root.path().join("package.json");
	write_json(
		&path,
		&json!({
			"name": "tool", "dist-tags": {"latest": "1.0.0"},
			"versions": {"1.0.0": {"directories": {"bin": "bin"}}}
		}),
	)
	.unwrap();
	let (candidates, observation) = super::import_packument(&path).unwrap();
	assert!(candidates.is_empty());
	assert_eq!(observation.status, PackageStatus::Failed);
	assert_eq!(observation.variant["reason"], "directories.bin-needs-artifact-inspection");
	write_json(&path, &json!({"name": "tool", "time": {"unpublished": {"time": "2026-01-01"}}})).unwrap();
	let (_, observation) = super::import_packument(&path).unwrap();
	assert_eq!(observation.status, PackageStatus::Removed);
	write_json(&path, &json!({"name": "tool", "dist-tags": {"latest": "1.0.0"}, "versions": {"1.0.0": {}}})).unwrap();
	let (candidates, observation) = super::import_packument(&path).unwrap();
	assert!(candidates.is_empty());
	assert_eq!(observation.status, PackageStatus::Observed);
}
