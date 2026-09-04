use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use crate::model::{ProviderDataset, read_provider_dataset};

pub const SQLITE_SCHEMA_VERSION: u32 = 1;

pub struct SqliteMetadata<'a> {
	pub dataset_release: &'a str,
	pub generated_at: &'a str,
	pub generator_version: &'a str,
	pub generator_revision: &'a str,
}

pub fn generate_sqlite_artifact(input: &Path, output: &Path, metadata: &SqliteMetadata<'_>) -> Result<ProviderDataset> {
	let parent = output
		.parent()
		.filter(|path| !path.as_os_str().is_empty())
		.unwrap_or(Path::new("."));
	fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	let database =
		NamedTempFile::new_in(parent).with_context(|| format!("create temporary file in {}", parent.display()))?;
	let dataset = write_sqlite_database(input, database.path(), metadata)?;
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

fn write_sqlite_database(input: &Path, output: &Path, metadata: &SqliteMetadata<'_>) -> Result<ProviderDataset> {
	validate_metadata(metadata)?;
	let mut connection = Connection::open(output)?;
	let transaction = connection.transaction()?;
	transaction.execute_batch(
		"CREATE TABLE metadata (
			key TEXT PRIMARY KEY,
			value TEXT NOT NULL
		) WITHOUT ROWID;

		CREATE TABLE providers (
			project TEXT NOT NULL,
			executable TEXT NOT NULL,
			channel TEXT NOT NULL,
			package TEXT NOT NULL,
			PRIMARY KEY (project, executable, channel, package)
		) WITHOUT ROWID;

		CREATE INDEX providers_by_executable
		ON providers (executable, project, channel, package);",
	)?;

	let dataset = {
		let mut statement = transaction.prepare_cached(
			"INSERT INTO providers (project, executable, channel, package)
			 VALUES (?1, ?2, ?3, ?4)",
		)?;
		read_provider_dataset(input, |provider| {
			statement.execute(params![provider.project, provider.executable, provider.channel, provider.package])?;
			Ok(())
		})?
	};

	let metadata_values = [
		("schema_version", SQLITE_SCHEMA_VERSION.to_string()),
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

fn validate_metadata(metadata: &SqliteMetadata<'_>) -> Result<()> {
	for (name, value) in [
		("dataset release", metadata.dataset_release),
		("generation timestamp", metadata.generated_at),
		("generator version", metadata.generator_version),
		("generator revision", metadata.generator_revision),
	] {
		ensure!(!value.trim().is_empty(), "{name} is empty");
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use rusqlite::Connection;
	use tempfile::tempdir;

	use super::{SQLITE_SCHEMA_VERSION, SqliteMetadata, generate_sqlite_artifact};

	const PROVIDERS: &str = concat!(
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n",
		"{\"project\":\"ripgrep\",\"executable\":\"rg\",\"channel\":\"crates.io\",\"package\":\"ripgrep\"}\n",
	);

	#[test]
	fn creates_queryable_database_with_release_metadata() {
		let root = tempdir().unwrap();
		let input = root.path().join("providers.jsonl");
		let output = root.path().join("bnck.sqlite.zst");
		fs::write(&input, PROVIDERS).unwrap();
		let metadata = SqliteMetadata {
			dataset_release: "2026-04-16.1",
			generated_at: "2026-04-16T12:00:00Z",
			generator_version: "0.1.0",
			generator_revision: "0123456789abcdef",
		};

		let dataset = generate_sqlite_artifact(&input, &output, &metadata).unwrap();
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
