use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tempfile::tempdir;

use super::{browser_dataset, generate, read_application_assets, read_names, write_assets};
use crate::model::Provider;
use crate::web::SearchQuery;
use crate::web::query_tests::fixture;

fn provider(project: &str, executable: &str, channel: &str) -> Provider {
	Provider {
		project: project.to_owned(),
		executable: executable.to_owned(),
		channel: channel.to_owned(),
		package: format!("{project}-package"),
	}
}

fn asset_path(root: &Path, prefix: &str) -> PathBuf {
	let paths: Vec<_> = fs::read_dir(root.join("assets"))
		.unwrap()
		.map(|entry| entry.unwrap().path())
		.filter(|path| path.file_name().unwrap().to_str().unwrap().starts_with(prefix))
		.collect();
	assert_eq!(paths.len(), 1, "expected one asset with prefix {prefix}");
	paths.into_iter().next().unwrap()
}

#[test]
fn generates_complete_versioned_site_without_modifying_database() {
	let database = fixture([
		provider("helix", "hx", "debian"),
		provider("helix", "hx", "crates.io"),
		provider("other", "hx", "npm"),
		provider("other", "HX", "npm"),
		provider("escaping", "é%&'<>#?", "npm"),
	]);
	let connection = rusqlite::Connection::open(&database.path).unwrap();
	connection
		.execute("INSERT INTO project_links VALUES ('helix', 'homepage', 'https://helix-editor.com/')", [])
		.unwrap();
	drop(connection);
	let before = fs::read(&database.path).unwrap();
	let root = tempdir().unwrap();
	let output = root.path().join("site");
	generate(&database.path, &output).unwrap();
	assert_eq!(before, fs::read(&database.path).unwrap());
	let home = fs::read_to_string(output.join("index.html")).unwrap();
	assert!(home.contains("name='bnck-dataset'"));
	assert!(home.contains("Search and filtered browsing require JavaScript"));
	let asset_directory = asset_path(&output, "app.");
	let dataset_path = asset_path(&output, "dataset.");
	let dataset_bytes = fs::read(&dataset_path).unwrap();
	assert!(!asset_directory.join("dataset.json").exists());
	let dataset_name = dataset_path.file_name().unwrap().to_str().unwrap();
	assert!(home.contains(&format!("content='&#x2f;assets&#x2f;{dataset_name}'")));
	let application_name = asset_directory.file_name().unwrap().to_str().unwrap();
	assert!(home.contains(&format!("src='&#x2f;assets&#x2f;{application_name}/browser.js'")));
	let dataset: Value = serde_json::from_slice(&dataset_bytes).unwrap();
	assert_eq!(dataset["schema_version"], 1);
	assert_eq!(dataset["dataset_release"], "2026-04-16.1");
	assert_eq!(dataset["channels"].as_array().unwrap().len(), 7);
	assert_eq!(dataset["projects"].as_array().unwrap().len(), 3);
	assert_eq!(dataset["projects"][1]["links"][0]["url"], "https://helix-editor.com/");
	assert_eq!(dataset["projects"][1]["providers"].as_array().unwrap().len(), 2);
	for asset in ["style.css", "search.js", "exports.js", "browser.js", "dataset.js", "query.js", "results.js"] {
		assert!(asset_directory.join(asset).is_file());
	}
	for path in [
		"about/index.html",
		"404.html",
		"projects/helix/index.html",
		"executables/hx/index.html",
		"executables/HX/index.html",
		"executables/é%&'<>#?/index.html",
	] {
		assert!(output.join(path).is_file(), "missing {path}");
	}
	let project = fs::read_to_string(output.join("projects/helix/index.html")).unwrap();
	assert!(project.contains("https:&#x2f;&#x2f;helix-editor.com&#x2f;"));
	assert!(!project.contains("browser.js"));
	let export = fs::read_to_string(output.join("executables/hx/providers.json")).unwrap();
	assert!(export.contains("\n\t\"dataset_release\""));
	assert!(export.ends_with('\n'));
	let export: Value = serde_json::from_str(&export).unwrap();
	assert_eq!(export["providers"].as_array().unwrap().len(), 3);
	assert_eq!(export["query"], json!({ "executable": "hx" }));
	let headers = fs::read_to_string(output.join("_headers")).unwrap();
	assert!(headers.contains("max-age=31536000, immutable"));
	assert!(headers.contains("Content-Security-Policy"));
	assert!(!headers.split("\n\n").next().unwrap().contains("Cache-Control"));
	let redirects = fs::read_to_string(output.join("_redirects")).unwrap();
	assert!(
		redirects.contains("/data/latest https://github.com/NightSail/bnck/releases/tag/bnck-dataset-2026-04-16.1 302")
	);
	assert!(redirects.contains(
		"/data/:release/* https://github.com/NightSail/bnck/releases/download/bnck-dataset-:release/:splat 301"
	));
	assert!(!redirects.contains("{{release}}"));
	generate(&database.path, &root.path().join("second")).unwrap();
	let second_assets = asset_path(&root.path().join("second"), "app.");
	let second_dataset = asset_path(&root.path().join("second"), "dataset.");
	assert_eq!(asset_directory.file_name(), second_assets.file_name());
	assert_eq!(dataset_path.file_name(), second_dataset.file_name());
	assert_eq!(dataset_bytes, fs::read(second_dataset).unwrap());
	assert!(
		generate(&database.path, &output)
			.unwrap_err()
			.to_string()
			.contains("already exists")
	);
}

