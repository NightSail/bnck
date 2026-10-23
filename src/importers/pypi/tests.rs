use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

use super::selection::{Selection, WheelPlan, select};
use super::{import, import_plan, plan_snapshot};
use crate::acquisition::{PackageStatus, SnapshotStatus, file_identity, read_snapshot_directory};
use crate::model::read_candidates;
use crate::registry::{MetadataPlan, plan_pypi_metadata, write_json};

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ENTRY_POINTS: &str = "[console_scripts]\ntool = package:main\n[gui_scripts]\ntool-gui = package:gui\n";

fn wheel(version: &str) -> Value {
	json!({
		"filename": format!("some_tool-{version}-py3-none-any.whl"),
		"url": "https://files.pythonhosted.org/tool.whl", "hashes": {"sha256": DIGEST},
		"size": 100, "yanked": false,
	})
}

fn simple(path: &Path, files: Vec<Value>) {
	write_json(path, &json!({"name": "Some.Tool", "files": files, "versions": ["999.0", "1.0"]})).unwrap();
}

fn selected_version(path: &Path) -> String {
	match select(path, path.parent().unwrap()).unwrap() {
		Selection::Wheel { version, .. } => version,
		other => panic!("unexpected selection {other:?}"),
	}
}

fn create_wheel(path: &Path, name: &str, version: &str) {
	let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
	for (member, content) in [
		("some_tool-1.0.dist-info/METADATA", format!("Name: {name}\nVersion: {version}\n")),
		("some_tool-1.0.dist-info/entry_points.txt", ENTRY_POINTS.to_owned()),
		("some_tool-1.0.data/scripts/direct", "not executed".to_owned()),
		("some_tool-1.0.data/scripts/private/nested", "not a direct installed command".to_owned()),
	] {
		archive.start_file(member, SimpleFileOptions::default()).unwrap();
		archive.write_all(content.as_bytes()).unwrap();
	}
	archive.finish().unwrap();
}

fn scope() -> BTreeMap<String, String> {
	BTreeMap::from([("release-policy".to_owned(), "latest-non-yanked-stable".to_owned())])
}

#[test]
fn release_selection_uses_pep440_not_array_order_and_excludes_yanks_and_prereleases() {
	let root = tempdir().unwrap();
	let path = root.path().join("simple.json");
	let mut yanked = wheel("20.0");
	yanked["yanked"] = json!("withdrawn");
	let files = vec![wheel("10.0"), wheel("9.0"), wheel("11.0rc1"), wheel("11.0.dev1"), yanked];
	simple(&path, files.clone());
	assert_eq!(selected_version(&path), "10.0");
	simple(&path, files.into_iter().rev().collect());
	assert_eq!(selected_version(&path), "10.0");
	simple(&path, vec![wheel("10.0"), wheel("1!1.0"), wheel("10.0.post1")]);
	assert_eq!(selected_version(&path), "1!1.0");
	simple(&path, vec![wheel("10.0"), wheel("10.0.post1")]);
	assert_eq!(selected_version(&path), "10.0.post1");
}

#[test]
fn wheel_choice_is_deterministic_and_prefers_universal_before_size() {
	let root = tempdir().unwrap();
	let path = root.path().join("simple.json");
	let mut platform = wheel("1.0");
	platform["filename"] = json!("some_tool-1.0-cp313-cp313-manylinux_x86_64.whl");
	platform["size"] = json!(1);
	simple(&path, vec![platform, wheel("1.0")]);
	let Selection::Wheel { filename, .. } = select(&path, root.path()).unwrap() else { panic!() };
	assert_eq!(filename, "some_tool-1.0-py3-none-any.whl");
}

#[test]
fn a_newer_source_only_release_is_unknown_not_a_fallback_to_old_commands() {
	let root = tempdir().unwrap();
	let path = root.path().join("simple.json");
	simple(&path, vec![wheel("1.0"), json!({"filename": "some-tool-2.0.tar.gz", "yanked": false})]);
	let Selection::Unsupported { package, version, reason } = select(&path, root.path()).unwrap() else { panic!() };
	assert_eq!(package, "some-tool");
	assert_eq!(version.as_deref(), Some("2.0"));
	assert_eq!(reason, "current-release-has-no-wheel");
}

