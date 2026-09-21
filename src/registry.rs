#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::curation::{curated_packages, load_projects};
use crate::matcher::{normalize_name, normalize_url, repository_name};
use crate::model::validate_identity;

/* This narrows acquisition, never project ownership. Similar names still need curation. */
pub struct CuratedScope {
	pub packages: BTreeSet<String>,
	names: BTreeSet<String>,
	urls: BTreeSet<String>,
}

impl CuratedScope {
	pub fn load(root: &Path, channel: &str) -> Result<Self> {
		let packages = curated_packages(root, channel)?;
		let projects = load_projects(&root.join("projects"))?;
		let mut names = BTreeSet::new();
		let mut urls = BTreeSet::new();
		for project in projects.values() {
			let mut project_names = BTreeSet::from([normalize_name(&project.id), normalize_name(&project.name)]);
			if let Some(name) = project.repository.as_deref().and_then(repository_name) {
				project_names.insert(normalize_name(name));
			}
			for package in &project.packages {
				let name = package.name.rsplit('/').next().unwrap_or(&package.name);
				project_names.insert(normalize_name(name));
			}
			project_names.remove("");
			names.extend(project_names);
			for url in project
				.homepage
				.iter()
				.chain(project.repository.iter())
				.chain(project.links.iter())
			{
				urls.insert(normalize_url(url));
			}
		}
		names.remove("");
		urls.remove("");
		Ok(Self { packages, names, urls })
	}

	pub fn matches_name(&self, package: &str) -> bool {
		if self.packages.contains(package) {
			return true;
		}
		self.names.contains(&normalize_name(package))
	}

