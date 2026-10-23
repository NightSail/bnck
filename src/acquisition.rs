use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{Candidate, read_candidates, validate_channel, validate_identity, validate_sha256, write_json_lines};

pub const ACQUISITION_FORMAT: &str = "bnck-channel-snapshot";
pub const ACQUISITION_FORMAT_VERSION: u32 = 2;
const CONTEXT_FORMAT: &str = "bnck-acquisition-context";
const CONTEXT_FORMAT_VERSION: &str = "1";
const SIGNATURE_RESULTS: [&str; 3] = ["verified", "failed", "not-available"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotValidationMode {
	Full,
	CandidateOnly,
}

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
	#[serde(skip_serializing_if = "Option::is_none")]
	pub media_type: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub retrieved_at: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub signature: Option<InputSignature>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputSignature {
	pub kind: String,
	pub result: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub signer: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotMetrics {
	pub input_count: usize,
	pub input_bytes: u64,
	pub observed_packages: usize,
	pub failed_packages: usize,
	pub removed_packages: usize,
	pub candidate_count: usize,
	pub candidate_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct InputContext {
	pub uri: Option<String>,
	pub media_type: Option<String>,
	pub retrieved_at: Option<String>,
	pub signature: Option<InputSignature>,
}

#[derive(Clone, Debug, Default)]
pub struct AcquisitionContext {
	inputs: BTreeMap<PathBuf, InputContext>,
	boundary: BTreeMap<String, String>,
}

impl AcquisitionContext {
	pub fn read(path: &Path) -> Result<Self> {
		let value = fs::read_to_string(path).with_context(|| format!("read acquisition context {}", path.display()))?;
		let mut lines = value.lines();
		let header = lines.next().context("acquisition context is empty")?;
		ensure!(
			header == format!("{CONTEXT_FORMAT}\t{CONTEXT_FORMAT_VERSION}"),
			"unsupported acquisition context header {header:?}"
		);
		let mut context = Self::default();
		for (index, line) in lines.enumerate() {
			let line_number = index + 2;
			ensure!(!line.is_empty(), "{} line {line_number} is blank", path.display());
			let fields = line.split('\t').collect::<Vec<_>>();
			match fields.first().copied() {
				Some("input") => {
					ensure!(fields.len() == 8, "{} line {line_number} input must have 8 fields", path.display());
					let input_path = std::path::absolute(fields[1])
						.with_context(|| format!("resolve {} line {line_number} input path", path.display()))?;
					let retrieved_at = optional(fields[4]);
					if let Some(value) = &retrieved_at {
						validate_timestamp(value).with_context(|| {
							format!("validate {} line {line_number} retrieval timestamp", path.display())
						})?;
					}
					let signature = signature(fields[5], fields[6], fields[7])
						.with_context(|| format!("validate {} line {line_number} signature", path.display()))?;
					let input = InputContext {
						uri: optional(fields[2]),
						media_type: optional(fields[3]),
						retrieved_at,
						signature,
					};
					if let Some(uri) = &input.uri {
						ensure!(
							(uri.starts_with("https://") || uri.starts_with("http://"))
								&& !uri.chars().any(char::is_control),
							"{} line {line_number} has an unsafe input URI {uri:?}",
							path.display()
						);
					}
					if let Some(media_type) = &input.media_type {
						validate_identity("input media type", media_type)?;
					}
					ensure!(
						context.inputs.insert(input_path, input).is_none(),
						"{} line {line_number} duplicates an input",
						path.display()
					);
				},
				Some("boundary") => {
					ensure!(fields.len() == 3, "{} line {line_number} boundary must have 3 fields", path.display());
					validate_identity("boundary key", fields[1])?;
					validate_identity("boundary value", fields[2])?;
					ensure!(
						context.boundary.insert(fields[1].to_owned(), fields[2].to_owned()).is_none(),
						"{} line {line_number} duplicates boundary key {}",
						path.display(),
						fields[1]
					);
				},
				Some(kind) => anyhow::bail!("{} line {line_number} has unknown kind {kind:?}", path.display()),
				None => anyhow::bail!("{} line {line_number} is empty", path.display()),
			}
		}
		Ok(context)
	}

	fn input(&self, path: &Path) -> Result<Option<&InputContext>> {
		let path = std::path::absolute(path)?;
		Ok(self.inputs.get(&path))
	}
}

fn optional(value: &str) -> Option<String> {
	(!value.is_empty()).then(|| value.to_owned())
}

fn signature(kind: &str, result: &str, signer: &str) -> Result<Option<InputSignature>> {
	if kind.is_empty() && result.is_empty() && signer.is_empty() {
		return Ok(None);
	}
	ensure!(!kind.is_empty() && !result.is_empty(), "signature kind and result must both be set");
	ensure!(SIGNATURE_RESULTS.contains(&result), "signature result must be one of {}", SIGNATURE_RESULTS.join(", "));
	validate_identity("signature kind", kind)?;
	validate_identity("signature result", result)?;
	let signer = optional(signer);
	if let Some(signer) = &signer {
		validate_identity("signature signer", signer)?;
	}
	Ok(Some(InputSignature { kind: kind.to_owned(), result: result.to_owned(), signer }))
}

fn validate_timestamp(value: &str) -> Result<()> {
	ensure!(value.len() == 20 && value.ends_with('Z'), "timestamp must use YYYY-MM-DDTHH:MM:SSZ");
	let bytes = value.as_bytes();
	ensure!(
		bytes[4] == b'-' && bytes[7] == b'-' && bytes[10] == b'T' && bytes[13] == b':' && bytes[16] == b':',
		"timestamp must use YYYY-MM-DDTHH:MM:SSZ"
	);
	for range in [0..4, 5..7, 8..10, 11..13, 14..16, 17..19] {
		ensure!(bytes[range].iter().all(u8::is_ascii_digit), "timestamp is not numeric");
	}
	let parse = |range: std::ops::Range<usize>| value[range].parse::<u32>();
	let (year, month, day, hour, minute, second) =
		(parse(0..4)?, parse(5..7)?, parse(8..10)?, parse(11..13)?, parse(14..16)?, parse(17..19)?);
	ensure!(year > 0 && (1..=12).contains(&month) && (1..=31).contains(&day), "timestamp date is out of range");
	ensure!(hour < 24 && minute < 60 && second < 60, "timestamp time is out of range");
	Ok(())
}

fn file_retrieved_at(path: &Path) -> Option<String> {
	let modified = fs::metadata(path).ok()?.modified().ok()?;
	Some(format_system_time(modified))
}

fn format_system_time(value: SystemTime) -> String {
	let seconds = value.duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0);
	let days = (seconds / 86_400) as i64;
	let remaining = seconds % 86_400;
	let (year, month, day) = civil_from_days(days);
	format!(
		"{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
		remaining / 3_600,
		(remaining % 3_600) / 60,
		remaining % 60
	)
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
	let days = days + 719_468;
	let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
	let day_of_era = days - era * 146_097;
	let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
	let year = year_of_era + era * 400;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_prime = (5 * day_of_year + 2) / 153;
	let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
	let month = month_prime + if month_prime < 10 { 3 } else { -9 };
	(year + i64::from(month <= 2), month as u32, day as u32)
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
	pub boundary: BTreeMap<String, String>,
	pub inputs: Vec<SnapshotInput>,
	pub candidates: CandidateArtifact,
	pub packages: Vec<PackageObservation>,
	pub metrics: SnapshotMetrics,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub failure: Option<String>,
	pub importer_version: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub importer_revision: Option<String>,
}

impl ChannelSnapshot {
	pub fn validate(&self, manifest_path: &Path, mode: SnapshotValidationMode) -> Result<()> {
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
		for (key, value) in &self.boundary {
			validate_identity("boundary key", key)?;
			validate_identity("boundary value", value)?;
		}
		ensure!(
			(self.status == SnapshotStatus::Failed) == self.failure.is_some(),
			"failed snapshots require a failure and other snapshots must not have one"
		);
		if let Some(failure) = &self.failure {
			validate_identity("snapshot failure", failure)?;
		}
		for input in &self.inputs {
			validate_identity("snapshot input path", &input.path)?;
			validate_sha256("snapshot input", &input.sha256)?;
			if let Some(uri) = &input.uri {
				ensure!(
					(uri.starts_with("https://") || uri.starts_with("http://")) && !uri.chars().any(char::is_control),
					"snapshot input URI is unsafe"
				);
			}
			if let Some(media_type) = &input.media_type {
				validate_identity("input media type", media_type)?;
			}
			if let Some(retrieved_at) = &input.retrieved_at {
				validate_timestamp(retrieved_at)?;
			}
			if let Some(signature) = &input.signature {
				validate_identity("signature kind", &signature.kind)?;
				validate_identity("signature result", &signature.result)?;
				ensure!(SIGNATURE_RESULTS.contains(&signature.result.as_str()), "unknown signature result");
				if let Some(signer) = &signature.signer {
					validate_identity("signature signer", signer)?;
				}
			}
			if mode == SnapshotValidationMode::Full {
				let input_path = Path::new(&input.path);
				let (size, sha256) = file_identity(input_path)?;
				ensure!(size == input.size, "{} size does not match its snapshot manifest", input_path.display());
				ensure!(sha256 == input.sha256, "{} digest does not match its snapshot manifest", input_path.display());
			}
		}
		let package_set = self.packages.iter().cloned().collect::<BTreeSet<_>>();
		ensure!(package_set.len() == self.packages.len(), "snapshot has duplicate package observations");
		for package in &package_set {
			package.validate()?;
		}
		validate_identity("candidate artifact path", &self.candidates.path)?;
		validate_sha256("candidate artifact", &self.candidates.sha256)?;

		let metrics = SnapshotMetrics {
			input_count: self.inputs.len(),
			input_bytes: self.inputs.iter().map(|input| input.size).sum(),
			observed_packages: self
				.packages
				.iter()
				.filter(|package| package.status == PackageStatus::Observed)
				.count(),
			failed_packages: self
				.packages
				.iter()
				.filter(|package| package.status == PackageStatus::Failed)
				.count(),
			removed_packages: self
				.packages
				.iter()
				.filter(|package| package.status == PackageStatus::Removed)
				.count(),
			candidate_count: self.candidates.candidate_count,
			candidate_bytes: self.candidates.size,
		};
		ensure!(self.metrics == metrics, "snapshot metrics do not match manifest content");

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
	pub boundary: BTreeMap<String, String>,
	pub inputs: &'a [PathBuf],
	pub context: Option<&'a AcquisitionContext>,
	pub previous: Option<&'a Path>,
	pub failure: Option<String>,
	pub importer_revision: Option<String>,
}

impl<'a> SnapshotMetadata<'a> {
	pub fn complete(channel: &'a str, inputs: &'a [PathBuf], scope: BTreeMap<String, String>) -> Self {
		Self {
			channel,
			status: SnapshotStatus::Complete,
			scope,
			boundary: BTreeMap::new(),
			inputs,
			context: None,
			previous: None,
			failure: None,
			importer_revision: std::env::var("BNCK_SOURCE_REVISION")
				.ok()
				.or_else(|| option_env!("BNCK_SOURCE_REVISION").map(str::to_owned)),
		}
	}
}

pub fn write_failed_snapshot(
	candidate_path: &Path,
	channel: &str,
	scope: BTreeMap<String, String>,
	inputs: &[PathBuf],
	context: Option<&AcquisitionContext>,
	error: &anyhow::Error,
) -> Result<()> {
	let inputs = inputs.iter().filter(|path| path.exists()).cloned().collect::<Vec<_>>();
	let mut metadata = SnapshotMetadata::complete(channel, &inputs, scope);
	metadata.status = SnapshotStatus::Failed;
	metadata.context = context;
	metadata.failure = Some(sanitize_failure(&error.to_string())?);
	write_acquisition(candidate_path, metadata, std::iter::empty(), Vec::new())?;
	Ok(())
}

fn sanitize_failure(value: &str) -> Result<String> {
	let value = value
		.chars()
		.map(|character| if character.is_control() { ' ' } else { character })
		.collect::<String>()
		.trim()
		.to_owned();
	ensure!(!value.is_empty(), "snapshot failure message is empty");
	validate_identity("snapshot failure", &value)?;
	Ok(value)
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

fn add_removed_packages(
	mut packages: Vec<PackageObservation>,
	metadata: &SnapshotMetadata<'_>,
) -> Result<Vec<PackageObservation>> {
	let Some(previous) = metadata.previous else {
		return Ok(packages);
	};
	if metadata.status != SnapshotStatus::Complete {
		return Ok(packages);
	}
	let snapshots = read_snapshot_directory_with_mode(previous, SnapshotValidationMode::CandidateOnly)?;
	let previous_names = snapshots
		.iter()
		.filter(|(_, snapshot)| {
			snapshot.channel == metadata.channel
				&& snapshot.scope == metadata.scope
				&& snapshot.status == SnapshotStatus::Complete
		})
		.flat_map(|(_, snapshot)| {
			snapshot
				.packages
				.iter()
				.filter(|package| package.status == PackageStatus::Observed)
				.map(|package| package.name.clone())
		})
		.collect::<BTreeSet<_>>();
	if previous_names.is_empty() {
		return Ok(packages);
	}
	let current_names = packages.iter().map(|package| package.name.clone()).collect::<BTreeSet<_>>();
	for name in previous_names {
		if !current_names.contains(&name) {
			packages.push(PackageObservation {
				name,
				version: None,
				variant: BTreeMap::new(),
				artifact_sha256: None,
				status: PackageStatus::Removed,
			});
		}
	}
	Ok(packages)
}

pub fn write_snapshot_manifest(
	candidate_path: &Path,
	metadata: SnapshotMetadata<'_>,
	packages: impl IntoIterator<Item = PackageObservation>,
) -> Result<PathBuf> {
	validate_channel(metadata.channel)?;
	ensure!(!metadata.scope.is_empty(), "acquisition scope is empty");
	let packages = add_removed_packages(packages.into_iter().collect(), &metadata)?;
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
			let input_context = metadata.context.map(|context| context.input(path)).transpose()?.flatten();
			let retrieved_at = input_context
				.and_then(|context| context.retrieved_at.clone())
				.or_else(|| file_retrieved_at(path));
			Ok(SnapshotInput {
				path: recorded_input_path(path)?,
				size,
				sha256,
				uri: input_context.and_then(|context| context.uri.clone()),
				media_type: input_context.and_then(|context| context.media_type.clone()),
				retrieved_at,
				signature: input_context.and_then(|context| context.signature.clone()),
			})
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
	let mut boundary = metadata.boundary;
	if let Some(context) = metadata.context {
		for (key, value) in &context.boundary {
			ensure!(
				boundary
					.insert(key.clone(), value.clone())
					.is_none_or(|previous| previous == *value),
				"conflicting boundary value for {key}"
			);
		}
	}
	let metrics = SnapshotMetrics {
		input_count: inputs.len(),
		input_bytes: inputs.iter().map(|input| input.size).sum(),
		observed_packages: package_set
			.iter()
			.filter(|package| package.status == PackageStatus::Observed)
			.count(),
		failed_packages: package_set
			.iter()
			.filter(|package| package.status == PackageStatus::Failed)
			.count(),
		removed_packages: package_set
			.iter()
			.filter(|package| package.status == PackageStatus::Removed)
			.count(),
		candidate_count: candidates.candidate_count,
		candidate_bytes: candidates.size,
	};
	let mut manifest = ChannelSnapshot {
		format: ACQUISITION_FORMAT.to_owned(),
		format_version: ACQUISITION_FORMAT_VERSION,
		snapshot: String::new(),
		channel: metadata.channel.to_owned(),
		status: metadata.status,
		scope: metadata.scope,
		boundary,
		inputs,
		candidates,
		packages: package_set.into_iter().collect(),
		metrics,
		failure: metadata.failure,
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

#[cfg(test)]
pub fn read_snapshot_directory(path: &Path) -> Result<Vec<(PathBuf, ChannelSnapshot)>> {
	read_snapshot_directory_with_mode(path, SnapshotValidationMode::Full)
}

pub fn read_snapshot_directory_with_mode(
	path: &Path,
	mode: SnapshotValidationMode,
) -> Result<Vec<(PathBuf, ChannelSnapshot)>> {
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
		manifest.validate(&manifest_path, mode)?;
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

/* Keep published manifests independent of the producer's local filesystem. */
fn recorded_input_path(path: &Path) -> Result<String> {
	if path.is_relative() {
		return Ok(path.display().to_string());
	}
	let current = std::env::current_dir().context("read current directory")?;
	Ok(path.strip_prefix(&current).unwrap_or(path).display().to_string())
}

fn snapshot_identity(snapshot: &ChannelSnapshot) -> String {
	let mut digest = Sha256::new();
	for value in [
		serde_json::to_vec(&snapshot.channel).expect("serialize channel"),
		serde_json::to_vec(&snapshot.status).expect("serialize status"),
		serde_json::to_vec(&snapshot.scope).expect("serialize scope"),
		serde_json::to_vec(&snapshot.candidates).expect("serialize candidate artifact"),
		serde_json::to_vec(&snapshot.packages).expect("serialize package observations"),
		serde_json::to_vec(&snapshot.importer_version).expect("serialize importer version"),
		serde_json::to_vec(&snapshot.importer_revision).expect("serialize importer revision"),
	] {
		digest.update(value);
		digest.update([0]);
	}
	/* Retrieval provenance describes an acquisition run, not the content identity. */
	for input in &snapshot.inputs {
		digest.update(input.path.as_bytes());
		digest.update([0]);
		digest.update(input.size.to_le_bytes());
		digest.update([0]);
		digest.update(input.sha256.as_bytes());
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

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;
	use std::path::Path;
	use std::time::{Duration, UNIX_EPOCH};

	use tempfile::tempdir;

	use super::{
		AcquisitionContext, ChannelSnapshot, PackageObservation, PackageStatus, SnapshotMetadata,
		SnapshotValidationMode, format_system_time, read_snapshot_directory, read_snapshot_directory_with_mode,
		recorded_input_path, write_failed_snapshot, write_snapshot_manifest,
	};

	#[test]
	fn records_working_tree_paths_relative_to_the_repository_root() {
		let current = std::env::current_dir().unwrap();
		assert_eq!(recorded_input_path(&current.join("Cargo.toml")).unwrap(), "Cargo.toml");
		assert_eq!(recorded_input_path(Path::new("var/input/Packages")).unwrap(), "var/input/Packages");
	}

	#[test]
	fn records_input_provenance_boundaries_and_metrics() {
		let root = tempdir().unwrap();
		let input = root.path().join("response.json");
		fs::write(&input, "{}").unwrap();
		let candidate_path = root.path().join("npm.jsonl");
		fs::write(
			&candidate_path,
			concat!(
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"tool\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		let context_path = root.path().join("context.tsv");
		let write_context = |retrieved_at: &str| {
			fs::write(
				&context_path,
				format!(
					"bnck-acquisition-context\t1\ninput\t{}\thttps://example.test/response.json\tapplication/json\t{retrieved_at}\topenpgp\tverified\t0123456789ABCDEF\nboundary\tpypi-serial\t42\n",
					input.display()
				),
			)
			.unwrap();
		};
		write_context("2026-10-23T00:00:00Z");
		let context = AcquisitionContext::read(&context_path).unwrap();
		let mut metadata = SnapshotMetadata::complete(
			"npm",
			std::slice::from_ref(&input),
			BTreeMap::from([("registry".to_owned(), "test".to_owned())]),
		);
		metadata.context = Some(&context);
		metadata.boundary.insert("release-date".to_owned(), "2026-10-23".to_owned());
		let manifest_path =
			write_snapshot_manifest(&candidate_path, metadata, [PackageObservation::observed("tool", "1.0.0")])
				.unwrap();

		let manifest: ChannelSnapshot = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
		assert_eq!(manifest.format_version, 2);
		assert_eq!(manifest.boundary["pypi-serial"], "42");
		assert_eq!(manifest.boundary["release-date"], "2026-10-23");
		assert_eq!(manifest.metrics.input_count, 1);
		assert_eq!(manifest.metrics.observed_packages, 1);
		assert_eq!(manifest.metrics.candidate_count, 1);
		let input_manifest = &manifest.inputs[0];
		assert_eq!(input_manifest.uri.as_deref(), Some("https://example.test/response.json"));
		assert_eq!(input_manifest.media_type.as_deref(), Some("application/json"));
		assert_eq!(input_manifest.retrieved_at.as_deref(), Some("2026-10-23T00:00:00Z"));
		assert_eq!(input_manifest.signature.as_ref().unwrap().result, "verified");
		assert_eq!(input_manifest.signature.as_ref().unwrap().signer.as_deref(), Some("0123456789ABCDEF"));

		write_context("2026-10-24T00:00:00Z");
		let context = AcquisitionContext::read(&context_path).unwrap();
		let mut metadata = SnapshotMetadata::complete(
			"npm",
			std::slice::from_ref(&input),
			BTreeMap::from([("registry".to_owned(), "test".to_owned())]),
		);
		metadata.context = Some(&context);
		metadata.boundary.insert("release-date".to_owned(), "2026-10-23".to_owned());
		write_snapshot_manifest(&candidate_path, metadata, [PackageObservation::observed("tool", "1.0.0")]).unwrap();
		let rewritten: ChannelSnapshot = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
		assert_eq!(rewritten.snapshot, manifest.snapshot);
		assert_ne!(rewritten.inputs[0].retrieved_at.as_deref(), manifest.inputs[0].retrieved_at.as_deref());
	}

	#[test]
	fn formats_file_timestamps_in_utc() {
		assert_eq!(format_system_time(UNIX_EPOCH), "1970-01-01T00:00:00Z");
		assert_eq!(format_system_time(UNIX_EPOCH + Duration::from_secs(1_792_785_399)), "2026-10-23T19:56:39Z");
		assert_eq!(format_system_time(UNIX_EPOCH + Duration::from_secs(951_827_696)), "2000-02-29T12:34:56Z");
	}

	#[test]
	fn previous_complete_snapshots_add_removed_observations() {
		let root = tempdir().unwrap();
		let previous = root.path().join("previous");
		let current = root.path().join("current");
		fs::create_dir(&previous).unwrap();
		fs::create_dir(&current).unwrap();
		let previous_path = previous.join("npm.jsonl");
		fs::write(&previous_path, "").unwrap();
		write_snapshot_manifest(
			&previous_path,
			SnapshotMetadata::complete("npm", &[], BTreeMap::from([("registry".to_owned(), "test".to_owned())])),
			[PackageObservation::observed("old-tool", "1.0.0")],
		)
		.unwrap();
		let current_path = current.join("npm.jsonl");
		fs::write(&current_path, "").unwrap();
		let mut metadata =
			SnapshotMetadata::complete("npm", &[], BTreeMap::from([("registry".to_owned(), "test".to_owned())]));
		metadata.previous = Some(&previous);
		write_snapshot_manifest(&current_path, metadata, [PackageObservation::observed("new-tool", "2.0.0")]).unwrap();
		let snapshots = read_snapshot_directory(&current).unwrap();
		assert!(
			snapshots[0]
				.1
				.packages
				.iter()
				.any(|package| { package.name == "old-tool" && package.status == PackageStatus::Removed })
		);
		assert_eq!(snapshots[0].1.metrics.removed_packages, 1);
	}

	#[test]
	fn failed_snapshots_record_the_failure_and_empty_artifacts() {
		let root = tempdir().unwrap();
		let output = root.path().join("npm.jsonl");
		write_failed_snapshot(
			&output,
			"npm",
			BTreeMap::from([("registry".to_owned(), "test".to_owned())]),
			&[],
			None,
			&anyhow::anyhow!("metadata is malformed\nwith a second line"),
		)
		.unwrap();
		let snapshots = read_snapshot_directory(root.path()).unwrap();
		assert_eq!(snapshots[0].1.status, super::SnapshotStatus::Failed);
		assert_eq!(snapshots[0].1.failure.as_deref(), Some("metadata is malformed with a second line"));
		assert_eq!(snapshots[0].1.metrics.candidate_count, 0);
		assert_eq!(snapshots[0].1.packages.len(), 0);
		assert_eq!(fs::read_to_string(&output).unwrap(), "");
	}

	#[test]
	fn candidate_only_validation_does_not_read_raw_inputs() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		fs::create_dir(&acquisitions).unwrap();
		let input = root.path().join("Packages");
		fs::write(&input, "Packages").unwrap();
		let candidate_path = acquisitions.join("npm.jsonl");
		fs::write(
			&candidate_path,
			concat!(
				"{\"channel\":\"npm\",\"package\":\"tool\",\"version\":\"1.0.0\",",
				"\"executable\":\"tool\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		write_snapshot_manifest(
			&candidate_path,
			SnapshotMetadata::complete(
				"npm",
				std::slice::from_ref(&input),
				BTreeMap::from([("registry".to_owned(), "test".to_owned())]),
			),
			[PackageObservation::observed("tool", "1.0.0")],
		)
		.unwrap();
		fs::remove_file(input).unwrap();

		assert!(read_snapshot_directory(&acquisitions).is_err());
		let snapshots =
			read_snapshot_directory_with_mode(&acquisitions, SnapshotValidationMode::CandidateOnly).unwrap();
		assert_eq!(snapshots.len(), 1);
		assert_eq!(snapshots[0].1.channel, "npm");
	}
}