#[test]
fn planning_rejects_unsafe_digests_and_download_urls() {
	let root = tempdir().unwrap();
	let path = root.path().join("simple.json");
	for (field, value) in [
		("hashes", json!({"sha256": "../escape"})),
		("url", json!("file:///etc/passwd")),
		("url", json!("https://example.org/wheel\tother")),
	] {
		let mut file = wheel("1.0");
		file[field] = value;
		simple(&path, vec![file]);
		assert!(select(&path, root.path()).is_err());
	}
}

#[test]
fn imports_only_the_current_selection_and_checks_its_digest_and_metadata() {
	let root = tempdir().unwrap();
	let cache = root.path().join("wheels");
	let metadata = root.path().join("simple");
	let output = root.path().join("acquired");
	for directory in [&cache, &metadata, &output] {
		fs::create_dir(directory).unwrap();
	}
	let temporary = cache.join("download.whl");
	create_wheel(&temporary, "Some.Tool", "1.0");
	let digest = file_identity(&temporary).unwrap().1;
	let wheel_path = cache.join(format!("{digest}.whl"));
	fs::rename(temporary, &wheel_path).unwrap();
	fs::write(cache.join("old.whl"), "stale cache content must never be imported").unwrap();
	let mut current = wheel("1.0");
	current["hashes"]["sha256"] = json!(digest);
	simple(&metadata.join("tool.json"), vec![current]);
	let providers = output.join("pypi.jsonl");
	assert_eq!(import(&cache, &providers, scope()).unwrap(), 3);
	let candidates = read_candidates(&providers).unwrap();
	assert!(candidates.iter().all(|candidate| candidate.package == "some-tool"));
	assert_eq!(
		candidates
			.iter()
			.map(|candidate| candidate.executable.as_str())
			.collect::<Vec<_>>(),
		["direct", "tool", "tool-gui"]
	);
	assert_eq!(read_snapshot_directory(&output).unwrap()[0].1.status, SnapshotStatus::Partial);
	fs::write(cache.join("another-old.whl"), "not included in input identity").unwrap();
	assert!(read_snapshot_directory(&output).is_ok());
	fs::write(&wheel_path, "corrupted").unwrap();
	assert!(
		import(&cache, &providers, scope())
			.unwrap_err()
			.to_string()
			.contains("SHA-256 mismatch")
	);
}

fn curated_metadata(root: &Path) -> (PathBuf, MetadataPlan) {
	let curations = root.join("curations");
	fs::create_dir_all(curations.join("projects")).unwrap();
	fs::write(curations.join("manifest.toml"), "schema = 1\n").unwrap();
	fs::write(
		curations.join("projects/tool.toml"),
		"id = 'tool'\nname = 'Tool'\n[[package]]\nchannel = 'pypi'\nname = 'some-tool'\n",
	)
	.unwrap();
	let path = root.join("metadata.json");
	assert_eq!(plan_pypi_metadata(&curations, &root.join("packages"), &path).unwrap(), 1);
	let plan = MetadataPlan::read(&path, "pypi").unwrap();
	(path, plan)
}

#[test]
fn curated_metadata_flows_through_wheel_selection_and_import_without_an_index() {
	let root = tempdir().unwrap();
	let (metadata_path, metadata) = curated_metadata(root.path());
	let cache = root.path().join("wheels");
	fs::create_dir(&cache).unwrap();
	let temporary = cache.join("download.whl");
	create_wheel(&temporary, "Some.Tool", "1.0");
	let digest = file_identity(&temporary).unwrap().1;
	fs::rename(&temporary, cache.join(format!("{digest}.whl"))).unwrap();
	let mut current = wheel("1.0");
	current["hashes"]["sha256"] = json!(digest);
	simple(&metadata.packages[0].path, vec![current]);
	fs::write(metadata.packages[0].path.with_extension("status"), "200\n").unwrap();
	fs::write(root.path().join("packages/unplanned.json"), "not selected").unwrap();
	let wheels_path = root.path().join("wheels.json");
	assert_eq!(plan_snapshot(&metadata_path, &cache, &wheels_path).unwrap(), 1);
	let output = root.path().join("acquired");
	fs::create_dir(&output).unwrap();
	assert_eq!(import_plan(&wheels_path, &output.join("pypi.jsonl"), scope()).unwrap(), 3);
	let snapshots = read_snapshot_directory(&output).unwrap();
	let snapshot = &snapshots[0].1;
	assert_eq!(snapshot.status, SnapshotStatus::Complete);
	assert_eq!(snapshot.scope["package-selection"], "curated-packages");
	assert_eq!(snapshot.packages.len(), 1);
	assert_eq!(snapshot.packages[0].name, "some-tool");
	/* Metadata plan, selected response and status, wheel plan, and selected wheel only. */
	assert_eq!(snapshot.inputs.len(), 5);
}

