use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::Serialize;
use tempfile::NamedTempFile;

use crate::acquisition::{PackageStatus, file_identity, read_snapshot_directory};
use crate::curation::{Project, load_projects};
use crate::model::{PROVIDER_SCHEMA_VERSION, ProviderDataset, read_provider_dataset};

pub const SQLITE_SCHEMA_VERSION: u32 = 2;
const LICENSE_TEXT: &str = include_str!("../LICENSE");
const LICENSE_FILE: &str = "LICENSE";

pub struct SqliteMetadata<'a> {
	pub dataset_release: &'a str,
	pub generated_at: &'a str,
	pub generator_version: &'a str,
	pub generator_revision: &'a str,
}

pub fn generate_sqlite_artifact(
	input: &Path,
	curations: &Path,
	output: &Path,
	metadata: &SqliteMetadata<'_>,
) -> Result<ProviderDataset> {
	let parent = output
		.parent()
		.filter(|path| !path.as_os_str().is_empty())
		.unwrap_or(Path::new("."));
	fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	let database =
		NamedTempFile::new_in(parent).with_context(|| format!("create temporary file in {}", parent.display()))?;
	let dataset = write_sqlite_database(input, curations, database.path(), metadata)?;
	let mut artifact =
		NamedTempFile::new_in(parent).with_context(|| format!("create temporary file in {}", parent.display()))?;

	let source = File::open(database.path()).with_context(|| format!("open {}", database.path().display()))?;
	let mut encoder = zstd::stream::write::Encoder::new(artifact.as_file_mut(), zstd::DEFAULT_COMPRESSION_LEVEL)?;
	io::copy(&mut io::BufReader::new(source), &mut encoder)?;
	let compressed = encoder.finish()?;
	compressed.flush()?;
	compressed.sync_all()?;
	artifact
		.persist(output)
		.map_err(|error| error.error)
		.with_context(|| format!("persist {}", output.display()))?;

	Ok(dataset)
}

fn write_sqlite_database(
	input: &Path,
	curations: &Path,
	output: &Path,
	metadata: &SqliteMetadata<'_>,
) -> Result<ProviderDataset> {
	validate_metadata(metadata)?;
	let projects = load_projects(&curations.join("projects"))?;
	let mut connection = Connection::open(output)?;
	let transaction = connection.transaction()?;
	transaction.execute_batch(
		"CREATE TABLE metadata (
			key TEXT PRIMARY KEY,
			value TEXT NOT NULL
		) WITHOUT ROWID;

		CREATE TABLE providers (
			project TEXT NOT NULL CHECK (length(project) > 0),
			executable TEXT NOT NULL CHECK (length(executable) > 0 AND instr(executable, '/') = 0),
			channel TEXT NOT NULL CHECK (channel IN ('arch', 'crates.io', 'debian', 'fedora', 'homebrew', 'npm', 'pypi')),
			package TEXT NOT NULL CHECK (length(package) > 0),
			PRIMARY KEY (project, executable, channel, package)
		) WITHOUT ROWID;

		CREATE INDEX providers_by_executable
		ON providers (executable, project, channel, package);

		CREATE TABLE project_links (
			project TEXT NOT NULL,
			type TEXT NOT NULL CHECK (type IN ('homepage', 'repository', 'documentation', 'issue tracker', 'link')),
			url TEXT NOT NULL CHECK (length(url) > 0),
			PRIMARY KEY (project, type, url)
		) WITHOUT ROWID;",
	)?;

	let mut dataset_projects = std::collections::BTreeSet::new();
	let dataset = {
		let mut statement = transaction.prepare_cached(
			"INSERT INTO providers (project, executable, channel, package)
			 VALUES (?1, ?2, ?3, ?4)",
		)?;
		read_provider_dataset(input, |provider| {
			ensure!(
				projects.contains_key(&provider.project),
				"provider references unknown project {}",
				provider.project
			);
			statement.execute(params![provider.project, provider.executable, provider.channel, provider.package])?;
			dataset_projects.insert(provider.project.clone());
			Ok(())
		})?
	};

	{
		let mut statement =
			transaction.prepare_cached("INSERT INTO project_links (project, type, url) VALUES (?1, ?2, ?3)")?;
		for project_id in dataset_projects {
			let project = &projects[&project_id];
			for (link_type, url) in project_links(project) {
				ensure_web_url(url)?;
				statement.execute((&project_id, link_type, url))?;
			}
		}
	}

	let metadata_values = [
		("schema_version", SQLITE_SCHEMA_VERSION.to_string()),
		("dataset_schema_version", PROVIDER_SCHEMA_VERSION.to_string()),
		("dataset_release", metadata.dataset_release.to_owned()),
		("dataset_sha256", dataset.sha256.clone()),
		("provider_count", dataset.provider_count.to_string()),
		("generated_at", metadata.generated_at.to_owned()),
		("generator_version", metadata.generator_version.to_owned()),
		("generator_revision", metadata.generator_revision.to_owned()),
	];
	{
		let mut statement = transaction.prepare_cached("INSERT INTO metadata (key, value) VALUES (?1, ?2)")?;
		for (key, value) in metadata_values {
			statement.execute((key, value))?;
		}
	}
	transaction.pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)?;
	transaction.commit()?;

	Ok(dataset)
}

