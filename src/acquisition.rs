use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{Candidate, read_candidates, validate_channel, validate_identity, validate_sha256, write_json_lines};

pub const ACQUISITION_FORMAT: &str = "bnck-channel-snapshot";
pub const ACQUISITION_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageObservation {
	pub name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub variant: BTreeMap<String, String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub artifact_sha256: Option<String>,
	#[serde(default)]
	pub status: PackageStatus,
}

impl PackageObservation {
	pub fn observed(name: impl Into<String>, version: impl Into<String>) -> Self {
		Self {
			name: name.into(),
			version: Some(version.into()),
			variant: BTreeMap::new(),
			artifact_sha256: None,
			status: PackageStatus::Observed,
		}
	}

	fn validate(&self) -> Result<()> {
		validate_identity("package", &self.name)?;
		if let Some(version) = &self.version {
			validate_identity("package version", version)?;
		} else {
			ensure!(self.status != PackageStatus::Observed, "observed package {} has no version", self.name);
		}
		for (key, value) in &self.variant {
			validate_identity("variant key", key)?;
			validate_identity("variant value", value)?;
		}
		if let Some(digest) = &self.artifact_sha256 {
			validate_sha256("package artifact", digest)?;
		}
		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageStatus {
	#[default]
	Observed,
	Failed,
	Removed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotInput {
	pub path: String,
	pub size: u64,
	pub sha256: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub uri: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateArtifact {
	pub path: String,
	pub size: u64,
	pub sha256: String,
	pub candidate_count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotStatus {
	Complete,
	Partial,
	Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSnapshot {
	pub format: String,
	pub format_version: u32,
	pub snapshot: String,
	pub channel: String,
	pub status: SnapshotStatus,
	pub scope: BTreeMap<String, String>,
	pub inputs: Vec<SnapshotInput>,
	pub candidates: CandidateArtifact,
	pub packages: Vec<PackageObservation>,
	pub importer_version: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub importer_revision: Option<String>,
}

impl ChannelSnapshot {
	pub fn validate(&self, manifest_path: &Path) -> Result<()> {
		ensure!(self.format == ACQUISITION_FORMAT, "unsupported acquisition format {}", self.format);
		ensure!(
			self.format_version == ACQUISITION_FORMAT_VERSION,
			"unsupported acquisition format version {}",
			self.format_version
		);
		validate_sha256("snapshot", &self.snapshot)?;
		validate_channel(&self.channel)?;
		validate_identity("importer version", &self.importer_version)?;
		for (key, value) in &self.scope {
			validate_identity("scope key", key)?;
			validate_identity("scope value", value)?;
		}
		for input in &self.inputs {
			validate_identity("snapshot input path", &input.path)?;
			validate_sha256("snapshot input", &input.sha256)?;
			let input_path = Path::new(&input.path);
			let (size, sha256) = file_identity(input_path)?;
			ensure!(size == input.size, "{} size does not match its snapshot manifest", input_path.display());
			ensure!(sha256 == input.sha256, "{} digest does not match its snapshot manifest", input_path.display());
		}
		let package_set = self.packages.iter().cloned().collect::<BTreeSet<_>>();
		ensure!(package_set.len() == self.packages.len(), "snapshot has duplicate package observations");
		for package in &package_set {
			package.validate()?;
		}
		validate_identity("candidate artifact path", &self.candidates.path)?;
		validate_sha256("candidate artifact", &self.candidates.sha256)?;

		let candidate_path = manifest_path.parent().unwrap_or(Path::new(".")).join(&self.candidates.path);
		let (size, sha256) = file_identity(&candidate_path)?;
		ensure!(size == self.candidates.size, "{} size does not match its snapshot manifest", candidate_path.display());
		ensure!(
			sha256 == self.candidates.sha256,
			"{} digest does not match its snapshot manifest",
			candidate_path.display()
		);
		let candidates = read_candidates(&candidate_path)?;
		ensure!(
			candidates.len() == self.candidates.candidate_count,
			"{} record count does not match its snapshot manifest",
			candidate_path.display()
		);
		ensure!(
			candidates.iter().all(|candidate| candidate.channel == self.channel),
			"{} contains a candidate for another channel",
			candidate_path.display()
		);
		let observed_releases = package_set
			.iter()
			.filter(|package| package.status == PackageStatus::Observed)
			.map(package_release_key)
			.collect::<BTreeSet<_>>();
		for candidate in &candidates {
			ensure!(
				observed_releases.contains(&candidate_release_key(candidate)),
				"candidate package release {} {} is not observed by its snapshot",
				candidate.package,
				candidate.version
			);
		}
		let expected_snapshot = snapshot_identity(self);
		ensure!(self.snapshot == expected_snapshot, "snapshot identity does not match manifest content");
		Ok(())
	}
}

pub struct SnapshotMetadata<'a> {
	pub channel: &'a str,
	pub status: SnapshotStatus,
	pub scope: BTreeMap<String, String>,
	pub inputs: &'a [PathBuf],
	pub importer_revision: Option<String>,
}

impl<'a> SnapshotMetadata<'a> {
	pub fn complete(channel: &'a str, inputs: &'a [PathBuf], scope: BTreeMap<String, String>) -> Self {
		Self {
			channel,
			status: SnapshotStatus::Complete,
			scope,
			inputs,
			importer_revision: std::env::var("BNCK_SOURCE_REVISION")
				.ok()
				.or_else(|| option_env!("BNCK_SOURCE_REVISION").map(str::to_owned)),
		}
	}
}

pub fn write_acquisition(
	candidate_path: &Path,
	metadata: SnapshotMetadata<'_>,
	packages: impl IntoIterator<Item = PackageObservation>,
	candidates: Vec<Candidate>,
) -> Result<usize> {
	validate_channel(metadata.channel)?;
	ensure!(!metadata.scope.is_empty(), "acquisition scope is empty");
	let count = write_json_lines(candidate_path, candidates)?;
	if let Err(error) = write_snapshot_manifest(candidate_path, metadata, packages) {
		if let Err(cleanup_error) = fs::remove_file(candidate_path) {
			return Err(error
				.context(format!("also failed to remove incomplete {}: {cleanup_error}", candidate_path.display())));
		}
		return Err(error);
	}
	Ok(count)
}

pub fn write_snapshot_manifest(
	candidate_path: &Path,
	metadata: SnapshotMetadata<'_>,
	packages: impl IntoIterator<Item = PackageObservation>,
) -> Result<PathBuf> {
	validate_channel(metadata.channel)?;
	ensure!(!metadata.scope.is_empty(), "acquisition scope is empty");
	let package_set = packages.into_iter().collect::<BTreeSet<_>>();
	for package in &package_set {
		package.validate()?;
	}
	let package_releases = package_set.iter().map(package_release_key).collect::<BTreeSet<_>>();
	let candidates = read_candidates(candidate_path)?;
	ensure!(
		candidates.iter().all(|candidate| candidate.channel == metadata.channel),
		"{} contains a candidate for another channel",
		candidate_path.display()
	);
	for candidate in &candidates {
		ensure!(
			package_releases.contains(&candidate_release_key(candidate)),
			"candidate package release {} {} has no package observation",
			candidate.package,
			candidate.version
		);
	}

	let inputs = metadata
		.inputs
		.iter()
		.map(|path| {
			let (size, sha256) = file_identity(path)?;
			Ok(SnapshotInput { path: path.display().to_string(), size, sha256, uri: None })
		})
		.collect::<Result<Vec<_>>>()?;
	let (candidate_size, candidate_sha256) = file_identity(candidate_path)?;
	let candidate_filename = candidate_path
		.file_name()
		.and_then(|value| value.to_str())
		.context("candidate output has no UTF-8 filename")?
		.to_owned();
	let candidates = CandidateArtifact {
		path: candidate_filename,
		size: candidate_size,
		sha256: candidate_sha256,
		candidate_count: candidates.len(),
	};
	let mut manifest = ChannelSnapshot {
		format: ACQUISITION_FORMAT.to_owned(),
		format_version: ACQUISITION_FORMAT_VERSION,
		snapshot: String::new(),
		channel: metadata.channel.to_owned(),
		status: metadata.status,
		scope: metadata.scope,
		inputs,
		candidates,
		packages: package_set.into_iter().collect(),
		importer_version: env!("CARGO_PKG_VERSION").to_owned(),
		importer_revision: metadata.importer_revision,
	};
	manifest.snapshot = snapshot_identity(&manifest);
	let manifest_path = snapshot_manifest_path(candidate_path)?;
	let mut bytes = serde_json::to_vec_pretty(&manifest)?;
	bytes.push(b'\n');
	fs::write(&manifest_path, bytes).with_context(|| format!("write {}", manifest_path.display()))?;
	Ok(manifest_path)
}

pub fn read_snapshot_directory(path: &Path) -> Result<Vec<(PathBuf, ChannelSnapshot)>> {
	let mut manifests = Vec::new();
	let mut snapshot_ids = BTreeSet::new();
	for entry in WalkDir::new(path).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", path.display()))?;
		if !entry.file_type().is_file() || !entry.file_name().to_string_lossy().ends_with(".manifest.json") {
			continue;
		}
		let manifest_path = entry.into_path();
		let file = File::open(&manifest_path).with_context(|| format!("open {}", manifest_path.display()))?;
		let manifest: ChannelSnapshot = serde_json::from_reader(BufReader::new(file))
			.with_context(|| format!("parse {}", manifest_path.display()))?;
		manifest.validate(&manifest_path)?;
		ensure!(snapshot_ids.insert(manifest.snapshot.clone()), "duplicate snapshot ID {}", manifest.snapshot);
		manifests.push((manifest_path, manifest));
	}
	manifests.sort_by(|left, right| left.0.cmp(&right.0));
	Ok(manifests)
}

type PackageReleaseKey<'a> = (&'a str, Option<&'a str>, &'a BTreeMap<String, String>, Option<&'a str>);

fn package_release_key(package: &PackageObservation) -> PackageReleaseKey<'_> {
	(&package.name, package.version.as_deref(), &package.variant, package.artifact_sha256.as_deref())
}

fn candidate_release_key(candidate: &Candidate) -> PackageReleaseKey<'_> {
	(&candidate.package, Some(candidate.version.as_str()), &candidate.variant, candidate.artifact_sha256.as_deref())
}

fn snapshot_manifest_path(candidate_path: &Path) -> Result<PathBuf> {
	let filename = candidate_path
		.file_name()
		.and_then(|value| value.to_str())
		.context("candidate output has no UTF-8 filename")?;
	Ok(candidate_path.with_file_name(format!("{filename}.manifest.json")))
}

fn snapshot_identity(snapshot: &ChannelSnapshot) -> String {
	let mut digest = Sha256::new();
	for value in [
		serde_json::to_vec(&snapshot.channel).expect("serialize channel"),
		serde_json::to_vec(&snapshot.status).expect("serialize status"),
		serde_json::to_vec(&snapshot.scope).expect("serialize scope"),
		serde_json::to_vec(&snapshot.inputs).expect("serialize inputs"),
		serde_json::to_vec(&snapshot.candidates).expect("serialize candidate artifact"),
		serde_json::to_vec(&snapshot.packages).expect("serialize package observations"),
		serde_json::to_vec(&snapshot.importer_version).expect("serialize importer version"),
		serde_json::to_vec(&snapshot.importer_revision).expect("serialize importer revision"),
	] {
		digest.update(value);
		digest.update([0]);
	}
	format!("{:x}", digest.finalize())
}

pub fn file_identity(path: &Path) -> Result<(u64, String)> {
	if path.is_dir() {
		return directory_identity(path);
	}
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let size = file.metadata()?.len();
	let mut reader = BufReader::new(file);
	let mut digest = Sha256::new();
	hash_reader(&mut reader, &mut digest)?;
	Ok((size, format!("{:x}", digest.finalize())))
}

fn directory_identity(path: &Path) -> Result<(u64, String)> {
	let mut paths = WalkDir::new(path)
		.follow_links(false)
		.into_iter()
		.filter_map(|entry| match entry {
			Ok(entry) if entry.file_type().is_file() => Some(Ok(entry.into_path())),
			Ok(_) => None,
			Err(error) => Some(Err(error)),
		})
		.collect::<Result<Vec<_>, _>>()?;
	paths.sort();
	let mut total_size = 0;
	let mut digest = Sha256::new();
	for file_path in paths {
		let relative = file_path.strip_prefix(path).unwrap_or(&file_path);
		let relative = relative.to_str().context("snapshot input path is not UTF-8")?;
		digest.update(relative.as_bytes());
		digest.update([0]);
		let file = File::open(&file_path).with_context(|| format!("open {}", file_path.display()))?;
		total_size += file.metadata()?.len();
		hash_reader(&mut BufReader::new(file), &mut digest)?;
		digest.update([0]);
	}
	Ok((total_size, format!("{:x}", digest.finalize())))
}

fn hash_reader(reader: &mut impl Read, digest: &mut Sha256) -> Result<()> {
	let mut buffer = [0_u8; 64 * 1024];
	loop {
		let count = reader.read(&mut buffer)?;
		if count == 0 {
			break;
		}
		digest.update(&buffer[..count]);
	}
	Ok(())
}