#[test]
fn unavailable_curated_packages_are_retained_as_failed_observations_without_wheel_requests() {
	let root = tempdir().unwrap();
	let (metadata_path, metadata) = curated_metadata(root.path());
	let wheels_path = root.path().join("wheels.json");
	let output = root.path().join("acquired");
	fs::create_dir(&output).unwrap();
	for status in ["404", "410"] {
		fs::write(&metadata.packages[0].path, "unavailable response").unwrap();
		fs::write(metadata.packages[0].path.with_extension("status"), status).unwrap();
		assert_eq!(plan_snapshot(&metadata_path, &root.path().join("cache"), &wheels_path).unwrap(), 0);
		assert_eq!(fs::read_to_string(wheels_path.with_extension("tsv")).unwrap(), "");
		assert_eq!(import_plan(&wheels_path, &output.join("pypi.jsonl"), scope()).unwrap(), 0);
		let snapshots = read_snapshot_directory(&output).unwrap();
		let snapshot = &snapshots[0].1;
		assert_eq!(snapshot.status, SnapshotStatus::Partial);
		assert_eq!(snapshot.scope["package-selection"], "curated-packages");
		assert_eq!(snapshot.packages[0].status, PackageStatus::Failed);
		assert_eq!(snapshot.packages[0].variant["reason"], format!("http-{status}"));
	}
}

#[test]
fn snapshot_plan_preserves_unsupported_packages_as_partial_acquisition() {
	let root = tempdir().unwrap();
	let plan_path = root.path().join("wheels.json");
	write_json(
		&plan_path,
		&WheelPlan {
			inputs: Vec::new(),
			packages: vec![Selection::Unsupported {
				package: "source-only".to_owned(),
				version: Some("2.0".to_owned()),
				reason: "current-release-has-no-wheel".to_owned(),
			}],
		},
	)
	.unwrap();
	let output = root.path().join("acquired");
	fs::create_dir(&output).unwrap();
	assert_eq!(import_plan(&plan_path, &output.join("pypi.jsonl"), scope()).unwrap(), 0);
	let snapshots = read_snapshot_directory(&output).unwrap();
	assert_eq!(snapshots[0].1.status, SnapshotStatus::Partial);
	assert_eq!(snapshots[0].1.packages[0].status, PackageStatus::Failed);
	assert_eq!(snapshots[0].1.packages[0].variant["reason"], "current-release-has-no-wheel");
}

#[test]
fn a_valid_digest_does_not_allow_a_wheel_for_another_package_or_version() {
	let root = tempdir().unwrap();
	for (name, version) in [("other-tool", "1.0"), ("some-tool", "2.0")] {
		let path = root.path().join("wheel.whl");
		create_wheel(&path, name, version);
		let plan = WheelPlan {
			inputs: Vec::new(),
			packages: vec![Selection::Wheel {
				package: "some-tool".to_owned(),
				version: "1.0".to_owned(),
				filename: "some_tool-1.0-py3-none-any.whl".to_owned(),
				url: "https://example.org/wheel.whl".to_owned(),
				sha256: file_identity(&path).unwrap().1,
				path,
			}],
		};
		let plan_path = root.path().join("plan.json");
		write_json(&plan_path, &plan).unwrap();
		assert!(import_plan(&plan_path, &root.path().join("output.jsonl"), scope()).is_err());
	}
}