#[derive(Serialize)]
struct ReleaseManifest<'a> {
	format: &'static str,
	format_version: u32,
	release: &'a str,
	generated_at: &'a str,
	dataset: DatasetManifest,
	generator: GeneratorManifest<'a>,
	artifacts: Vec<ArtifactManifest>,
	channels: Vec<ChannelManifest>,
}

#[derive(Serialize)]
struct DatasetManifest {
	schema_version: u32,
	sha256: String,
	provider_count: usize,
}

#[derive(Serialize)]
struct GeneratorManifest<'a> {
	version: &'static str,
	source_revision: &'a str,
}

#[derive(Serialize)]
struct ArtifactManifest {
	name: String,
	media_type: &'static str,
	compression: &'static str,
	#[serde(skip_serializing_if = "Option::is_none")]
	schema_version: Option<u32>,
	sha256: String,
	size: u64,
}

#[derive(Serialize)]
struct ChannelManifest {
	name: String,
	scope: BTreeMap<String, String>,
	snapshot_manifest: SnapshotManifest,
	package_count: usize,
	provider_count: usize,
	status: String,
}

#[derive(Serialize)]
struct SnapshotManifest {
	path: String,
	sha256: String,
}

pub struct ReleaseMetadata<'a> {
	pub release: &'a str,
	pub generated_at: &'a str,
	pub generator_revision: &'a str,
}