#[test]
fn project_link_changes_only_invalidate_the_dataset() {
	let database = fixture([provider("helix", "hx", "debian")]);
	let root = tempdir().unwrap();
	generate(&database.path, &root.path().join("before")).unwrap();
	let connection = rusqlite::Connection::open(&database.path).unwrap();
	connection
		.execute("INSERT INTO project_links VALUES ('helix', 'homepage', 'https://helix-editor.com/')", [])
		.unwrap();
	drop(connection);
	generate(&database.path, &root.path().join("after")).unwrap();
	let before = root.path().join("before");
	let after = root.path().join("after");
	assert_eq!(asset_path(&before, "app.").file_name(), asset_path(&after, "app.").file_name());
	assert_ne!(asset_path(&before, "dataset.").file_name(), asset_path(&after, "dataset.").file_name());
}

#[test]
fn application_changes_only_invalidate_application_assets() {
	const DATASET: &[u8] = b"{}";
	let root = tempdir().unwrap();
	let application_assets = read_application_assets().unwrap();
	let original = write_assets(&root.path().join("original"), &application_assets, DATASET).unwrap();
	for name in ["browser.js", "style.css"] {
		let mut changed_assets = application_assets.clone();
		changed_assets
			.iter_mut()
			.find(|(asset_name, _)| *asset_name == name)
			.unwrap()
			.1
			.push(b'\n');
		let changed = write_assets(&root.path().join(name), &changed_assets, DATASET).unwrap();
		assert_ne!(original.application, changed.application);
		assert_eq!(original.dataset, changed.dataset);
	}
}

#[test]
fn failed_build_does_not_publish_partial_files() {
	let database = fixture([provider("dot", "..", "npm")]);
	let root = tempdir().unwrap();
	let output = root.path().join("site");
	assert!(
		generate(&database.path, &output)
			.unwrap_err()
			.to_string()
			.contains("static page path")
	);
	assert!(!output.exists());
	assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn empty_dataset_still_generates_a_usable_site() {
	let database = fixture([]);
	let root = tempdir().unwrap();
	generate(&database.path, &root.path().join("site")).unwrap();
	let home = fs::read_to_string(root.path().join("site/index.html")).unwrap();
	assert!(home.contains("No executables listed in this release."));
}

#[test]
fn browser_queries_match_sqlite() {
	let records = [
		("helix", "hx", "debian"),
		("helix", "hx", "crates.io"),
		("other", "hx", "npm"),
		("hx", "hx-admin", "debian"),
		("case", "HX", "debian"),
		("literal", "tool%_", "npm"),
		("unicode", "étool", "pypi"),
		("escaping", "<tool>&'", "npm"),
		("age", "age", "debian"),
		("age", "age-inspect", "debian"),
		("age", "age-keygen", "debian"),
		("age", "age-plugin-batchpass", "npm"),
		("age-plugin-yubikey", "age-plugin-yubikey", "debian"),
		("0tool", "lookup", "npm"),
		(".tool", "lookup", "npm"),
		("tool-prefix", "unrelated", "npm"),
		("short-prefix", "tool-x", "npm"),
		("substring", "xtool", "npm"),
		("substring", "tool-long-prefix", "npm"),
		("unicode", "tool\u{e000}", "npm"),
		("unicode", "tool\u{10000}", "npm"),
	];
	let records = records
		.into_iter()
		.map(|(project, executable, channel)| provider(project, executable, channel))
		.chain((0..120).map(|index| provider(&format!("page-{index:03}"), "tool", "npm")));
	let database = fixture(records);
	let projects = read_names(&database, "SELECT DISTINCT project FROM providers ORDER BY project").unwrap();
	let dataset = browser_dataset(&database, &projects).unwrap();
	let mut cases = Vec::new();
	for query in ["", "hx", "HX", "age", "tool", "%_", "<tool>&'", "É", "é", "\u{85}hx\u{85}", "\u{feff}hx"] {
		for channel in ["", "npm", "debian", "arch"] {
			for initial in ["", "#", "A"] {
				for page in [1, 2, 3, 4, 5] {
					let search = SearchQuery {
						q: Some(query.to_owned()),
						channel: channel.to_owned(),
						initial: initial.to_owned(),
						page,
					};
					cases.push(json!({ "url": search.page_url(page), "results": database.search(&search).unwrap() }));
				}
			}
		}
	}
	let mut child = Command::new("bun")
		.arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/query-parity.js"))
		.stdin(Stdio::piped())
		.spawn()
		.expect("Bun is required for browser/SQLite search parity tests");
	child
		.stdin
		.take()
		.unwrap()
		.write_all(&serde_json::to_vec(&json!({ "dataset": dataset, "cases": cases })).unwrap())
		.unwrap();
	assert!(child.wait().unwrap().success(), "browser queries differ from SQLite");
}
