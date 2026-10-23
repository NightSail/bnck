use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub const PROVIDER_SCHEMA_VERSION: u32 = 1;
pub const ASSOCIATION_SCHEMA_VERSION: u32 = 1;
pub const CHANNELS: [&str; 7] = ["arch", "crates.io", "debian", "fedora", "homebrew", "npm", "pypi"];
pub const ASSOCIATION_ROLES: [&str; 2] = ["reference", "implementation"];

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
	pub channel: String,
	pub package: String,
	pub version: String,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub variant: BTreeMap<String, String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub artifact_sha256: Option<String>,
	pub executable: String,
	pub evidence: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_package: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub homepage: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub repository: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
	pub project: String,
	pub executable: String,
	pub channel: String,
	pub package: String,
}

impl Provider {
	pub fn validate(&self) -> Result<()> {
		validate_project_id(&self.project)?;
		validate_executable(&self.executable)?;
		validate_channel(&self.channel)?;
		validate_identity("package", &self.package)?;
		Ok(())
	}
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NameAssociation {
	pub executable: String,
	pub project: String,
	pub role: String,
}

impl NameAssociation {
	pub fn validate(&self) -> Result<()> {
		validate_executable(&self.executable)?;
		validate_project_id(&self.project)?;
		validate_association_role(&self.role)?;
		Ok(())
	}
}

pub fn validate_project_id(value: &str) -> Result<()> {
	validate_identity("project ID", value)?;
	ensure!(
		value.chars().all(|character| character.is_ascii_lowercase()
			|| character.is_ascii_digit()
			|| matches!(character, '-' | '.' | '_')),
		"project ID {value:?} must contain only lowercase ASCII letters, digits, '-', '.', or '_'"
	);
	Ok(())
}

pub fn validate_channel(value: &str) -> Result<()> {
	ensure!(CHANNELS.contains(&value), "unknown channel {value}");
	Ok(())
}

pub fn validate_association_role(value: &str) -> Result<()> {
	ensure!(ASSOCIATION_ROLES.contains(&value), "unknown name association role {value}");
	Ok(())
}

pub fn validate_executable(value: &str) -> Result<()> {
	validate_identity("executable", value)?;
	ensure!(!value.contains('/'), "executable {value:?} contains /");
	Ok(())
}

pub fn validate_identity(kind: &str, value: &str) -> Result<()> {
	ensure!(!value.is_empty(), "{kind} is empty");
	ensure!(value.trim() == value, "{kind} has leading or trailing whitespace");
	ensure!(!value.chars().any(char::is_control), "{kind} contains a control character");
	Ok(())
}

pub fn validate_sha256(kind: &str, value: &str) -> Result<()> {
	ensure!(
		value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
		"{kind} SHA-256 is invalid"
	);
	Ok(())
}

pub fn write_json_lines<T>(path: &Path, values: impl IntoIterator<Item = T>) -> Result<usize>
where
	T: Ord + Serialize,
{
	let values: BTreeSet<_> = values.into_iter().collect();
	let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
	let mut writer = BufWriter::new(file);

	for value in &values {
		serde_json::to_writer(&mut writer, value)?;
		writer.write_all(b"\n")?;
	}
	writer.flush()?;

	Ok(values.len())
}

#[derive(Debug, Eq, PartialEq)]
pub struct ProviderDataset {
	pub sha256: String,
	pub provider_count: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct NameAssociationDataset {
	pub sha256: String,
	pub association_count: usize,
}

pub fn read_provider_dataset(
	path: &Path,
	mut read_provider: impl FnMut(&Provider) -> Result<()>,
) -> Result<ProviderDataset> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut reader = BufReader::new(file);
	let mut digest = Sha256::new();
	let mut previous = None::<Provider>;
	let mut provider_count = 0;
	let mut bytes = Vec::new();

	loop {
		bytes.clear();
		if reader.read_until(b'\n', &mut bytes)? == 0 {
			break;
		}
		let line_number = provider_count + 1;
		ensure!(bytes.ends_with(b"\n"), "{} line {line_number} has no final LF", path.display());
		digest.update(&bytes);
		bytes.pop();
		ensure!(!bytes.is_empty(), "{} line {line_number} is blank", path.display());

		let line = std::str::from_utf8(&bytes)
			.with_context(|| format!("{} line {line_number} is not UTF-8", path.display()))?;
		let provider: Provider =
			serde_json::from_str(line).with_context(|| format!("parse {} line {line_number}", path.display()))?;
		let canonical = serde_json::to_string(&provider)?;
		ensure!(line == canonical, "{} line {line_number} is not canonical JSON", path.display());
		provider
			.validate()
			.with_context(|| format!("validate {} line {line_number}", path.display()))?;
		if let Some(previous) = previous.as_ref() {
			if provider == *previous {
				bail!("{} line {line_number} duplicates the preceding provider", path.display());
			}
			ensure!(provider > *previous, "{} line {line_number} is not sorted", path.display());
		}

		read_provider(&provider).with_context(|| format!("process {} line {line_number}", path.display()))?;
		previous = Some(provider);
		provider_count += 1;
	}

