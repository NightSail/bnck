use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use xz2::read::XzDecoder;

use crate::acquisition::{PackageObservation, SnapshotMetadata, file_identity, write_acquisition};
use crate::model::Candidate;

#[derive(Clone, Debug, Default)]
struct Package {
	name: String,
	version: String,
	source: Option<String>,
	homepage: Option<String>,
	description: Option<String>,
	architecture: Option<String>,
	checksum: String,
}

pub fn plan(repomd: &Path, base_url: &str, output_directory: &Path, output: &Path) -> Result<usize> {
	let mut downloads = Vec::new();
	for entry in read_repomd(repomd)? {
		if !matches!(entry.kind.as_str(), "primary" | "filelists") {
			continue;
		}
		ensure!(!entry.location.is_empty(), "repomd.xml {} has a metadata entry without a location", entry.kind);
		let filename = entry.location.rsplit('/').next().unwrap_or(&entry.kind);
		downloads.push((
			format!("{}/{}", base_url.trim_end_matches('/'), entry.location.trim_start_matches('/')),
			output_directory.join(format!("{}-{filename}", entry.kind)),
		));
	}

	let file = File::create(output).with_context(|| format!("create {}", output.display()))?;
	let mut writer = BufWriter::new(file);
	for (url, path) in &downloads {
		writeln!(writer, "{url}\t{}", path.display())?;
	}
	writer.flush()?;
	Ok(downloads.len())
}

#[derive(Clone, Debug, Default)]
struct RepomdEntry {
	kind: String,
	location: String,
	checksum_type: Option<String>,
	checksum: Option<String>,
	size: Option<u64>,
}

#[derive(Clone, Copy)]
enum RepomdField {
	Checksum,
	Size,
}

fn read_repomd(path: &Path) -> Result<Vec<RepomdEntry>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut reader = Reader::from_reader(BufReader::new(file));
	reader.config_mut().trim_text(true);
	let mut buffer = Vec::new();
	let mut current = None::<RepomdEntry>;
	let mut field = None::<RepomdField>;
	let mut entries = Vec::new();

	loop {
		match reader.read_event_into(&mut buffer)? {
			Event::Start(event) if event.local_name().as_ref() == b"data" => {
				current = Some(RepomdEntry {
					kind: attribute(&event, b"type", reader.decoder())?.unwrap_or_default(),
					..RepomdEntry::default()
				});
				field = None;
			},
			Event::Empty(event) if event.local_name().as_ref() == b"location" => {
				if let Some(entry) = current.as_mut() {
					entry.location = attribute(&event, b"href", reader.decoder())?.unwrap_or_default();
				}
			},
			Event::Start(event) if event.local_name().as_ref() == b"checksum" => {
				if let Some(entry) = current.as_mut() {
					entry.checksum_type = attribute(&event, b"type", reader.decoder())?;
				}
				field = Some(RepomdField::Checksum);
			},
			Event::Start(event) if event.local_name().as_ref() == b"size" => {
				field = Some(RepomdField::Size);
			},
			Event::Text(text) if field.is_some() => {
				if let Some(entry) = current.as_mut() {
					let value = text.decode()?.trim().to_owned();
					match field {
						Some(RepomdField::Checksum) => entry.checksum = Some(value),
						Some(RepomdField::Size) => {
							entry.size = Some(value.parse().with_context(|| {
								format!("parse repomd.xml size for {} in {}", entry.kind, path.display())
							})?);
						},
						None => {},
					}
				}
			},
			Event::End(event) if matches!(event.local_name().as_ref(), b"checksum" | b"size") => field = None,
			Event::End(event) if event.local_name().as_ref() == b"data" => {
				if let Some(entry) = current.take() {
					entries.push(entry);
				}
				field = None;
			},
			Event::Eof => break,
			_ => {},
		}
		buffer.clear();
	}
	Ok(entries)
}

fn verify_repomd_metadata(repomd: &Path, primary: &Path, filelists: &Path) -> Result<()> {
	let entries = read_repomd(repomd)?;
	for (kind, path) in [("primary", primary), ("filelists", filelists)] {
		let entry = entries
			.iter()
			.find(|entry| entry.kind == kind)
			.with_context(|| format!("repomd.xml has no {kind} metadata entry"))?;
		ensure!(
			entry.checksum_type.as_deref() == Some("sha256"),
			"repomd.xml {kind} entry does not declare a SHA-256 checksum"
		);
		let expected_sha256 = entry
			.checksum
			.as_deref()
			.with_context(|| format!("repomd.xml {kind} entry has no checksum"))?;
		let expected_size = entry.size.with_context(|| format!("repomd.xml {kind} entry has no size"))?;
		let (size, sha256) = file_identity(path)?;
		ensure!(size == expected_size, "{} size does not match repomd.xml", path.display());
		ensure!(sha256 == expected_sha256, "{} digest does not match repomd.xml", path.display());
	}
	Ok(())
}

