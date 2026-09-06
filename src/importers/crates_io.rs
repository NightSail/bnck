#[cfg(test)]
#[path = "crates_io_tests.rs"]
mod snapshot_tests;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::Deserialize;
use serde_json::Value;

use crate::acquisition::{PackageObservation, SnapshotMetadata, SnapshotStatus, write_acquisition};
use crate::model::Candidate;
use crate::registry::CuratedScope;

#[derive(Debug, Deserialize)]
struct CrateRow {
	id: u64,
	name: String,
	#[serde(default)]
	description: String,
	#[serde(default)]
	homepage: String,
	#[serde(default)]
	repository: String,
}

#[derive(Debug, Deserialize)]
struct VersionRow {
	crate_id: u64,
	num: String,
	#[serde(default, deserialize_with = "csv_bool")]
	yanked: bool,
	#[serde(default)]
	bin_names: String,
	#[serde(default)]
	checksum: String,
}

pub fn import_dump(
	crates_path: &Path,
	versions_path: &Path,
	output: &Path,
	mut scope: std::collections::BTreeMap<String, String>,
	curations: Option<&Path>,
) -> Result<usize> {
	let selection = curations.map(|root| CuratedScope::load(root, "crates.io")).transpose()?;
	let crates = read_crates(crates_path, selection.as_ref())?;
	if selection.is_some() {
		scope.insert("package-selection".to_owned(), "curation-discovery".to_owned());
	}
	let mut reader =
		csv::Reader::from_path(versions_path).with_context(|| format!("open {}", versions_path.display()))?;
	let mut selected = HashMap::<u64, (Version, VersionRow)>::new();

	for row in reader.deserialize::<VersionRow>() {
		let row = row.with_context(|| format!("parse {}", versions_path.display()))?;
		if !crates.contains_key(&row.crate_id) {
			continue;
		}
		let Ok(version) = Version::parse(&row.num) else {
			continue;
		};
		if row.yanked || !version.pre.is_empty() {
			continue;
		}
		let should_replace = selected.get(&row.crate_id).is_none_or(|(current, _)| version > *current);
		if should_replace {
			selected.insert(row.crate_id, (version, row));
		}
	}

	let mut candidates = Vec::new();
	let mut package_observations = Vec::new();
	for (_, row) in selected.into_values() {
		let Some(krate) = crates.get(&row.crate_id) else {
			continue;
		};
		let mut package_observation = PackageObservation::observed(&krate.name, &row.num);
		package_observation.artifact_sha256 = (row.checksum.len() == 64).then(|| row.checksum.clone());
		package_observations.push(package_observation);
		for executable in parse_bin_names(&row.bin_names)? {
			let artifact_sha256 = (row.checksum.len() == 64).then(|| row.checksum.clone());
			candidates.push(candidate(krate, row.num.clone(), artifact_sha256, executable));
		}
	}
	let inputs = [crates_path.to_owned(), versions_path.to_owned()];
	write_acquisition(output, SnapshotMetadata::complete("crates.io", &inputs, scope), package_observations, candidates)
}

pub fn import_api(input: &Path, output: &Path, mut scope: std::collections::BTreeMap<String, String>) -> Result<usize> {
	let mut candidates = Vec::new();
	let mut package_observations = Vec::new();
	for path in json_files(input)? {
		let file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
		let document: Value =
			serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", path.display()))?;
		let crate_value = document
			.get("crate")
			.and_then(Value::as_object)
			.with_context(|| format!("{} has no crate object", path.display()))?;
		let name = string_field(crate_value, "name")?;
		let versions = document
			.get("versions")
			.and_then(Value::as_array)
			.with_context(|| format!("{} has no versions array", path.display()))?;
		let selected = select_api_version(versions)
			.with_context(|| format!("{} has no non-yanked stable version", path.display()))?;
		let version = selected.get("num").and_then(Value::as_str).unwrap_or_default();
		let artifact_sha256 = selected
			.get("checksum")
			.and_then(Value::as_str)
			.filter(|checksum| checksum.len() == 64)
			.map(str::to_owned);
		let mut package_observation = PackageObservation::observed(name, version);
		package_observation.artifact_sha256 = artifact_sha256.clone();
		package_observations.push(package_observation);
		let bin_names = selected.get("bin_names").and_then(Value::as_array);
		let Some(bin_names) = bin_names else {
			continue;
		};

		for executable in bin_names.iter().filter_map(Value::as_str) {
			candidates.push(Candidate {
				channel: "crates.io".into(),
				package: name.to_owned(),
				version: version.to_owned(),
				variant: std::collections::BTreeMap::new(),
				artifact_sha256: artifact_sha256.clone(),
				executable: executable.to_owned(),
				evidence: "declared-entry-point".into(),
				source_package: None,
				homepage: optional_string(crate_value, "homepage"),
				repository: optional_string(crate_value, "repository"),
				description: optional_string(crate_value, "description"),
			});
		}
	}

	let inputs = [input.to_owned()];
	scope.insert("package-selection".to_owned(), "explicit-packages".to_owned());
	let mut metadata = SnapshotMetadata::complete("crates.io", &inputs, scope);
	metadata.status = SnapshotStatus::Partial;
	write_acquisition(output, metadata, package_observations, candidates)
}

