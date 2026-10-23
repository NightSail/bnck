use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail, ensure};
use pep440_rs::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::validate_sha256;
use crate::registry::{MetadataPlan, normalize_pypi_name, read_json, unavailable_metadata, write_download, write_json};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WheelPlan {
	pub inputs: Vec<PathBuf>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub serial: Option<u64>,
	pub packages: Vec<Selection>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Selection {
	Wheel { package: String, version: String, filename: String, url: String, sha256: String, path: PathBuf },
	Unsupported { package: String, version: Option<String>, reason: String },
}

pub fn plan(simple: &Path, directory: &Path, output: &Path) -> Result<usize> {
	let selection = select(simple, directory)?;
	let Selection::Wheel { url, path, .. } = selection else {
		bail!("{} has no supported current wheel", simple.display());
	};
	write_download(&mut File::create(output)?, &url, &path)?;
	Ok(1)
}

pub fn plan_snapshot(metadata_path: &Path, directory: &Path, output: &Path) -> Result<usize> {
	let metadata = MetadataPlan::read(metadata_path, "pypi")?;
	let mut packages = Vec::new();
	let mut serial = None;
	for package in &metadata.packages {
		if let Some(reason) = unavailable_metadata(&package.path)? {
			packages.push(Selection::Unsupported { package: package.name.clone(), version: None, reason });
			continue;
		}
		if let Some(value) = last_serial(&package.path)? {
			serial = Some(serial.unwrap_or(0).max(value));
		}
		let selection = select(&package.path, directory)?;
		let selected_name = match &selection {
			Selection::Wheel { package, .. } | Selection::Unsupported { package, .. } => package,
		};
		ensure!(selected_name == &package.name, "expected PyPI package {}, found {selected_name}", package.name);
		packages.push(selection);
	}
	let plan = WheelPlan { inputs: metadata.input_paths(metadata_path), serial, packages };
	write_json(output, &plan)?;
	let mut downloads = File::create(output.with_extension("tsv"))?;
	let mut count = 0;
	for package in &plan.packages {
		if let Selection::Wheel { url, path, .. } = package {
			write_download(&mut downloads, url, path)?;
			count += 1;
		}
	}
	Ok(count)
}

pub fn last_serial(path: &Path) -> Result<Option<u64>> {
	let document: Value = read_json(path)?;
	Ok(document.pointer("/meta/_last-serial").and_then(Value::as_u64))
}

pub fn select(simple: &Path, directory: &Path) -> Result<Selection> {
	let document: Value = read_json(simple)?;
	let package = normalize_pypi_name(
		document
			.get("name")
			.and_then(Value::as_str)
			.context("Simple response has no name")?,
	)?;
	let files = document
		.get("files")
		.and_then(Value::as_array)
		.context("Simple response has no files")?;
	/* Simple API versions and files are sets, not release-ordered arrays. */
	let releases = files
		.iter()
		.filter(|file| !is_yanked(file))
		.filter_map(|file| {
			let version = artifact_version(file.get("filename")?.as_str()?, &package)?;
			version.is_stable().then_some((version, file))
		})
		.collect::<Vec<_>>();
	let Some(version) = releases.iter().map(|(version, _)| version).max() else {
		return Ok(Selection::Unsupported {
			package,
			version: None,
			reason: "no-non-yanked-stable-release".to_owned(),
		});
	};
	let mut wheels = releases
		.iter()
		.filter(|(release, file)| {
			release == version && file["filename"].as_str().is_some_and(|name| name.ends_with(".whl"))
		})
		.map(|(_, file)| *file)
		.collect::<Vec<_>>();
	wheels.sort_by_key(|file| {
		let name = file["filename"].as_str().unwrap_or_default();
		(!name.ends_with("-none-any.whl"), file.get("size").and_then(Value::as_u64).unwrap_or(u64::MAX), name)
	});
	let Some(wheel) = wheels.first() else {
		return Ok(Selection::Unsupported {
			package,
			version: Some(version.to_string()),
			reason: "current-release-has-no-wheel".to_owned(),
		});
	};
	let sha256 = wheel
		.pointer("/hashes/sha256")
		.and_then(Value::as_str)
		.context("wheel has no SHA-256")?;
	validate_sha256("planned wheel", sha256)?;
	let filename = wheel["filename"].as_str().context("wheel has no filename")?.to_owned();
	let url = wheel.get("url").and_then(Value::as_str).context("wheel has no URL")?.to_owned();
	let path = std::path::absolute(directory)?.join(format!("{sha256}.whl"));
	/* Validate URLs even when callers don't write a download list. */
	write_download(&mut std::io::sink(), &url, &path)?;
	Ok(Selection::Wheel { package, version: version.to_string(), filename, url, sha256: sha256.to_owned(), path })
}

fn artifact_version(filename: &str, package: &str) -> Option<Version> {
	if let Some(stem) = filename.strip_suffix(".whl") {
		let parts = stem.split('-').collect::<Vec<_>>();
		if !(5..=6).contains(&parts.len()) || normalize_pypi_name(parts[0]).ok()? != package {
			return None;
		}
		return Version::from_str(parts[1]).ok();
	}
	let stem = filename.strip_suffix(".tar.gz").or_else(|| filename.strip_suffix(".zip"))?;
	/* Legacy sdists can contain hyphens in both the distribution name and version. */
	stem.match_indices('-').find_map(|(index, _)| {
		if normalize_pypi_name(&stem[..index]).ok()? != package {
			return None;
		}
		Version::from_str(&stem[index + 1..]).ok()
	})
}

fn is_yanked(file: &Value) -> bool {
	match file.get("yanked") {
		Some(Value::Bool(value)) => *value,
		Some(Value::String(_)) => true,
		_ => false,
	}
}
