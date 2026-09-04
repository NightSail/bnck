use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Candidate {
	pub channel: String,
	pub package: String,
	pub version: String,
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

#[cfg_attr(not(test), expect(dead_code, reason = "consumed by the next implementation stage"))]
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
	let reader = BufReader::new(file);
	let mut candidates = Vec::new();

	for (index, line) in reader.lines().enumerate() {
		let line = line.with_context(|| format!("read {} line {}", path.display(), index + 1))?;
		if line.is_empty() {
			continue;
		}
		let candidate =
			serde_json::from_str(&line).with_context(|| format!("parse {} line {}", path.display(), index + 1))?;
		candidates.push(candidate);
	}

	Ok(candidates)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::{Provider, read_provider_dataset};

	const PROVIDERS: &str = concat!(
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n",
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
		];
		for (name, value) in cases {
			let root = tempdir().unwrap();
			let path = root.path().join(format!("{name}.jsonl"));
			fs::write(&path, value).unwrap();
			assert!(read_provider_dataset(&path, |_| Ok(())).is_err(), "accepted {name}");
		}
	}
}