pub fn assemble_release(
	providers: &Path,
	acquisitions: &Path,
	curations: &Path,
	output: &Path,
	metadata: &ReleaseMetadata<'_>,
) -> Result<ProviderDataset> {
	validate_metadata(&SqliteMetadata {
		dataset_release: metadata.release,
		generated_at: metadata.generated_at,
		generator_version: env!("CARGO_PKG_VERSION"),
		generator_revision: metadata.generator_revision,
	})?;
	ensure!(!output.exists(), "release output already exists: {}", output.display());

	let snapshots = read_snapshot_directory(acquisitions)?;
	ensure!(!snapshots.is_empty(), "{} contains no acquisition snapshot manifests", acquisitions.display());
	for (_, snapshot) in &snapshots {
		for input in &snapshot.inputs {
			let input_path = Path::new(&input.path);
			ensure!(
				!input_path.is_absolute(),
				"channel snapshot {} records an absolute input path {}; record inputs under the repository root and re-import",
				snapshot.snapshot,
				input.path
			);
			ensure!(
				!input_path.components().any(|component| matches!(
					component,
					Component::CurDir | Component::ParentDir | Component::RootDir | Component::Prefix(_)
				)),
				"channel snapshot {} records a non-relative input path {}",
				snapshot.snapshot,
				input.path
			);
		}
	}
	let snapshot_channels = snapshots
		.iter()
		.map(|(_, snapshot)| snapshot.channel.as_str())
		.collect::<std::collections::BTreeSet<_>>();
	let mut provider_package_counts = BTreeMap::<(String, String), usize>::new();
	let dataset = read_provider_dataset(providers, |provider| {
		*provider_package_counts
			.entry((provider.channel.clone(), provider.package.clone()))
			.or_default() += 1;
		Ok(())
	})?;

	let provider_channels = provider_package_counts
		.keys()
		.map(|(channel, _)| channel.as_ref())
		.collect::<std::collections::BTreeSet<&str>>();
	for channel in provider_channels {
		ensure!(snapshot_channels.contains(channel), "provider channel {channel:?} has no snapshot manifest");
	}
	fs::create_dir_all(output.join("channel-manifests"))
		.with_context(|| format!("create release directory {}", output.display()))?;
	fs::write(output.join(LICENSE_FILE), LICENSE_TEXT)
		.with_context(|| format!("write {LICENSE_FILE} to {}", output.display()))?;

	let providers_artifact = output.join("providers.jsonl.zst");
	compress_file(providers, &providers_artifact)?;
	let sqlite_artifact = output.join("bnck.sqlite.zst");
	generate_sqlite_artifact(
		providers,
		curations,
		&sqlite_artifact,
		&SqliteMetadata {
			dataset_release: metadata.release,
			generated_at: metadata.generated_at,
			generator_version: env!("CARGO_PKG_VERSION"),
			generator_revision: metadata.generator_revision,
		},
	)?;

	let artifacts = vec![
		artifact_manifest(&providers_artifact, "application/x-ndjson", None)?,
		artifact_manifest(&sqlite_artifact, "application/vnd.sqlite3", Some(SQLITE_SCHEMA_VERSION))?,
	];
	let mut channels = Vec::new();
	for (source_path, snapshot) in snapshots {
		let filename = format!("{}-{}.json", snapshot.channel.replace('.', "-"), snapshot.snapshot);
		let relative_path = PathBuf::from("channel-manifests").join(filename);
		let destination = output.join(&relative_path);
		fs::copy(&source_path, &destination)
			.with_context(|| format!("copy {} to {}", source_path.display(), destination.display()))?;
		let (_, sha256) = file_identity(&destination)?;
		let candidate_path = Path::new(&snapshot.candidates.path);
		ensure!(
			candidate_path.components().count() == 1 && candidate_path.file_name().is_some(),
			"candidate artifact path {:?} is not a single file name",
			snapshot.candidates.path
		);
		let candidate_name = candidate_path
			.file_name()
			.and_then(|value| value.to_str())
			.context("candidate artifact path is not UTF-8")?;
		let candidate_source = source_path.parent().unwrap_or(Path::new(".")).join(candidate_path);
		let candidate_destination = output.join("channel-manifests").join(candidate_name);
		ensure!(
			!candidate_destination.exists(),
			"candidate artifact {candidate_name} is referenced by multiple snapshots"
		);
		fs::copy(&candidate_source, &candidate_destination)
			.with_context(|| format!("copy {} to {}", candidate_source.display(), candidate_destination.display()))?;
		let (candidate_size, candidate_sha256) = file_identity(&candidate_destination)?;
		ensure!(
			candidate_size == snapshot.candidates.size && candidate_sha256 == snapshot.candidates.sha256,
			"copied candidate artifact {candidate_name} does not match its manifest"
		);
		let observed_packages = snapshot
			.packages
			.iter()
			.filter(|package| package.status == PackageStatus::Observed)
			.map(|package| package.name.as_str())
			.collect::<std::collections::BTreeSet<_>>();
		let provider_count = provider_package_counts
			.iter()
			.filter(|((channel, package), _)| {
				channel == &snapshot.channel && observed_packages.contains(package.as_str())
			})
			.map(|(_, count)| count)
			.sum();
		channels.push(ChannelManifest {
			name: snapshot.channel.clone(),
			scope: snapshot.scope,
			snapshot_manifest: SnapshotManifest { path: path_string(&relative_path)?, sha256 },
			package_count: snapshot
				.packages
				.iter()
				.filter(|package| package.status == PackageStatus::Observed)
				.count(),
			provider_count,
			status: serde_json::to_value(snapshot.status)?
				.as_str()
				.context("serialize snapshot status")?
				.to_owned(),
		});
	}
	channels.sort_by(|left, right| {
		(&left.name, &left.snapshot_manifest.path).cmp(&(&right.name, &right.snapshot_manifest.path))
	});

	let manifest = ReleaseManifest {
		format: "bnck-dataset-release",
		format_version: 1,
		release: metadata.release,
		generated_at: metadata.generated_at,
		dataset: DatasetManifest {
			schema_version: PROVIDER_SCHEMA_VERSION,
			sha256: dataset.sha256.clone(),
			provider_count: dataset.provider_count,
		},
		generator: GeneratorManifest {
			version: env!("CARGO_PKG_VERSION"),
			source_revision: metadata.generator_revision,
		},
		artifacts,
		channels,
	};
	let manifest_path = output.join("manifest.json");
	let mut bytes = serde_json::to_vec_pretty(&manifest)?;
	bytes.push(b'\n');
	fs::write(&manifest_path, bytes).with_context(|| format!("write {}", manifest_path.display()))?;
	write_checksums(output)?;
	Ok(dataset)
}

