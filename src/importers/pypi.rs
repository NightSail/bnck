use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::model::{Candidate, write_json_lines};

const MAX_METADATA_SIZE: u64 = 4 * 1024 * 1024;

pub fn plan(simple: &Path, output_directory: &Path, output: &Path) -> Result<usize> {
	let file = File::open(simple).with_context(|| format!("open {}", simple.display()))?;
	let document: Value =
		serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", simple.display()))?;
	let versions = document
		.get("versions")
		.and_then(Value::as_array)
		.context("simple response has no versions")?;
	let version = versions
		.iter()
		.rev()
		.find_map(Value::as_str)
		.context("simple response has no versions")?;
	let files = document
		.get("files")
		.and_then(Value::as_array)
		.context("simple response has no files")?;
	let mut wheels: Vec<_> = files
		.iter()
		.filter(|file| !is_yanked(file))
		.filter_map(|file| planned_wheel(file, version, output_directory))
		.collect();
	wheels.sort_by_key(|wheel| (!wheel.is_platform_independent, wheel.size));
	let wheel = wheels.first().with_context(|| format!("no wheel for version {version}"))?;

	let file = File::create(output).with_context(|| format!("create {}", output.display()))?;
	let mut writer = BufWriter::new(file);
	writeln!(writer, "{}\t{}", wheel.url, wheel.path.display())?;
	writer.flush()?;
	Ok(1)
}

pub fn import(input: &Path, output: &Path) -> Result<usize> {
	let mut candidates = Vec::new();
	for entry in WalkDir::new(input).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", input.display()))?;
		if !entry.file_type().is_file() || entry.path().extension().is_none_or(|value| value != "whl") {
			continue;
		}
		candidates.extend(import_wheel(entry.path())?);
	}
	write_json_lines(output, candidates)
}

struct PlannedWheel {
	url: String,
	path: PathBuf,
	size: u64,
	is_platform_independent: bool,
}

fn planned_wheel(file: &Value, version: &str, output_directory: &Path) -> Option<PlannedWheel> {
	let filename = file.get("filename")?.as_str()?;
	if !filename.ends_with(".whl") || wheel_version(filename)? != version {
		return None;
	}
	let digest = file.pointer("/hashes/sha256")?.as_str()?;
	Some(PlannedWheel {
		url: file.get("url")?.as_str()?.to_owned(),
		path: output_directory.join(format!("{digest}.whl")),
		size: file.get("size").and_then(Value::as_u64).unwrap_or(u64::MAX),
		is_platform_independent: filename.ends_with("-none-any.whl"),
	})
}

fn wheel_version(filename: &str) -> Option<&str> {
	filename.strip_suffix(".whl")?.split('-').nth(1)
}

fn is_yanked(file: &Value) -> bool {
	match file.get("yanked") {
		Some(Value::Bool(value)) => *value,
		Some(Value::String(_)) => true,
		_ => false,
	}
}

fn import_wheel(path: &Path) -> Result<Vec<Candidate>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut archive = ZipArchive::new(file).with_context(|| format!("open wheel {}", path.display()))?;
	let metadata_name = member_name(&mut archive, ".dist-info/METADATA")?
		.with_context(|| format!("{} has no METADATA", path.display()))?;
	let metadata = read_member(&mut archive, &metadata_name)?;
	let fields = parse_metadata(&metadata);
	let package = field(&fields, "Name")?;
	let version = field(&fields, "Version")?;
	let homepage = first_field(&fields, "Home-page").or_else(|| project_url(&fields, &["homepage"]));
	let repository = project_url(&fields, &["source", "source code", "repository"]);
	let description = first_field(&fields, "Summary");
	let mut executable_evidence = Vec::new();

	if let Some(entry_points_name) = member_name(&mut archive, ".dist-info/entry_points.txt")? {
		let entry_points = read_member(&mut archive, &entry_points_name)?;
		for executable in parse_entry_points(&entry_points) {
			executable_evidence.push((executable, "declared-entry-point"));
		}
	}

	for index in 0..archive.len() {
		let member = archive.by_index(index)?;
		let name = member.name();
		if !member.is_dir()
			&& name.contains(".data/scripts/")
			&& let Some(executable) = name.rsplit('/').next().filter(|value| !value.is_empty())
		{
			executable_evidence.push((executable.to_owned(), "artifact-metadata"));
		}
	}

	Ok(executable_evidence
		.into_iter()
		.map(|(executable, evidence)| Candidate {
			channel: "pypi".into(),
			package: package.clone(),
			version: version.clone(),
			executable,
			evidence: evidence.into(),
			source_package: None,
			homepage: homepage.clone(),
			repository: repository.clone(),
			description: description.clone(),
		})
		.collect())
}