fn read_crates(path: &Path, selection: Option<&CuratedScope>) -> Result<HashMap<u64, CrateRow>> {
	let mut reader = csv::Reader::from_path(path).with_context(|| format!("open {}", path.display()))?;
	let mut crates = HashMap::new();
	for row in reader.deserialize::<CrateRow>() {
		let row = row.with_context(|| format!("parse {}", path.display()))?;
		if selection.is_none_or(|selection| selection.matches_metadata(&row.name, &row.homepage, &row.repository)) {
			crates.insert(row.id, row);
		}
	}
	Ok(crates)
}

fn candidate(krate: &CrateRow, version: String, artifact_sha256: Option<String>, executable: String) -> Candidate {
	Candidate {
		channel: "crates.io".into(),
		package: krate.name.clone(),
		version,
		variant: std::collections::BTreeMap::new(),
		artifact_sha256,
		executable,
		evidence: "declared-entry-point".into(),
		source_package: None,
		homepage: nonempty(&krate.homepage),
		repository: nonempty(&krate.repository),
		description: nonempty(&krate.description),
	}
}

fn parse_bin_names(value: &str) -> Result<Vec<String>> {
	let value = value.trim();
	if value.is_empty() || value == "\\N" || value == "{}" || value == "[]" {
		return Ok(Vec::new());
	}
	if value.starts_with('[') {
		return serde_json::from_str(value).context("parse bin_names JSON");
	}
	if let Some(inner) = value.strip_prefix('{').and_then(|value| value.strip_suffix('}')) {
		return Ok(inner
			.split(',')
			.map(|name| name.trim().trim_matches('"').replace("\\\"", "\""))
			.filter(|name| !name.is_empty())
			.collect());
	}
	bail!("unsupported bin_names value: {value}")
}

fn select_api_version(versions: &[Value]) -> Option<&Value> {
	versions
		.iter()
		.filter(|version| !version.get("yanked").and_then(Value::as_bool).unwrap_or(false))
		.filter_map(|value| {
			let version = Version::parse(value.get("num")?.as_str()?).ok()?;
			version.pre.is_empty().then_some((version, value))
		})
		.max_by(|left, right| left.0.cmp(&right.0))
		.map(|(_, value)| value)
}

fn json_files(path: &Path) -> Result<Vec<std::path::PathBuf>> {
	if path.is_file() {
		return Ok(vec![path.to_owned()]);
	}
	let mut paths = Vec::new();
	for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
		let path = entry?.path();
		if path.extension().is_some_and(|extension| extension == "json") {
			paths.push(path);
		}
	}
	paths.sort();
	Ok(paths)
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, field: &str) -> Result<&'a str> {
	object
		.get(field)
		.and_then(Value::as_str)
		.with_context(|| format!("missing {field}"))
}

fn optional_string(object: &serde_json::Map<String, Value>, field: &str) -> Option<String> {
	object
		.get(field)
		.and_then(Value::as_str)
		.filter(|value| !value.is_empty())
		.map(str::to_owned)
}

fn nonempty(value: &str) -> Option<String> {
	(!value.is_empty() && value != "\\N").then(|| value.to_owned())
}

fn csv_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let value = String::deserialize(deserializer)?;
	Ok(matches!(value.as_str(), "t" | "true" | "1"))
}

#[cfg(test)]
mod tests {
	use super::parse_bin_names;

	#[test]
	fn parses_postgres_and_json_arrays() {
		assert_eq!(parse_bin_names("{rg,rgrep}").unwrap(), ["rg", "rgrep"]);
		assert_eq!(parse_bin_names("[\"rg\"]").unwrap(), ["rg"]);
	}
}
