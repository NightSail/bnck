use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

use super::import_plan;
use crate::acquisition::{PackageStatus, SnapshotStatus, read_snapshot_directory};
use crate::model::read_candidates;
use crate::registry::{MetadataPackage, MetadataPlan, write_json};

struct Snapshot {
	_root: TempDir,
	metadata: PathBuf,
	plan: PathBuf,
	acquired: PathBuf,
	output: PathBuf,
}

impl Snapshot {
	fn new() -> Self {
		let root = tempdir().unwrap();
		let metadata = root.path().join("package.json");
		write_json(&metadata, &release()).unwrap();
		fs::write(root.path().join("stale.json"), "not selected").unwrap();
		let plan = root.path().join("plan.json");
		write_json(
			&plan,
			&MetadataPlan {
				channel: "npm".to_owned(),
				inputs: Vec::new(),
				packages: vec![MetadataPackage { name: "@scope/tool".to_owned(), path: metadata.clone() }],
			},
		)
		.unwrap();
		let acquired = root.path().join("acquired");
		fs::create_dir(&acquired).unwrap();
		let output = acquired.join("npm.jsonl");
		Self { _root: root, metadata, plan, acquired, output }
	}

	fn import(&self) -> anyhow::Result<usize> {
		import_plan(&self.plan, &self.output, BTreeMap::new(), None, None)
	}
}

fn release() -> Value {
	json!({
		"name": "@scope/tool", "version": "1.0.0",
		"bin": {"tool": "bin/tool.js"},
		"repository": {"url": "https://github.com/owner/tool"},
		"description": "A tool"
	})
}

#[test]
fn snapshot_import_reads_the_selected_release_and_ignores_unplanned_cache_files() {
	let snapshot = Snapshot::new();
	assert_eq!(snapshot.import().unwrap(), 1);
	let candidates = read_candidates(&snapshot.output).unwrap();
	assert_eq!(candidates[0].package, "@scope/tool");
	assert_eq!(candidates[0].executable, "tool");
	assert_eq!(candidates[0].version, "1.0.0");
	assert_eq!(candidates[0].repository.as_deref(), Some("https://github.com/owner/tool"));
	let snapshots = read_snapshot_directory(&snapshot.acquired).unwrap();
	let manifest = &snapshots[0].1;
	assert_eq!(manifest.status, SnapshotStatus::Complete);
	assert_eq!(manifest.scope["package-selection"], "curated-packages");
	assert_eq!(manifest.scope["release-policy"], "latest-dist-tag");
	assert_eq!(manifest.inputs.len(), 2);
	fs::write(&snapshot.metadata, "changed").unwrap();
	assert!(read_snapshot_directory(&snapshot.acquired).is_err());
}

#[test]
fn refreshed_release_drops_commands_and_does_not_fall_back_to_older_metadata() {
	let snapshot = Snapshot::new();
	assert_eq!(snapshot.import().unwrap(), 1);
	write_json(&snapshot.metadata, &json!({"name": "@scope/tool", "version": "2.0.0"})).unwrap();
	assert_eq!(snapshot.import().unwrap(), 0);
	let snapshots = read_snapshot_directory(&snapshot.acquired).unwrap();
	assert_eq!(snapshots[0].1.packages[0].status, PackageStatus::Observed);
	assert_eq!(snapshots[0].1.packages[0].version.as_deref(), Some("2.0.0"));
}

#[test]
fn unavailable_latest_releases_are_failed_not_library_only_or_removed() {
	let snapshot = Snapshot::new();
	for status in ["404", "410"] {
		fs::write(snapshot.metadata.with_extension("status"), status).unwrap();
		fs::write(&snapshot.metadata, "unavailable").unwrap();
		assert_eq!(snapshot.import().unwrap(), 0);
		let snapshots = read_snapshot_directory(&snapshot.acquired).unwrap();
		assert_eq!(snapshots[0].1.status, SnapshotStatus::Partial);
		assert_eq!(snapshots[0].1.packages[0].status, PackageStatus::Failed);
		assert_eq!(snapshots[0].1.packages[0].variant["reason"], format!("http-{status}"));
		assert_eq!(snapshots[0].1.inputs.len(), 3);
	}
}

#[test]
fn unsupported_directory_extraction_makes_the_snapshot_partial() {
	let snapshot = Snapshot::new();
	write_json(&snapshot.metadata, &json!({"name": "@scope/tool", "version": "1.0.0", "directories": {"bin": "bin"}}))
		.unwrap();
	assert_eq!(snapshot.import().unwrap(), 0);
	let snapshots = read_snapshot_directory(&snapshot.acquired).unwrap();
	assert_eq!(snapshots[0].1.status, SnapshotStatus::Partial);
	assert_eq!(snapshots[0].1.packages[0].status, PackageStatus::Failed);
	assert_eq!(snapshots[0].1.packages[0].variant["reason"], "directories.bin-needs-artifact-inspection");
}

#[test]
fn mismatched_missing_or_malformed_metadata_fails_import() {
	let snapshot = Snapshot::new();
	let mut manifest = release();
	manifest["name"] = json!("other-tool");
	write_json(&snapshot.metadata, &manifest).unwrap();
	assert!(snapshot.import().unwrap_err().to_string().contains("expected npm package"));
	for field in ["name", "version"] {
		let mut manifest = release();
		manifest.as_object_mut().unwrap().remove(field);
		write_json(&snapshot.metadata, &manifest).unwrap();
		assert!(snapshot.import().is_err());
	}
	for invalid in [json!(42), json!(["tool"]), json!({"tool": false})] {
		let mut manifest = release();
		manifest["bin"] = invalid;
		write_json(&snapshot.metadata, &manifest).unwrap();
		assert!(snapshot.import().is_err());
	}
	fs::write(&snapshot.metadata, "not JSON").unwrap();
	assert!(snapshot.import().is_err());
	fs::remove_file(&snapshot.metadata).unwrap();
	assert!(snapshot.import().is_err());
	fs::write(snapshot.metadata.with_extension("status"), "404").unwrap();
	assert!(snapshot.import().is_err());
}

#[test]
fn old_packument_inputs_are_rejected_instead_of_silently_importing_no_commands() {
	let snapshot = Snapshot::new();
	write_json(
		&snapshot.metadata,
		&json!({"name": "@scope/tool", "dist-tags": {"latest": "1.0.0"}, "versions": {"1.0.0": {}}}),
	)
	.unwrap();
	assert!(snapshot.import().unwrap_err().to_string().contains("release version"));
}