	Ok(ProviderDataset { sha256: format!("{:x}", digest.finalize()), provider_count })
}

pub fn read_name_association_dataset(
	path: &Path,
	mut read_association: impl FnMut(&NameAssociation) -> Result<()>,
) -> Result<NameAssociationDataset> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut reader = BufReader::new(file);
	let mut digest = Sha256::new();
	let mut previous = None::<NameAssociation>;
	let mut keys = BTreeSet::new();
	let mut association_count = 0;
	let mut bytes = Vec::new();

	loop {
		bytes.clear();
		if reader.read_until(b'\n', &mut bytes)? == 0 {
			break;
		}
		let line_number = association_count + 1;
		ensure!(bytes.ends_with(b"\n"), "{} line {line_number} has no final LF", path.display());
		digest.update(&bytes);
		bytes.pop();
		ensure!(!bytes.is_empty(), "{} line {line_number} is blank", path.display());

		let line = std::str::from_utf8(&bytes)
			.with_context(|| format!("{} line {line_number} is not UTF-8", path.display()))?;
		let association: NameAssociation =
			serde_json::from_str(line).with_context(|| format!("parse {} line {line_number}", path.display()))?;
		let canonical = serde_json::to_string(&association)?;
		ensure!(line == canonical, "{} line {line_number} is not canonical JSON", path.display());
		association
			.validate()
			.with_context(|| format!("validate {} line {line_number}", path.display()))?;
		ensure!(
			keys.insert((association.executable.clone(), association.project.clone())),
			"{} line {line_number} duplicates a name association for {} and {}",
			path.display(),
			association.executable,
			association.project
		);
		if let Some(previous) = previous.as_ref() {
			if association == *previous {
				bail!("{} line {line_number} duplicates the preceding name association", path.display());
			}
			ensure!(association > *previous, "{} line {line_number} is not sorted", path.display());
		}

		read_association(&association).with_context(|| format!("process {} line {line_number}", path.display()))?;
		previous = Some(association);
		association_count += 1;
	}

	Ok(NameAssociationDataset { sha256: format!("{:x}", digest.finalize()), association_count })
}

pub fn read_candidate_directory(path: &Path) -> Result<Vec<Candidate>> {
	let mut candidates = Vec::new();
	for entry in WalkDir::new(path).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", path.display()))?;
		if entry.file_type().is_file() && entry.path().extension().is_some_and(|value| value == "jsonl") {
			candidates.extend(read_candidates(entry.path())?);
		}
	}
	Ok(candidates)
}

pub fn read_candidates(path: &Path) -> Result<Vec<Candidate>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut reader = BufReader::new(file);
	let mut candidates = Vec::new();
	let mut previous = None::<Candidate>;
	let mut bytes = Vec::new();
	let mut line_number = 0;

	loop {
		bytes.clear();
		if reader.read_until(b'\n', &mut bytes)? == 0 {
			break;
		}
		line_number += 1;
		ensure!(bytes.ends_with(b"\n"), "{} line {line_number} has no final LF", path.display());
		bytes.pop();
		ensure!(!bytes.is_empty(), "{} line {line_number} is blank", path.display());
		let line = std::str::from_utf8(&bytes)
			.with_context(|| format!("{} line {line_number} is not UTF-8", path.display()))?;
		let candidate: Candidate =
			serde_json::from_str(line).with_context(|| format!("parse {} line {line_number}", path.display()))?;
		let canonical = serde_json::to_string(&candidate)?;
		ensure!(line == canonical, "{} line {line_number} is not canonical JSON", path.display());
		validate_candidate(&candidate).with_context(|| format!("validate {} line {line_number}", path.display()))?;
		if let Some(previous) = previous.as_ref() {
			ensure!(candidate > *previous, "{} line {line_number} is duplicated or not sorted", path.display());
		}
		previous = Some(candidate.clone());
		candidates.push(candidate);
	}

	Ok(candidates)
}

