use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};

use crate::acquisition::{PackageObservation, SnapshotMetadata, write_acquisition};
use crate::model::Candidate;

#[derive(Clone, Debug, Default)]
struct Package {
	name: String,
	version: String,
	source: Option<String>,
	homepage: Option<String>,
	description: Option<String>,
	architecture: Option<String>,
	artifact_sha256: Option<String>,
}

pub fn import(
	packages: &Path,
	sources: &Path,
	contents: &Path,
	output: &Path,
	scope: std::collections::BTreeMap<String, String>,
) -> Result<usize> {
	let mut inputs = vec![packages.to_owned(), sources.to_owned(), contents.to_owned()];
	if let Some(in_release) = packages
		.parent()
		.map(|parent| parent.join("InRelease"))
		.filter(|path| path.is_file())
	{
		inputs.push(in_release);
	}
	let source_metadata = read_sources(sources)?;
	let mut packages = read_packages(packages)?;
	for package in packages.values_mut() {
		if let Some(source) = package.source.as_ref().and_then(|source| source_metadata.get(source)) {
			package.homepage = package.homepage.take().or_else(|| source.homepage.clone());
		}
	}

	let mut candidates = Vec::new();
	for (executable, owners) in read_contents(contents)? {
		for owner in owners {
			let Some(package) = packages.get(&owner) else {
				continue;
			};
			candidates.push(Candidate {
				channel: "debian".into(),
				package: package.name.clone(),
				version: package.version.clone(),
				variant: package
					.architecture
					.iter()
					.map(|architecture| ("architecture".to_owned(), architecture.clone()))
					.collect(),
				artifact_sha256: package.artifact_sha256.clone(),
				executable: executable.clone(),
				evidence: "repository-file-listing".into(),
				source_package: package.source.clone(),
				homepage: package.homepage.clone(),
				repository: None,
				description: package.description.clone(),
			});
		}
	}
	let package_observations = packages
		.values()
		.map(|package| {
			let mut observation = PackageObservation::observed(&package.name, &package.version);
			if let Some(architecture) = &package.architecture {
				observation.variant.insert("architecture".to_owned(), architecture.clone());
			}
			observation.artifact_sha256 = package.artifact_sha256.clone();
			observation
		})
		.collect::<Vec<_>>();
	write_acquisition(output, SnapshotMetadata::complete("debian", &inputs, scope), package_observations, candidates)
}

fn read_packages(path: &Path) -> Result<BTreeMap<String, Package>> {
	let mut packages = BTreeMap::new();
	for fields in read_control(path)? {
		let Some(name) = fields.get("Package") else {
			continue;
		};
		let package = Package {
			name: name.clone(),
			version: fields.get("Version").cloned().unwrap_or_default(),
			source: Some(
				fields
					.get("Source")
					.map(|value| value.split_whitespace().next().unwrap_or(value))
					.unwrap_or(name)
					.to_owned(),
			),
			homepage: fields.get("Homepage").cloned(),
			description: fields.get("Description").cloned(),
			architecture: fields.get("Architecture").cloned(),
			artifact_sha256: fields.get("SHA256").cloned(),
		};
		packages.insert(name.clone(), package);
	}
	Ok(packages)
}

fn read_sources(path: &Path) -> Result<BTreeMap<String, Package>> {
	let mut sources = BTreeMap::new();
	for fields in read_control(path)? {
		let Some(name) = fields.get("Package") else {
			continue;
		};
		sources.insert(
			name.clone(),
			Package { name: name.clone(), homepage: fields.get("Homepage").cloned(), ..Package::default() },
		);
	}
	Ok(sources)
}

fn read_control(path: &Path) -> Result<Vec<BTreeMap<String, String>>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let reader = BufReader::new(file);
	let mut paragraphs = Vec::new();
	let mut fields = BTreeMap::<String, String>::new();
	let mut current = None::<String>;

	for line in reader.lines() {
		let line = line.with_context(|| format!("read {}", path.display()))?;
		if line.is_empty() {
			if !fields.is_empty() {
				paragraphs.push(std::mem::take(&mut fields));
			}
			current = None;
			continue;
		}
		if line.starts_with([' ', '\t']) {
			if let Some(value) = current.as_ref().and_then(|field| fields.get_mut(field)) {
				value.push('\n');
				value.push_str(line.trim());
			}
			continue;
		}
		let Some((name, value)) = line.split_once(':') else {
			continue;
		};
		current = Some(name.to_owned());
		fields.insert(name.to_owned(), value.trim().to_owned());
	}
	if !fields.is_empty() {
		paragraphs.push(fields);
	}
	Ok(paragraphs)
}

fn read_contents(path: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let reader = BufReader::new(file);
	let mut commands = BTreeMap::<String, BTreeSet<String>>::new();
	for line in reader.lines() {
		let line = line.with_context(|| format!("read {}", path.display()))?;
		let Some((path, owners)) = line.rsplit_once(char::is_whitespace) else {
			continue;
		};
		let Some(executable) = public_executable(path.trim()) else {
			continue;
		};
		for owner in owners.split(',') {
			let package = owner.trim().rsplit('/').next().unwrap_or(owner.trim());
			if !package.is_empty() {
				commands.entry(executable.to_owned()).or_default().insert(package.to_owned());
			}
		}
	}
	Ok(commands)
}

fn public_executable(path: &str) -> Option<&str> {
	let path = path.trim_start_matches('/');
	for prefix in ["usr/bin/", "usr/sbin/", "bin/", "sbin/"] {
		if let Some(name) = path.strip_prefix(prefix) {
			return (!name.is_empty() && !name.contains('/')).then_some(name);
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use std::fs;

	use tempfile::tempdir;

	use super::{import, public_executable};
	use crate::model::read_candidates;

	#[test]
	fn recognizes_only_direct_public_commands() {
		assert_eq!(public_executable("usr/bin/rg"), Some("rg"));
		assert_eq!(public_executable("/bin/rg"), Some("rg"));
		assert_eq!(public_executable("usr/lib/ripgrep/rg"), None);
		assert_eq!(public_executable("usr/bin/helpers/rg"), None);
	}

	#[test]
	fn ignores_debian_packaging_repositories() {
		let root = tempdir().unwrap();
		let packages = root.path().join("Packages");
		let sources = root.path().join("Sources");
		let contents = root.path().join("Contents");
		let output = root.path().join("debian.jsonl");
		fs::write(&packages, "Package: ripgrep\nVersion: 1\nSource: rust-ripgrep\n").unwrap();
		fs::write(
			&sources,
			concat!(
				"Package: rust-ripgrep\n",
				"Homepage: https://github.com/BurntSushi/ripgrep\n",
				"Vcs-Git: https://salsa.debian.org/rust-team/debcargo-conf.git [src/ripgrep]\n",
				"Vcs-Browser: https://salsa.debian.org/rust-team/debcargo-conf/src/ripgrep\n",
			),
		)
		.unwrap();
		fs::write(&contents, "usr/bin/rg utils/ripgrep\n").unwrap();

		import(
			&packages,
			&sources,
			&contents,
			&output,
			std::collections::BTreeMap::from([("suite".to_owned(), "test".to_owned())]),
		)
		.unwrap();
		let candidates = read_candidates(&output).unwrap();

		assert_eq!(candidates.len(), 1);
		assert_eq!(candidates[0].homepage.as_deref(), Some("https://github.com/BurntSushi/ripgrep"));
		assert_eq!(candidates[0].repository, None);
	}
}
