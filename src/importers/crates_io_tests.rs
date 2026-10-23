use std::collections::BTreeMap;
use std::fs;

use tempfile::tempdir;

use super::import_dump;
use crate::acquisition::read_snapshot_directory;
use crate::model::read_candidates;

const CRATES: &str = concat!(
	"id,name,homepage,repository\n",
	"1,renamed-cli,,https://github.com/owner/tool\n",
	"2,unrelated,,https://github.com/other/project\n",
	"3,tool-library,,https://github.com/owner/tool\n",
);
const VERSIONS: &str = concat!(
	"crate_id,num,yanked,bin_names\n",
	"1,1.0.0,f,{old}\n",
	"1,10.0.0,f,{current}\n",
	"1,9.0.0,f,{older}\n",
	"1,11.0.0,t,{withdrawn}\n",
	"1,12.0.0-beta.1,f,{beta}\n",
	"2,1.0.0,f,{unrelated}\n",
	"3,1.0.0,f,{removed}\n",
	"3,2.0.0,f,{}\n",
);

#[test]
fn bulk_selection_discovers_renamed_packages_by_repository_and_uses_current_stable_targets_only() {
	let root = tempdir().unwrap();
	let curations = root.path().join("curations");
	fs::create_dir_all(curations.join("projects")).unwrap();
	fs::write(curations.join("manifest.toml"), "schema = 1\n").unwrap();
	fs::write(
		curations.join("projects/tool.toml"),
		"id = 'tool'\nname = 'Tool'\nrepository = 'https://github.com/owner/tool'\n",
	)
	.unwrap();
	let crates = root.path().join("crates.csv");
	let versions = root.path().join("versions.csv");
	fs::write(&crates, CRATES).unwrap();
	fs::write(&versions, VERSIONS).unwrap();
	let acquired = root.path().join("acquired");
	fs::create_dir(&acquired).unwrap();
	let output = acquired.join("crates.jsonl");
	let scope = BTreeMap::from([("registry".to_owned(), "crates.io".to_owned())]);
	assert_eq!(import_dump(&crates, &versions, &output, scope, Some(&curations), None, None).unwrap(), 1);
	let candidates = read_candidates(&output).unwrap();
	assert_eq!(candidates[0].package, "renamed-cli");
	assert_eq!(candidates[0].executable, "current");
	assert_eq!(candidates[0].version, "10.0.0");
	let snapshots = read_snapshot_directory(&acquired).unwrap();
	assert_eq!(snapshots[0].1.packages.len(), 2);
	assert_eq!(snapshots[0].1.scope["package-selection"], "curation-discovery");
	let before = fs::read(curations.join("projects/tool.toml")).unwrap();
	let suggestions = crate::matcher::suggest_registry_packages(&acquired, &curations).unwrap();
	assert_eq!(suggestions.len(), 1);
	assert_eq!(suggestions[0].project, "tool");
	assert_eq!(suggestions[0].package.package, "renamed-cli");
	assert_eq!(fs::read(curations.join("projects/tool.toml")).unwrap(), before);
	/* Discovery is not ownership: neither package produces a canonical provider yet. */
	assert_eq!(
		crate::curation::generate(
			&acquired,
			&curations,
			&root.path().join("providers.jsonl"),
			&root.path().join("names.jsonl"),
			crate::acquisition::SnapshotValidationMode::Full,
		)
		.unwrap()
		.provider_count,
		0
	);
}
