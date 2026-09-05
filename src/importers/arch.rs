use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::acquisition::{PackageObservation, SnapshotMetadata, write_acquisition};
use crate::model::Candidate;

pub fn import(input: &Path, output: &Path, scope: std::collections::BTreeMap<String, String>) -> Result<usize> {
	let mut candidates = Vec::new();
	let mut package_observations = Vec::new();
	for entry in WalkDir::new(input).follow_links(false) {
		let entry = entry.with_context(|| format!("walk {}", input.display()))?;
		if !entry.file_type().is_file() || entry.file_name() != "files" {
			continue;
		}
		let Some(directory) = entry.path().parent() else {
			continue;
		};
		let fields = parse_sections(&fs::read_to_string(directory.join("desc"))?);
		let files = parse_sections(&fs::read_to_string(entry.path())?);
		let Some(package) = first(&fields, "NAME") else {
			continue;
		};
		let version = first(&fields, "VERSION").unwrap_or_default();
		let mut package_observation = PackageObservation::observed(package, version);
		if let Some(architecture) = first(&fields, "ARCH") {
			package_observation
				.variant
				.insert("architecture".to_owned(), architecture.to_owned());
		}
		package_observation.artifact_sha256 = first(&fields, "SHA256SUM").map(str::to_owned);
		package_observations.push(package_observation);
		for path in files.get("FILES").into_iter().flatten() {
			let Some(executable) = public_executable(path) else {
				continue;
			};
			candidates.push(Candidate {
				channel: "arch".into(),
				package: package.to_owned(),
				version: version.to_owned(),
				variant: first(&fields, "ARCH")
					.map(|architecture| BTreeMap::from([("architecture".to_owned(), architecture.to_owned())]))
					.unwrap_or_default(),
				artifact_sha256: first(&fields, "SHA256SUM").map(str::to_owned),
				executable: executable.to_owned(),
				evidence: "repository-file-listing".into(),
				source_package: first(&fields, "BASE").map(str::to_owned),
				homepage: first(&fields, "URL").map(str::to_owned),
				repository: None,
				description: first(&fields, "DESC").map(str::to_owned),
			});
		}
	}
	let mut inputs = vec![input.to_owned()];
	if let Some(archives) = input
		.parent()
		.map(|parent| parent.join("archives"))
		.filter(|path| path.is_dir())
	{
		inputs.push(archives);
	}
	write_acquisition(output, SnapshotMetadata::complete("arch", &inputs, scope), package_observations, candidates)
}

fn parse_sections(value: &str) -> BTreeMap<String, Vec<String>> {
	let mut fields = BTreeMap::<String, Vec<String>>::new();
	let mut current = None::<String>;
	for line in value.lines() {
		if line.starts_with('%') && line.ends_with('%') {
			let name = line.trim_matches('%').to_owned();
			fields.entry(name.clone()).or_default();
			current = Some(name);
		} else if !line.is_empty()
			&& let Some(name) = current.as_ref()
		{
			fields.entry(name.clone()).or_default().push(line.to_owned());
		}
	}
	fields
}

fn first<'a>(fields: &'a BTreeMap<String, Vec<String>>, name: &str) -> Option<&'a str> {
	fields.get(name)?.first().map(String::as_str)
}

fn public_executable(path: &str) -> Option<&str> {
	let path = path.trim_start_matches('/');
	for prefix in ["usr/bin/", "usr/sbin/", "bin/", "sbin/"] {
		if let Some(name) = path.strip_prefix(prefix) {
			return (!name.is_empty() && !name.ends_with('/') && !name.contains('/')).then_some(name);
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use super::{parse_sections, public_executable};

	#[test]
	fn parses_repository_sections() {
		let fields = parse_sections("%NAME%\nripgrep\n\n%FILES%\nusr/bin/rg\nusr/share/man/rg.1\n");
		assert_eq!(fields["NAME"], ["ripgrep"]);
		assert_eq!(fields["FILES"], ["usr/bin/rg", "usr/share/man/rg.1"]);
	}

	#[test]
	fn recognizes_public_commands() {
		assert_eq!(public_executable("usr/bin/rg"), Some("rg"));
		assert_eq!(public_executable("usr/bin/"), None);
	}
}