pub fn import(
	primary: &Path,
	filelists: &Path,
	output: &Path,
	scope: std::collections::BTreeMap<String, String>,
) -> Result<usize> {
	let packages = read_primary(primary)?;
	let candidates = read_filelists(filelists, &packages)?;
	let package_observations = packages
		.values()
		.map(|package| {
			let mut observation = PackageObservation::observed(&package.name, &package.version);
			if let Some(architecture) = &package.architecture {
				observation.variant.insert("architecture".to_owned(), architecture.clone());
			}
			if package.checksum.len() == 64 {
				observation.artifact_sha256 = Some(package.checksum.clone());
			}
			observation
		})
		.collect::<Vec<_>>();
	let repomd = primary
		.parent()
		.and_then(Path::parent)
		.map(|parent| parent.join("repomd.xml"))
		.with_context(|| format!("find repomd.xml for {}", primary.display()))?;
	ensure!(repomd.is_file(), "missing {}", repomd.display());
	verify_repomd_metadata(&repomd, primary, filelists)?;
	let inputs = vec![primary.to_owned(), filelists.to_owned(), repomd];
	write_acquisition(output, SnapshotMetadata::complete("fedora", &inputs, scope), package_observations, candidates)
}

fn read_primary(path: &Path) -> Result<HashMap<String, Package>> {
	let input = compressed_reader(path)?;
	let mut reader = Reader::from_reader(input);
	reader.config_mut().trim_text(true);
	let mut buffer = Vec::new();
	let mut package = None::<Package>;
	let mut field = None::<Field>;
	let mut packages = HashMap::new();

	loop {
		match reader.read_event_into(&mut buffer)? {
			Event::Start(event) if event.local_name().as_ref() == b"package" => package = Some(Package::default()),
			Event::Empty(event) if event.local_name().as_ref() == b"version" => {
				if let Some(package) = package.as_mut() {
					package.version = rpm_version(&event, reader.decoder())?;
				}
			},
			Event::Start(event) => field = Field::from_name(event.local_name().as_ref()),
			Event::Text(text) => {
				if let (Some(package), Some(field)) = (package.as_mut(), field) {
					field.set(package, text.decode()?.into_owned());
				}
			},
			Event::End(event) if event.local_name().as_ref() == b"package" => {
				if let Some(package) = package.take()
					&& !package.checksum.is_empty()
				{
					packages.insert(package.checksum.clone(), package);
				}
				field = None;
			},
			Event::End(_) => field = None,
			Event::Eof => break,
			_ => {},
		}
		buffer.clear();
	}
	Ok(packages)
}

fn read_filelists(path: &Path, packages: &HashMap<String, Package>) -> Result<Vec<Candidate>> {
	let input = compressed_reader(path)?;
	let mut reader = Reader::from_reader(input);
	reader.config_mut().trim_text(true);
	let mut buffer = Vec::new();
	let mut package_id = None::<String>;
	let mut is_file = false;
	let mut candidates = Vec::new();

	loop {
		match reader.read_event_into(&mut buffer)? {
			Event::Start(event) if event.local_name().as_ref() == b"package" => {
				package_id = attribute(&event, b"pkgid", reader.decoder())?;
			},
			Event::Start(event) if event.local_name().as_ref() == b"file" => {
				let kind = attribute(&event, b"type", reader.decoder())?;
				is_file = !matches!(kind.as_deref(), Some("dir" | "ghost"));
			},
			Event::Text(text) if is_file => {
				let path = text.decode()?;
				if let Some(executable) = public_executable(&path)
					&& let Some(package) = package_id.as_ref().and_then(|id| packages.get(id))
				{
					candidates.push(Candidate {
						channel: "fedora".into(),
						package: package.name.clone(),
						version: package.version.clone(),
						variant: package
							.architecture
							.iter()
							.map(|architecture| ("architecture".to_owned(), architecture.clone()))
							.collect(),
						artifact_sha256: (package.checksum.len() == 64).then(|| package.checksum.clone()),
						executable: executable.to_owned(),
						evidence: "repository-file-listing".into(),
						source_package: package.source.clone(),
						homepage: package.homepage.clone(),
						repository: None,
						description: package.description.clone(),
					});
				}
			},
			Event::End(event) if event.local_name().as_ref() == b"file" => is_file = false,
			Event::End(event) if event.local_name().as_ref() == b"package" => package_id = None,
			Event::Eof => break,
			_ => {},
		}
		buffer.clear();
	}
	Ok(candidates)
}

#[derive(Clone, Copy)]
enum Field {
	Name,
	Checksum,
	Summary,
	Description,
	Architecture,
	Url,
	Source,
}

impl Field {
	fn from_name(name: &[u8]) -> Option<Self> {
		match name {
			b"name" => Some(Self::Name),
			b"checksum" => Some(Self::Checksum),
			b"summary" => Some(Self::Summary),
			b"description" => Some(Self::Description),
			b"arch" => Some(Self::Architecture),
			b"url" => Some(Self::Url),
			b"sourcerpm" => Some(Self::Source),
			_ => None,
		}
	}