fn member_name(archive: &mut ZipArchive<File>, suffix: &str) -> Result<Option<String>> {
	for index in 0..archive.len() {
		let member = archive.by_index(index)?;
		if member.name().ends_with(suffix) {
			return Ok(Some(member.name().to_owned()));
		}
	}
	Ok(None)
}

fn read_member(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
	let member = archive.by_name(name)?;
	if member.size() > MAX_METADATA_SIZE {
		bail!("wheel metadata member {name} exceeds size limit")
	}
	let mut value = String::new();
	member.take(MAX_METADATA_SIZE + 1).read_to_string(&mut value)?;
	Ok(value)
}

fn parse_metadata(value: &str) -> HashMap<String, Vec<String>> {
	let mut fields = HashMap::<String, Vec<String>>::new();
	let mut current = None::<(String, usize)>;
	for line in value.lines() {
		if line.is_empty() {
			break;
		}
		if line.starts_with([' ', '\t']) {
			if let Some((field, index)) = current.as_ref()
				&& let Some(value) = fields.get_mut(field).and_then(|values| values.get_mut(*index))
			{
				value.push_str(line.trim());
			}
			continue;
		}
		let Some((name, value)) = line.split_once(':') else {
			continue;
		};
		let values = fields.entry(name.to_owned()).or_default();
		values.push(value.trim().to_owned());
		current = Some((name.to_owned(), values.len() - 1));
	}
	fields
}

fn field(fields: &HashMap<String, Vec<String>>, name: &str) -> Result<String> {
	first_field(fields, name).with_context(|| format!("METADATA has no {name}"))
}

fn first_field(fields: &HashMap<String, Vec<String>>, name: &str) -> Option<String> {
	fields.get(name)?.first().filter(|value| !value.is_empty()).cloned()
}

fn project_url(fields: &HashMap<String, Vec<String>>, labels: &[&str]) -> Option<String> {
	for value in fields.get("Project-URL")? {
		let Some((label, url)) = value.split_once(',') else {
			continue;
		};
		if labels.contains(&label.trim().to_ascii_lowercase().as_str()) {
			return Some(url.trim().to_owned());
		}
	}
	None
}

fn parse_entry_points(value: &str) -> Vec<String> {
	let mut is_script_group = false;
	let mut names = Vec::new();
	for line in value.lines().map(str::trim) {
		if line.starts_with('[') && line.ends_with(']') {
			is_script_group = matches!(line, "[console_scripts]" | "[gui_scripts]");
			continue;
		}
		if is_script_group && let Some((name, _)) = line.split_once('=') {
			let name = name.trim();
			if !name.is_empty() {
				names.push(name.to_owned());
			}
		}
	}
	names
}

#[cfg(test)]
mod tests {
	use super::{parse_entry_points, wheel_version};

	#[test]
	fn parses_wheel_versions() {
		assert_eq!(wheel_version("black-25.1.0-py3-none-any.whl"), Some("25.1.0"));
	}

	#[test]
	fn reads_script_entry_point_groups() {
		let value = "[console_scripts]\nblack = black:main\n[other]\nignored = package:main\n";
		assert_eq!(parse_entry_points(value), ["black"]);
	}
}
