use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::acquisition::{AcquisitionContext, PackageObservation, SnapshotMetadata, file_identity, write_acquisition};
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
	context: Option<&AcquisitionContext>,
	previous: Option<&Path>,
) -> Result<usize> {
	let (in_release, boundary) = verify_release_files(packages, &scope)?;
	let directory = packages.parent().context("Debian package index has no parent directory")?;
	let inputs = [
		packages.to_owned(),
		sources.to_owned(),
		contents.to_owned(),
		in_release,
		directory.join("Packages.xz"),
		directory.join("Sources.xz"),
		directory.join("Contents.gz"),
		directory.join("Contents-all.gz"),
	];

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
	let mut metadata = SnapshotMetadata::complete("debian", &inputs, scope);
	metadata.boundary = boundary;
	metadata.context = context;
	metadata.previous = previous;
	write_acquisition(output, metadata, package_observations, candidates)
}

fn verify_release_files(
	packages: &Path,
	scope: &BTreeMap<String, String>,
) -> Result<(PathBuf, BTreeMap<String, String>)> {
	let directory = packages.parent().context("Debian package index has no parent directory")?;
	let in_release = directory.join("InRelease");
	ensure!(in_release.is_file(), "missing Debian InRelease beside {}", packages.display());
	let checksums = read_release_checksums(&in_release)?;
	let component = scope
		.get("component")
		.context("Debian release verification requires component scope")?;
	let architecture = scope
		.get("architecture")
		.context("Debian release verification requires architecture scope")?;
	let expected = [
		(format!("{component}/binary-{architecture}/Packages.xz"), directory.join("Packages.xz")),
		(format!("{component}/source/Sources.xz"), directory.join("Sources.xz")),
		(format!("{component}/Contents-{architecture}.gz"), directory.join("Contents.gz")),
		(format!("{component}/Contents-all.gz"), directory.join("Contents-all.gz")),
	];
	for (relative, path) in expected {
		let (expected_size, expected_sha256) = checksums
			.get(&relative)
			.with_context(|| format!("InRelease has no SHA-256 for {relative}"))?;
		let (size, sha256) = file_identity(&path)?;
		ensure!(size == *expected_size, "{} size does not match InRelease", path.display());
		ensure!(sha256 == *expected_sha256, "{} digest does not match InRelease", path.display());
	}
	let mut boundary = BTreeMap::new();
	if let Some(date) = fs::read_to_string(&in_release)?
		.lines()
		.find_map(|line| line.strip_prefix("Date: "))
		.map(str::trim)
		.filter(|date| !date.is_empty())
	{
		boundary.insert("release-date".to_owned(), date.to_owned());
	}
	Ok((in_release, boundary))
}

fn read_release_checksums(path: &Path) -> Result<BTreeMap<String, (u64, String)>> {
	let value = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
	let mut checksums = BTreeMap::new();
	for line in value.lines() {
		let Some(line) = line.strip_prefix(' ') else {
			continue;
		};
		let mut parts = line.split_whitespace();
		let (Some(sha256), Some(size), Some(relative), None) = (parts.next(), parts.next(), parts.next(), parts.next())
		else {
			continue;
		};
		if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
			continue;
		}
		let size = size
			.parse::<u64>()
			.with_context(|| format!("parse size for {relative} in {}", path.display()))?;
		let checksum = (size, sha256.to_ascii_lowercase());
		ensure!(
			checksums
				.insert(relative.to_owned(), checksum.clone())
				.is_none_or(|previous| previous == checksum),
			"InRelease has conflicting checksums for {relative}"
		);
	}
	ensure!(!checksums.is_empty(), "{} has no SHA-256 release checksums", path.display());
	Ok(checksums)
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
	use std::collections::BTreeMap;
	use std::fs;

	use sha2::{Digest, Sha256};
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
		write_release_fixture(root.path());

		import(
			&packages,
			&sources,
			&contents,
			&output,
			BTreeMap::from([
				("suite".to_owned(), "test".to_owned()),
				("component".to_owned(), "main".to_owned()),
				("architecture".to_owned(), "amd64".to_owned()),
			]),
			None,
			None,
		)
		.unwrap();
		let candidates = read_candidates(&output).unwrap();

		assert_eq!(candidates.len(), 1);
		assert_eq!(candidates[0].homepage.as_deref(), Some("https://github.com/BurntSushi/ripgrep"));
		assert_eq!(candidates[0].repository, None);
	}

	#[test]
	fn rejects_indexes_that_do_not_match_inrelease() {
		let root = tempdir().unwrap();
		let packages = root.path().join("Packages");
		let sources = root.path().join("Sources");
		let contents = root.path().join("Contents");
		let output = root.path().join("debian.jsonl");
		fs::write(&packages, "Package: ripgrep\nVersion: 1\n").unwrap();
		fs::write(&sources, "").unwrap();
		fs::write(&contents, "").unwrap();
		write_release_fixture(root.path());
		fs::write(root.path().join("Packages.xz"), "tampered").unwrap();

		let error = import(
			&packages,
			&sources,
			&contents,
			&output,
			BTreeMap::from([
				("suite".to_owned(), "test".to_owned()),
				("component".to_owned(), "main".to_owned()),
				("architecture".to_owned(), "amd64".to_owned()),
			]),
			None,
			None,
		)
		.unwrap_err();
		assert!(error.to_string().contains("does not match InRelease"));
	}

	fn write_release_fixture(root: &std::path::Path) {
		let files = [
			("main/binary-amd64/Packages.xz", "Packages.xz", b"packages".as_slice()),
			("main/source/Sources.xz", "Sources.xz", b"sources".as_slice()),
			("main/Contents-amd64.gz", "Contents.gz", b"contents".as_slice()),
			("main/Contents-all.gz", "Contents-all.gz", b"contents-all".as_slice()),
		];
		let mut release = "SHA256:\n".to_owned();
		for (relative, name, bytes) in files {
			fs::write(root.join(name), bytes).unwrap();
			release.push_str(&format!(" {:x} {} {relative}\n", Sha256::digest(bytes), bytes.len()));
		}
		fs::write(root.join("InRelease"), release).unwrap();
	}
}