fn compress_file(input: &Path, output: &Path) -> Result<()> {
	let source = File::open(input).with_context(|| format!("open {}", input.display()))?;
	let target = File::create(output).with_context(|| format!("create {}", output.display()))?;
	let mut encoder = zstd::stream::write::Encoder::new(target, zstd::DEFAULT_COMPRESSION_LEVEL)?;
	io::copy(&mut io::BufReader::new(source), &mut encoder)?;
	let target = encoder.finish()?;
	target.sync_all()?;
	Ok(())
}

fn artifact_manifest(path: &Path, media_type: &'static str, schema_version: Option<u32>) -> Result<ArtifactManifest> {
	let (size, sha256) = file_identity(path)?;
	Ok(ArtifactManifest {
		name: path
			.file_name()
			.and_then(|value| value.to_str())
			.context("artifact filename is not UTF-8")?
			.to_owned(),
		media_type,
		compression: "zstd",
		schema_version,
		sha256,
		size,
	})
}

fn write_checksums(root: &Path) -> Result<()> {
	let mut paths = vec![
		root.join("manifest.json"),
		root.join("providers.jsonl.zst"),
		root.join("bnck.sqlite.zst"),
		root.join(LICENSE_FILE),
	];
	for entry in fs::read_dir(root.join("channel-manifests"))? {
		paths.push(entry?.path());
	}
	paths.sort();
	let checksum_path = root.join("SHA256SUMS");
	let mut writer = io::BufWriter::new(File::create(&checksum_path)?);
	for path in paths {
		let (_, sha256) = file_identity(&path)?;
		let relative = path.strip_prefix(root).unwrap_or(&path);
		writeln!(writer, "{sha256}  {}", path_string(relative)?)?;
	}
	writer.flush()?;
	Ok(())
}

fn path_string(path: &Path) -> Result<String> {
	Ok(path.to_str().context("release path is not UTF-8")?.replace('\\', "/"))
}