	fn set(self, package: &mut Package, value: String) {
		match self {
			Self::Name => package.name = value,
			Self::Checksum => package.checksum = value,
			Self::Summary => package.description = Some(value),
			Self::Description if package.description.is_none() => package.description = Some(value),
			Self::Description => {},
			Self::Architecture => package.architecture = Some(value),
			Self::Url => package.homepage = Some(value),
			Self::Source => package.source = Some(source_rpm_name(&value).to_owned()),
		}
	}
}

fn source_rpm_name(value: &str) -> &str {
	let value = value.strip_suffix(".src.rpm").unwrap_or(value);
	let mut parts = value.rsplitn(3, '-');
	let _release = parts.next();
	let _version = parts.next();
	parts.next().unwrap_or(value)
}

fn rpm_version(event: &BytesStart<'_>, decoder: Decoder) -> Result<String> {
	let epoch = attribute(event, b"epoch", decoder)?.unwrap_or_default();
	let version = attribute(event, b"ver", decoder)?.unwrap_or_default();
	let release = attribute(event, b"rel", decoder)?.unwrap_or_default();
	let epoch = if epoch.is_empty() || epoch == "0" { String::new() } else { format!("{epoch}:") };
	let release = if release.is_empty() { String::new() } else { format!("-{release}") };
	Ok(format!("{epoch}{version}{release}"))
}

fn attribute(event: &BytesStart<'_>, name: &[u8], decoder: Decoder) -> Result<Option<String>> {
	for attribute in event.attributes().with_checks(false) {
		let attribute = attribute?;
		if attribute.key.local_name().as_ref() == name {
			return Ok(Some(attribute.decode_and_unescape_value(decoder)?.into_owned()));
		}
	}
	Ok(None)
}

fn compressed_reader(path: &Path) -> Result<Box<dyn BufRead>> {
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let filename = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
	if filename.ends_with(".gz") {
		Ok(Box::new(BufReader::new(GzDecoder::new(file))))
	} else if filename.ends_with(".xz") {
		Ok(Box::new(BufReader::new(XzDecoder::new(file))))
	} else if filename.ends_with(".zst") || filename.ends_with(".zstd") {
		Ok(Box::new(BufReader::new(zstd::stream::read::Decoder::new(file)?)))
	} else {
		Ok(Box::new(BufReader::new(file)))
	}
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

	use quick_xml::Reader;
	use sha2::{Digest, Sha256};
	use tempfile::tempdir;

	use super::{public_executable, rpm_version, source_rpm_name, verify_repomd_metadata};
	use crate::acquisition::file_identity;

	#[test]
	fn formats_nevra_version() {
		let mut reader = Reader::from_str("<version epoch='1' ver='2.3' rel='4.fc43'/>");
		let event = reader.read_event().unwrap();
		let quick_xml::events::Event::Empty(event) = event else { panic!("expected empty element") };
		assert_eq!(rpm_version(&event, reader.decoder()).unwrap(), "1:2.3-4.fc43");
	}

	#[test]
	fn extracts_source_rpm_name() {
		assert_eq!(source_rpm_name("rust-ripgrep-14.1.1-3.fc43.src.rpm"), "rust-ripgrep");
	}

	#[test]
	fn recognizes_public_commands() {
		assert_eq!(public_executable("/usr/bin/rg"), Some("rg"));
		assert_eq!(public_executable("/usr/libexec/rg"), None);
	}

	#[test]
	fn verifies_repomd_declared_metadata_checksums() {
		let root = tempdir().unwrap();
		let primary = root.path().join("primary.xml.zst");
		let filelists = root.path().join("filelists.xml.zst");
		fs::write(&primary, "primary metadata").unwrap();
		fs::write(&filelists, "filelists metadata").unwrap();
		let repomd = root.path().join("repomd.xml");
		let (primary_size, primary_sha256) = file_identity(&primary).unwrap();
		let (filelists_size, filelists_sha256) = file_identity(&filelists).unwrap();
		let xml = format!(
			"<repomd><data type=\"primary\"><checksum type=\"sha256\">{primary_sha256}</checksum>\
			 <location href=\"repodata/primary.xml.zst\"/><size>{primary_size}</size></data>\
			 <data type=\"filelists\"><checksum type=\"sha256\">{filelists_sha256}</checksum>\
			 <location href=\"repodata/filelists.xml.zst\"/><size>{filelists_size}</size></data></repomd>"
		);
		fs::write(&repomd, xml).unwrap();
		verify_repomd_metadata(&repomd, &primary, &filelists).unwrap();

		fs::write(&primary, "tampered").unwrap();
		let error = verify_repomd_metadata(&repomd, &primary, &filelists).unwrap_err();
		assert!(error.to_string().contains("does not match repomd.xml"));
		assert_eq!(format!("{:x}", Sha256::digest(b"primary metadata")), primary_sha256);
	}
}
