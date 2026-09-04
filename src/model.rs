use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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