fn project_links(project: &Project) -> Vec<(&'static str, &str)> {
	let mut links = Vec::new();
	for (link_type, url) in [
		("homepage", project.homepage.as_deref()),
		("repository", project.repository.as_deref()),
		("documentation", project.documentation.as_deref()),
		("issue tracker", project.issue_tracker.as_deref()),
	] {
		if let Some(url) = url {
			links.push((link_type, url));
		}
	}
	links.extend(project.links.iter().map(|url| ("link", url.as_str())));
	links
}

fn ensure_web_url(url: &str) -> Result<()> {
	ensure!(url.starts_with("https://") || url.starts_with("http://"), "unsupported project link URL {url:?}");
	Ok(())
}

fn validate_metadata(metadata: &SqliteMetadata<'_>) -> Result<()> {
	validate_release(metadata.dataset_release)?;
	validate_timestamp(metadata.generated_at)?;
	ensure!(!metadata.generator_version.trim().is_empty(), "generator version is empty");
	ensure!(
		(7..=64).contains(&metadata.generator_revision.len())
			&& metadata.generator_revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
		"generator revision is not a hexadecimal Git revision"
	);
	Ok(())
}

fn validate_release(value: &str) -> Result<()> {
	let (date, sequence) = value.split_once('.').context("dataset release must use YYYY-MM-DD.N")?;
	validate_date(date, "dataset release")?;
	let sequence = sequence.parse::<u32>().context("dataset release sequence is not an integer")?;
	ensure!(sequence > 0, "dataset release sequence must be greater than zero");
	Ok(())
}

fn validate_timestamp(value: &str) -> Result<()> {
	ensure!(value.len() == 20 && value.ends_with('Z'), "generation timestamp must use YYYY-MM-DDTHH:MM:SSZ");
	let (date, time) = value[..19].split_once('T').context("generation timestamp has no T separator")?;
	validate_date(date, "generation timestamp")?;
	let parts = time
		.split(':')
		.map(str::parse::<u32>)
		.collect::<Result<Vec<_>, _>>()
		.context("generation timestamp has a non-numeric time")?;
	ensure!(parts.len() == 3, "generation timestamp must contain hours, minutes, and seconds");
	ensure!(parts[0] < 24 && parts[1] < 60 && parts[2] < 60, "generation timestamp time is out of range");
	Ok(())
}