	pub fn matches_metadata(&self, package: &str, homepage: &str, repository: &str) -> bool {
		self.matches_name(package)
			|| [homepage, repository]
				.into_iter()
				.any(|url| self.urls.contains(&normalize_url(url)))
	}
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPlan {
	pub channel: String,
	pub inputs: Vec<PathBuf>,
	pub packages: Vec<MetadataPackage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPackage {
	pub name: String,
	pub path: PathBuf,
}

impl MetadataPlan {
	pub fn read(path: &Path, channel: &str) -> Result<Self> {
		let plan: Self = read_json(path)?;
		ensure!(plan.channel == channel, "metadata plan is for {}, not {channel}", plan.channel);
		let mut names = BTreeSet::new();
		let mut paths = BTreeSet::new();
		for package in &plan.packages {
			validate_identity("planned package", &package.name)?;
			ensure!(names.insert(&package.name), "duplicate planned package {}", package.name);
			ensure!(paths.insert(&package.path), "duplicate metadata path {}", package.path.display());
		}
		Ok(plan)
	}

	pub fn input_paths(&self, plan_path: &Path) -> Vec<PathBuf> {
		self.inputs
			.iter()
			.cloned()
			.chain(std::iter::once(plan_path.to_owned()))
			.chain(self.packages.iter().flat_map(|package| {
				let status = package.path.with_extension("status");
				std::iter::once(package.path.clone()).chain(status.is_file().then_some(status))
			}))
			.collect()
	}
}

#[derive(Deserialize)]
struct PypiIndex {
	projects: Vec<PypiProject>,
}

#[derive(Deserialize)]
struct PypiProject {
	name: String,
}

pub fn plan_npm(curations: &Path, directory: &Path, output: &Path) -> Result<usize> {
	let packages = curated_packages(curations, "npm")?;
	write_metadata_plan("npm", Vec::new(), packages, directory, output)
}

pub fn plan_metadata(channel: &str, index: &Path, curations: &Path, directory: &Path, output: &Path) -> Result<usize> {
	ensure!(channel == "pypi", "catalog metadata planning supports pypi; use plan-npm for npm");
	let selection = CuratedScope::load(curations, channel)?;
	let mut selected = BTreeSet::new();
	let catalog: PypiIndex = read_json(index)?;
	for project in catalog.projects {
		let name = normalize_pypi_name(&project.name)?;
		if selection.matches_name(&name) {
			selected.insert(name);
		}
	}
	for package in &selection.packages {
		ensure!(selected.contains(package), "curated {channel} package {package} is absent from registry index");
	}
	write_metadata_plan(channel, vec![std::path::absolute(index)?], selected, directory, output)
}

fn write_metadata_plan(
	channel: &str,
	inputs: Vec<PathBuf>,
	selected: BTreeSet<String>,
	directory: &Path,
	output: &Path,
) -> Result<usize> {
	let directory = std::path::absolute(directory)?;
	fs::create_dir_all(&directory)?;
	let packages = selected
		.into_iter()
		.map(|name| {
			let path = directory.join(format!("{:x}.json", Sha256::digest(name.as_bytes())));
			MetadataPackage { name, path }
		})
		.collect::<Vec<_>>();
	let plan = MetadataPlan { channel: channel.to_owned(), inputs, packages };
	write_json(output, &plan)?;
	let mut downloads = File::create(output.with_extension("tsv"))?;
	for package in &plan.packages {
		let name = encode_segment(&package.name);
		let url = if channel == "npm" {
			format!("https://registry.npmjs.org/{name}/latest")
		} else {
			format!("https://pypi.org/simple/{name}/")
		};
		write_download(&mut downloads, &url, &package.path)?;
	}
	Ok(plan.packages.len())
}

pub fn normalize_pypi_name(name: &str) -> Result<String> {
	ensure!(
		!name.is_empty()
			&& name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
			&& name.as_bytes()[0].is_ascii_alphanumeric()
			&& name.as_bytes()[name.len() - 1].is_ascii_alphanumeric(),
		"invalid PyPI project name {name:?}"
	);
	let mut normalized = String::new();
	for byte in name.bytes() {
		if b"._-".contains(&byte) {
			if !normalized.ends_with('-') {
				normalized.push('-');
			}
		} else {
			normalized.push(char::from(byte.to_ascii_lowercase()));
		}
	}
	Ok(normalized)
}

pub fn unavailable_metadata(path: &Path) -> Result<Option<String>> {
	let status_path = path.with_extension("status");
	if !status_path.exists() {
		return Ok(None);
	}
	let status = fs::read_to_string(&status_path)?;
	match status.trim() {
		"200" => Ok(None),
		"404" | "410" => Ok(Some(format!("http-{}", status.trim()))),
		status => anyhow::bail!("metadata request {} returned HTTP {status}", path.display()),
	}
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	serde_json::from_reader(BufReader::new(file)).with_context(|| format!("parse {}", path.display()))
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
	let mut bytes = serde_json::to_vec_pretty(value)?;
	bytes.push(b'\n');
	fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

pub fn write_download(writer: &mut impl Write, url: &str, path: &Path) -> Result<()> {
	ensure!(url.starts_with("https://") && !url.chars().any(char::is_control), "unsafe download URL {url:?}");
	let path = path.to_str().context("download path is not UTF-8")?;
	ensure!(!path.chars().any(char::is_control), "download path contains a control character");
	writeln!(writer, "{url}\t{path}")?;
	Ok(())
}

pub fn json_files(path: &Path) -> Result<Vec<PathBuf>> {
	if path.is_file() {
		return Ok(vec![path.to_owned()]);
	}
	let mut paths = fs::read_dir(path)
		.with_context(|| format!("read {}", path.display()))?
		.map(|entry| entry.map(|entry| entry.path()))
		.collect::<std::io::Result<Vec<_>>>()?;
	paths.retain(|path| path.is_file() && path.extension().is_some_and(|extension| extension == "json"));
	paths.sort();
	Ok(paths)
}

fn encode_segment(value: &str) -> String {
	let mut encoded = String::new();
	for byte in value.bytes() {
		if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
			encoded.push(char::from(byte));
		} else {
			use std::fmt::Write;
			write!(encoded, "%{byte:02X}").expect("write to string");
		}
	}
	encoded
}