fn validate_candidate(candidate: &Candidate) -> Result<()> {
	validate_channel(&candidate.channel)?;
	validate_identity("package", &candidate.package)?;
	validate_identity("package version", &candidate.version)?;
	for (key, value) in &candidate.variant {
		validate_identity("variant key", key)?;
		validate_identity("variant value", value)?;
	}
	if let Some(digest) = &candidate.artifact_sha256 {
		validate_sha256("candidate artifact", digest)?;
	}
	validate_executable(&candidate.executable)?;
	ensure!(
		matches!(
			candidate.evidence.as_str(),
			"artifact-metadata" | "declared-entry-point" | "repository-file-listing" | "static-recipe-action"
		),
		"unknown evidence kind {}",
		candidate.evidence
	);
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::{NameAssociation, Provider, read_name_association_dataset, read_provider_dataset};

	const PROVIDERS: &str = concat!(
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n",
	);
	const NAMES: &str = concat!(
		"{\"executable\":\"ls\",\"project\":\"busybox\",\"role\":\"implementation\"}\n",
		"{\"executable\":\"ls\",\"project\":\"coreutils\",\"role\":\"reference\"}\n",
	);

	#[test]
	fn reads_and_hashes_canonical_provider_dataset() {
		let root = tempdir().unwrap();
		let path = root.path().join("providers.jsonl");
		fs::write(&path, PROVIDERS).unwrap();
		let mut providers = Vec::<Provider>::new();

		let dataset = read_provider_dataset(&path, |provider| {
			providers.push(provider.clone());
			Ok(())
		})
		.unwrap();

		assert_eq!(dataset.provider_count, 2);
		assert_eq!(dataset.sha256, "1a1d5b4857c31c3dd9b6d288c4184297a943a3f6213e582c983c89e7c346a074");
		assert_eq!(providers[0].package, "helix-term");
	}

	#[test]
	fn rejects_noncanonical_provider_dataset() {
		let cases = [
			("missing-final-lf", PROVIDERS.trim_end().to_owned()),
			("blank-line", "\n".to_owned()),
			(
				"whitespace",
				"{\"project\": \"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n"
					.to_owned(),
			),
			("duplicate", format!("{PROVIDERS}{PROVIDERS}")),
			(
				"empty-project",
				"{\"project\":\"\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n".to_owned(),
			),
			(
				"unknown-channel",
				"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"unknown\",\"package\":\"helix\"}\n"
					.to_owned(),
			),
		];
		for (name, value) in cases {
			let root = tempdir().unwrap();
			let path = root.path().join(format!("{name}.jsonl"));
			fs::write(&path, value).unwrap();
			assert!(read_provider_dataset(&path, |_| Ok(())).is_err(), "accepted {name}");
		}
	}

	#[test]
	fn reads_and_hashes_canonical_name_associations() {
		let root = tempdir().unwrap();
		let path = root.path().join("names.jsonl");
		fs::write(&path, NAMES).unwrap();
		let mut associations = Vec::<NameAssociation>::new();

		let dataset = read_name_association_dataset(&path, |association| {
			associations.push(association.clone());
			Ok(())
		})
		.unwrap();

		assert_eq!(dataset.association_count, 2);
		assert_eq!(associations[0].project, "busybox");
		assert_eq!(associations[1].role, "reference");
	}

	#[test]
	fn rejects_invalid_name_association_records() {
		let cases = [
			("unknown-role", "{\"executable\":\"ls\",\"project\":\"coreutils\",\"role\":\"owner\"}\n"),
			("unsafe-executable", "{\"executable\":\"bin/ls\",\"project\":\"coreutils\",\"role\":\"reference\"}\n"),
			(
				"unsorted",
				concat!(
					"{\"executable\":\"ls\",\"project\":\"coreutils\",\"role\":\"reference\"}\n",
					"{\"executable\":\"cat\",\"project\":\"coreutils\",\"role\":\"reference\"}\n",
				),
			),
			(
				"duplicate-project",
				concat!(
					"{\"executable\":\"ls\",\"project\":\"coreutils\",\"role\":\"implementation\"}\n",
					"{\"executable\":\"ls\",\"project\":\"coreutils\",\"role\":\"reference\"}\n",
				),
			),
		];
		for (name, value) in cases {
			let root = tempdir().unwrap();
			let path = root.path().join(format!("{name}.jsonl"));
			fs::write(&path, value).unwrap();
			assert!(read_name_association_dataset(&path, |_| Ok(())).is_err(), "accepted {name}");
		}
	}
}