fn validate_date(value: &str, field: &str) -> Result<()> {
	ensure!(value.len() == 10, "{field} date must use YYYY-MM-DD");
	let parts = value
		.split('-')
		.map(str::parse::<u32>)
		.collect::<Result<Vec<_>, _>>()
		.with_context(|| format!("{field} date is not numeric"))?;
	ensure!(parts.len() == 3, "{field} date must use YYYY-MM-DD");
	let (year, month, day) = (parts[0], parts[1], parts[2]);
	ensure!(year > 0 && (1..=12).contains(&month), "{field} date is out of range");
	let is_leap_year = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
	let days = match month {
		2 if is_leap_year => 29,
		2 => 28,
		4 | 6 | 9 | 11 => 30,
		_ => 31,
	};
	ensure!((1..=days).contains(&day), "{field} date is out of range");
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use rusqlite::Connection;
	use tempfile::tempdir;

	use super::{ReleaseMetadata, SQLITE_SCHEMA_VERSION, SqliteMetadata, assemble_release, generate_sqlite_artifact};
	use crate::acquisition::{PackageObservation, SnapshotMetadata, write_snapshot_manifest};

	const PROVIDERS: &str = concat!(
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n",
		"{\"project\":\"ripgrep\",\"executable\":\"rg\",\"channel\":\"crates.io\",\"package\":\"ripgrep\"}\n",
	);

	#[test]
	fn assembles_release_with_acquisition_provenance() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = write_curations(root.path());
		let output = root.path().join("release");
		fs::create_dir(&acquisitions).unwrap();
		let candidates = acquisitions.join("crates-io.jsonl");
		fs::write(
			&candidates,
			concat!(
				"{\"channel\":\"crates.io\",\"package\":\"helix-term\",\"version\":\"1\",",
				"\"executable\":\"hx\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		write_snapshot_manifest(
			&candidates,
			SnapshotMetadata::complete(
				"crates.io",
				&[],
				std::collections::BTreeMap::from([(
					"release-policy".to_owned(),
					"latest-non-yanked-stable".to_owned(),
				)]),
			),
			[PackageObservation::observed("helix-term", "1")],
		)
		.unwrap();
		let providers = root.path().join("providers.jsonl");
		fs::write(
			&providers,
			"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		)
		.unwrap();
		let metadata = ReleaseMetadata {
			release: "2026-04-16.1",
			generated_at: "2026-04-16T12:00:00Z",
			generator_revision: "0123456789abcdef",
		};

		let dataset = assemble_release(&providers, &acquisitions, &curations, &output, &metadata).unwrap();

		assert_eq!(dataset.provider_count, 1);
		for path in ["manifest.json", "providers.jsonl.zst", "bnck.sqlite.zst", "LICENSE", "SHA256SUMS"] {
			assert!(output.join(path).is_file(), "missing {path}");
		}
		let license = fs::read_to_string(output.join("LICENSE")).unwrap();
		assert!(license.contains("ISC License"));
		assert!(license.contains("Night Sail Crew"));
		let candidate_artifact = output.join("channel-manifests/crates-io.jsonl");
		assert!(candidate_artifact.is_file());
		assert_eq!(fs::read_to_string(candidate_artifact).unwrap(), fs::read_to_string(&candidates).unwrap());
		let checksums = fs::read_to_string(output.join("SHA256SUMS")).unwrap();
		assert!(checksums.contains("  LICENSE\n"));
		assert!(checksums.contains("  channel-manifests/crates-io.jsonl\n"));
		let manifest: serde_json::Value =
			serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
		assert_eq!(manifest["dataset"]["schema_version"], 1);
		assert_eq!(manifest["channels"][0]["name"], "crates.io");
	}

	#[test]
	fn rejects_absolute_input_paths() {
		let root = tempdir().unwrap();
		let acquisitions = root.path().join("acquired");
		let curations = write_curations(root.path());
		let output = root.path().join("release");
		fs::create_dir(&acquisitions).unwrap();
		let input = root.path().join("Packages");
		fs::write(&input, "Package: ripgrep\nVersion: 1\n").unwrap();
		let candidates = acquisitions.join("crates-io.jsonl");
		fs::write(
			&candidates,
			concat!(
				"{\"channel\":\"crates.io\",\"package\":\"helix-term\",\"version\":\"1\",",
				"\"executable\":\"hx\",\"evidence\":\"declared-entry-point\"}\n",
			),
		)
		.unwrap();
		write_snapshot_manifest(
			&candidates,
			SnapshotMetadata::complete(
				"crates.io",
				std::slice::from_ref(&input),
				std::collections::BTreeMap::from([(
					"release-policy".to_owned(),
					"latest-non-yanked-stable".to_owned(),
				)]),
			),
			[PackageObservation::observed("helix-term", "1")],
		)
		.unwrap();
		let providers = root.path().join("providers.jsonl");
		fs::write(
			&providers,
			"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		)
		.unwrap();
		let metadata = ReleaseMetadata {
			release: "2026-04-16.1",
			generated_at: "2026-04-16T12:00:00Z",
			generator_revision: "0123456789abcdef",
		};

		let error = assemble_release(&providers, &acquisitions, &curations, &output, &metadata).unwrap_err();
		assert!(error.to_string().contains("absolute input path"));
		assert!(!output.exists());
	}

	#[test]
	fn creates_queryable_database_with_release_metadata() {
		let root = tempdir().unwrap();
		let input = root.path().join("providers.jsonl");
		let curations = write_curations(root.path());
		let output = root.path().join("bnck.sqlite.zst");
		fs::write(&input, PROVIDERS).unwrap();
		let metadata = SqliteMetadata {
			dataset_release: "2026-04-16.1",
			generated_at: "2026-04-16T12:00:00Z",
			generator_version: "0.1.0",
			generator_revision: "0123456789abcdef",
		};

		let dataset = generate_sqlite_artifact(&input, &curations, &output, &metadata).unwrap();
		assert_eq!(dataset.provider_count, 3);

		let database = root.path().join("bnck.sqlite");
		let mut compressed = fs::File::open(output).unwrap();
		let mut decompressed = fs::File::create(&database).unwrap();
		zstd::stream::copy_decode(&mut compressed, &mut decompressed).unwrap();
		let connection = Connection::open(database).unwrap();
		let user_version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
		assert_eq!(user_version, SQLITE_SCHEMA_VERSION);
		assert_eq!(metadata_value(&connection, "schema_version"), SQLITE_SCHEMA_VERSION.to_string());
		assert_eq!(metadata_value(&connection, "dataset_release"), "2026-04-16.1");
		assert_eq!(metadata_value(&connection, "dataset_sha256"), dataset.sha256);
		assert_eq!(metadata_value(&connection, "provider_count"), "3");

		let executable_rows = query_rows(
			&connection,
			"SELECT project, channel, package FROM providers
			 WHERE executable = 'hx' ORDER BY project, channel, package",
		);
		assert_eq!(
			executable_rows,
			[
				("helix".into(), "crates.io".into(), "helix-term".into()),
				("helix".into(), "debian".into(), "helix".into()),
			]
		);

		let project_rows = query_rows(
			&connection,
			"SELECT executable, channel, package FROM providers
			 WHERE project = 'ripgrep' ORDER BY executable, channel, package",
		);
		assert_eq!(project_rows, [("rg".into(), "crates.io".into(), "ripgrep".into())]);

		let project_links = connection
			.prepare("SELECT type, url FROM project_links WHERE project = 'ripgrep' ORDER BY type, url")
			.unwrap()
			.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
			.unwrap()
			.collect::<Result<Vec<_>, _>>()
			.unwrap();
		assert_eq!(
			project_links,
			[
				("homepage".into(), "https://example.test/ripgrep".into()),
				("link".into(), "https://example.test/ripgrep/extra".into()),
				("repository".into(), "https://example.test/ripgrep/source".into()),
			]
		);

		let index_count: u32 = connection
			.query_row(
				"SELECT count(*) FROM sqlite_schema
				 WHERE type = 'index' AND name = 'providers_by_executable'",
				[],
				|row| row.get(0),
			)
			.unwrap();
		assert_eq!(index_count, 1);
	}

	fn write_curations(root: &std::path::Path) -> std::path::PathBuf {
		let curations = root.join("curations");
		let projects = curations.join("projects");
		fs::create_dir_all(&projects).unwrap();
		fs::write(
			projects.join("helix.toml"),
			"id = 'helix'\nname = 'Helix'\nhomepage = 'https://example.test/helix'\n",
		)
		.unwrap();
		fs::write(
			projects.join("ripgrep.toml"),
			concat!(
				"id = 'ripgrep'\nname = 'ripgrep'\n",
				"homepage = 'https://example.test/ripgrep'\n",
				"repository = 'https://example.test/ripgrep/source'\n",
				"links = ['https://example.test/ripgrep/extra']\n",
			),
		)
		.unwrap();
		curations
	}

	fn metadata_value(connection: &Connection, key: &str) -> String {
		connection
			.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| row.get(0))
			.unwrap()
	}

	fn query_rows(connection: &Connection, query: &str) -> Vec<(String, String, String)> {
		connection
			.prepare(query)
			.unwrap()
			.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
			.unwrap()
			.collect::<Result<_, _>>()
			.unwrap()
	}
}
