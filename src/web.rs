use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, ensure};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use minijinja::{Environment, Value, context};
#[cfg(debug_assertions)]
use minijinja::path_loader;
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::model::{CHANNELS, Provider};
use crate::release::SQLITE_SCHEMA_VERSION;

const SEARCH_PAGE_SIZE: u64 = 30;
const SEARCH_RELATED_NAME_LIMIT: u64 = 3;
const MAX_QUERY_CHARACTERS: usize = 100;

/* Discovery folds ASCII case; grouping and detail lookups preserve exact identities. */
const SEARCH_MATCHES: &str = "WITH matching_names AS (
	SELECT CASE WHEN ?3 = 'all' THEN project ELSE executable END AS name, executable AS matched_name
	FROM providers
	WHERE ?3 != 'project' AND instr(lower(executable), lower(?1)) > 0 AND (?2 = '' OR channel = ?2)
	UNION
	SELECT project AS name, project AS matched_name
	FROM providers
	WHERE ?3 != 'executable' AND instr(lower(project), lower(?1)) > 0 AND (?2 = '' OR channel = ?2)
), ranked_names AS (
	SELECT name, matched_name,
		CASE
			WHEN matched_name = ?1 THEN 0
			WHEN lower(matched_name) = lower(?1) THEN 1
			WHEN instr(lower(matched_name), lower(?1)) = 1 THEN 2
			ELSE 3
		END AS match_rank
	FROM matching_names
), best_ranks AS (
	SELECT name, min(match_rank) AS match_rank FROM ranked_names GROUP BY name
), matches AS (
	SELECT name, match_rank, min(length(matched_name)) AS match_length
	FROM ranked_names JOIN best_ranks USING (name, match_rank)
	GROUP BY name, match_rank
)";

pub async fn serve(database_path: &Path, address: SocketAddr) -> Result<()> {
	let database = Arc::new(Database::open(database_path)?);
	let router = router(database.clone());
	let listener = tokio::net::TcpListener::bind(address)
		.await
		.with_context(|| format!("bind web server to {address}"))?;
	let local_address = listener.local_addr()?;

	eprintln!("serving bnck dataset {} at http://{local_address}", database.metadata_value("dataset_release"));
	axum::serve(listener, router)
		.with_graceful_shutdown(shutdown_signal())
		.await
		.context("run web server")?;
	Ok(())
}

fn router(database: Arc<Database>) -> Router {
	Router::new()
		.route("/", get(home))
		.route("/about", get(about))
		.route("/projects/{project}", get(project))
		.route("/executables/{executable}", get(executable))
		.route("/style.css", get(stylesheet))
		.fallback(not_found)
		.with_state(database)
}

async fn shutdown_signal() {
	if let Err(error) = tokio::signal::ctrl_c().await {
		eprintln!("failed to listen for shutdown signal: {error}");
	}
}

struct Database {
	path: PathBuf,
	_temporary_database: Option<NamedTempFile>,
	metadata: BTreeMap<String, String>,
	stats: DatasetStats,
	channels: Vec<ChannelStats>,
	example: Option<Provider>,
}

impl Database {
	fn open(source: &Path) -> Result<Self> {
		let (path, temporary_database) = materialize_database(source)?;
		let connection = open_read_only(&path)?;
		let user_version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
		ensure!(
			user_version == SQLITE_SCHEMA_VERSION,
			"unsupported SQLite schema version {user_version}; expected {SQLITE_SCHEMA_VERSION}"
		);

		let metadata = read_metadata(&connection)?;
		let metadata_schema = required_metadata(&metadata, "schema_version")?
			.parse::<u32>()
			.context("parse metadata schema_version")?;
		ensure!(metadata_schema == user_version, "SQLite and metadata schema versions disagree");
		for key in [
			"dataset_schema_version",
			"dataset_release",
			"dataset_sha256",
			"provider_count",
			"generated_at",
			"generator_version",
			"generator_revision",
		] {
			required_metadata(&metadata, key)?;
		}

		let stats = read_stats(&connection)?;
		let expected_provider_count = required_metadata(&metadata, "provider_count")?
			.parse::<u64>()
			.context("parse metadata provider_count")?;
		ensure!(stats.providers == expected_provider_count, "provider count does not match database metadata");

		let channels = read_channel_stats(&connection)?;
		let example = read_example(&connection)?;
		Ok(Self { path, _temporary_database: temporary_database, metadata, stats, channels, example })
	}

	fn connection(&self) -> Result<Connection> {
		open_read_only(&self.path)
	}

	fn metadata_value(&self, key: &str) -> &str {
		self.metadata.get(key).map(String::as_str).unwrap_or("unknown")
	}

	fn search(&self, search: &SearchQuery) -> Result<SearchResults> {
		let connection = self.connection()?;
		let query = search.query();
		let parameters = params![query, search.channel, search.kind.as_str()];
		let total: u64 =
			connection
				.query_row(&format!("{SEARCH_MATCHES} SELECT count(*) FROM matches"), parameters, |row| row.get(0))?;
		let mut statement = connection.prepare(&format!(
			"{SEARCH_MATCHES} SELECT name FROM matches
			 ORDER BY match_rank, CASE WHEN ?1 != '' THEN match_length END, name
			 LIMIT ?4 OFFSET ?5"
		))?;
		let offset = u64::from(search.page.saturating_sub(1)) * SEARCH_PAGE_SIZE;
		/* Page groups before reading their executable lists so a project never crosses a page boundary. */
		let names = statement
			.query_map(params![query, search.channel, search.kind.as_str(), SEARCH_PAGE_SIZE, offset], |row| {
				row.get::<_, String>(0)
			})?;
		let rows = names
			.map(|name| read_search_result(&connection, name?, search))
			.collect::<Result<Vec<_>>>()?;
		let end = offset + rows.len() as u64;
		Ok(SearchResults {
			start: if rows.is_empty() { 0 } else { offset + 1 },
			end,
			total,
			previous: (search.page > 1).then(|| search.page_url(search.page - 1)),
			next: (end < total).then(|| search.page_url(search.page + 1)),
			rows,
		})
	}

	fn project_providers(&self, project: &str) -> Result<Vec<ProjectProvider>> {
		let connection = self.connection()?;
		let mut statement = connection.prepare(
			"SELECT executable, channel, package FROM providers
			 WHERE project = ?1 ORDER BY executable, channel, package",
		)?;
		let rows = statement.query_map([project], |row| {
			Ok(ProjectProvider { executable: row.get(0)?, channel: row.get(1)?, package: row.get(2)? })
		})?;
		Ok(rows.collect::<rusqlite::Result<_>>()?)
	}

	fn project_links(&self, project: &str) -> Result<Vec<ProjectLink>> {
		let connection = self.connection()?;
		let mut statement = connection.prepare(
			"SELECT type, url FROM project_links
			 WHERE project = ?1
			 ORDER BY CASE type
				WHEN 'homepage' THEN 0
				WHEN 'repository' THEN 1
				WHEN 'documentation' THEN 2
				WHEN 'issue tracker' THEN 3
				ELSE 4
			 END, url",
		)?;
		let rows =
			statement.query_map([project], |row| Ok(ProjectLink { link_type: row.get(0)?, url: row.get(1)? }))?;
		Ok(rows.collect::<rusqlite::Result<_>>()?)
	}

	fn executable_providers(&self, executable: &str) -> Result<Vec<ExecutableProvider>> {
		let connection = self.connection()?;
		let mut statement = connection.prepare(
			"SELECT project, channel, package FROM providers
			 WHERE executable = ?1 ORDER BY project, channel, package",
		)?;
		let rows = statement.query_map([executable], |row| {
			Ok(ExecutableProvider { project: row.get(0)?, channel: row.get(1)?, package: row.get(2)? })
		})?;
		Ok(rows.collect::<rusqlite::Result<_>>()?)
	}
}

fn materialize_database(source: &Path) -> Result<(PathBuf, Option<NamedTempFile>)> {
	ensure!(source.is_file(), "database does not exist: {}", source.display());
	if source.extension().is_none_or(|extension| extension != "zst") {
		return Ok((source.to_owned(), None));
	}

	let input = File::open(source).with_context(|| format!("open {}", source.display()))?;
	let mut temporary = NamedTempFile::new().context("create temporary SQLite database")?;
	zstd::stream::copy_decode(BufReader::new(input), temporary.as_file_mut())
		.with_context(|| format!("decompress {}", source.display()))?;
	temporary.flush()?;
	temporary.as_file().sync_all()?;
	let path = temporary.path().to_owned();
	Ok((path, Some(temporary)))
}

fn open_read_only(path: &Path) -> Result<Connection> {
	let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
	let connection = Connection::open_with_flags(path, flags)
		.with_context(|| format!("open SQLite database {} read-only", path.display()))?;
	connection.execute_batch("PRAGMA query_only = ON;")?;
	Ok(connection)
}

fn read_metadata(connection: &Connection) -> Result<BTreeMap<String, String>> {
	let mut statement = connection.prepare("SELECT key, value FROM metadata ORDER BY key")?;
	let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn required_metadata<'a>(metadata: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
	metadata
		.get(key)
		.map(String::as_str)
		.filter(|value| !value.is_empty())
		.ok_or_else(|| anyhow!("database metadata is missing {key}"))
}

fn read_stats(connection: &Connection) -> Result<DatasetStats> {
	connection
		.query_row(
			"SELECT count(*), count(DISTINCT project), count(DISTINCT executable), count(DISTINCT channel)
			 FROM providers",
			[],
			|row| {
				Ok(DatasetStats {
					providers: row.get(0)?,
					projects: row.get(1)?,
					executables: row.get(2)?,
					channels: row.get(3)?,
				})
			},
		)
		.context("read dataset statistics")
}

fn read_channel_stats(connection: &Connection) -> Result<Vec<ChannelStats>> {
	let mut statement = connection.prepare(
		"SELECT channel, count(*), count(DISTINCT project), count(DISTINCT package)
		 FROM providers GROUP BY channel ORDER BY channel",
	)?;
	let rows = statement.query_map([], |row| {
		Ok(ChannelStats { name: row.get(0)?, providers: row.get(1)?, projects: row.get(2)?, packages: row.get(3)? })
	})?;
	Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn read_example(connection: &Connection) -> Result<Option<Provider>> {
	let mut statement = connection.prepare(
		"SELECT project, executable, channel, package FROM providers
		 ORDER BY CASE executable WHEN 'rg' THEN 0 WHEN 'hx' THEN 1 ELSE 2 END,
		 project, executable, channel, package LIMIT 1",
	)?;
	let mut rows = statement.query([])?;
	rows.next()?
		.map(|row| {
			Ok(Provider { project: row.get(0)?, executable: row.get(1)?, channel: row.get(2)?, package: row.get(3)? })
		})
		.transpose()
}

#[derive(Clone, Copy, Serialize)]
struct DatasetStats {
	providers: u64,
	projects: u64,
	executables: u64,
	channels: u64,
}

#[derive(Serialize)]
struct ChannelStats {
	name: String,
	providers: u64,
	projects: u64,
	packages: u64,
}

#[derive(Serialize)]
struct SearchResults {
	rows: Vec<SearchResult>,
	total: u64,
	start: u64,
	end: u64,
	previous: Option<String>,
	next: Option<String>,
}

#[derive(Serialize)]
struct SearchResult {
	name: String,
	identity: SearchIdentity,
	project_executables: Vec<String>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum IdentityKind {
	Executable,
	Project,
}

impl IdentityKind {
	fn columns(self) -> (&'static str, &'static str) {
		match self {
			Self::Executable => ("executable", "project"),
			Self::Project => ("project", "executable"),
		}
	}
}

#[derive(Serialize)]
struct SearchIdentity {
	kind: IdentityKind,
	provider_count: u64,
	related_count: u64,
	channel_count: u64,
	related_names: Vec<String>,
}

fn read_search_result(connection: &Connection, name: String, search: &SearchQuery) -> Result<SearchResult> {
	let kind = match search.kind {
		SearchKind::All | SearchKind::Project => IdentityKind::Project,
		SearchKind::Executable => IdentityKind::Executable,
	};
	let identity = read_search_identity(connection, &name, kind, &search.channel)?;
	let project_executables =
		if search.kind == SearchKind::All { read_search_executables(connection, &name, search)? } else { Vec::new() };
	Ok(SearchResult { name, identity, project_executables })
}

fn read_search_identity(
	connection: &Connection,
	name: &str,
	kind: IdentityKind,
	channel: &str,
) -> Result<SearchIdentity> {
	/* Column names come only from the identity enum; dataset and search values remain bound parameters. */
	let (column, related_column) = kind.columns();
	let predicate = format!("FROM providers WHERE {column} = ?1 AND (?2 = '' OR channel = ?2)");
	let mut statement = connection.prepare_cached(&format!(
		"SELECT count(*), count(DISTINCT {related_column}), count(DISTINCT channel) {predicate}"
	))?;
	let mut identity = statement.query_row(params![name, channel], |row| {
		Ok(SearchIdentity {
			kind,
			provider_count: row.get(0)?,
			related_count: row.get(1)?,
			channel_count: row.get(2)?,
			related_names: Vec::new(),
		})
	})?;
	if identity.related_count <= SEARCH_RELATED_NAME_LIMIT {
		let mut statement = connection
			.prepare_cached(&format!("SELECT DISTINCT {related_column} {predicate} ORDER BY {related_column}"))?;
		identity.related_names = statement
			.query_map(params![name, channel], |row| row.get(0))?
			.collect::<rusqlite::Result<_>>()?;
	}
	Ok(identity)
}

fn read_search_executables(connection: &Connection, project: &str, search: &SearchQuery) -> Result<Vec<String>> {
	/* Keep exact and partial command matches in the preview, even for projects with many executables. */
	let mut statement = connection.prepare_cached(
		"SELECT DISTINCT executable FROM providers
		 WHERE project = ?1 AND (?2 = '' OR channel = ?2)
		 ORDER BY CASE
			WHEN executable = ?3 THEN 0
			WHEN lower(executable) = lower(?3) THEN 1
			WHEN instr(lower(executable), lower(?3)) = 1 THEN 2
			WHEN instr(lower(executable), lower(?3)) > 0 THEN 3
			ELSE 4
		 END, CASE WHEN ?3 != '' THEN length(executable) END, executable",
	)?;
	Ok(statement
		.query_map(params![project, search.channel, search.query()], |row| row.get(0))?
		.collect::<rusqlite::Result<_>>()?)
}

struct ProjectProvider {
	executable: String,
	channel: String,
	package: String,
}

struct ExecutableProvider {
	project: String,
	channel: String,
	package: String,
}

#[derive(Serialize)]
struct ProjectLink {
	link_type: String,
	url: String,
}

#[derive(Serialize)]
struct ExecutableGroup {
	executable: String,
	channels: Vec<ChannelGroup>,
}

#[derive(Serialize)]
struct ProjectGroup {
	project: String,
	channels: Vec<ChannelGroup>,
}

#[derive(Serialize)]
struct ChannelGroup {
	channel: String,
	packages: Vec<PackageReference>,
}

#[derive(Serialize)]
struct PackageReference {
	name: String,
	url: Option<String>,
}

fn group_project_executables(providers: Vec<ProjectProvider>) -> Vec<ExecutableGroup> {
	let mut executables = BTreeMap::<String, Vec<(String, String)>>::new();
	for provider in providers {
		executables
			.entry(provider.executable)
			.or_default()
			.push((provider.channel, provider.package));
	}
	executables
		.into_iter()
		.map(|(executable, providers)| ExecutableGroup { executable, channels: group_channels(providers) })
		.collect()
}

fn group_executable_projects(providers: Vec<ExecutableProvider>) -> Vec<ProjectGroup> {
	let mut projects = BTreeMap::<String, Vec<(String, String)>>::new();
	for provider in providers {
		projects
			.entry(provider.project)
			.or_default()
			.push((provider.channel, provider.package));
	}
	projects
		.into_iter()
		.map(|(project, providers)| ProjectGroup { project, channels: group_channels(providers) })
		.collect()
}

fn group_channels(providers: Vec<(String, String)>) -> Vec<ChannelGroup> {
	let mut channels = BTreeMap::<String, Vec<String>>::new();
	for (channel, package) in providers {
		channels.entry(channel).or_default().push(package);
	}
	channels
		.into_iter()
		.map(|(channel, packages)| ChannelGroup {
			packages: packages
				.into_iter()
				.map(|name| PackageReference { url: canonical_package_url(&channel, &name), name })
				.collect(),
			channel,
		})
		.collect()
}

fn canonical_package_url(channel: &str, package: &str) -> Option<String> {
	let package_path = encode_package_path(package);
	match channel {
		"arch" => Some(format!("https://archlinux.org/packages/?q={}", encode_path_segment(package))),
		"crates.io" => Some(format!("https://crates.io/crates/{package_path}")),
		"debian" => Some(format!("https://packages.debian.org/{package_path}")),
		"fedora" => Some(format!("https://packages.fedoraproject.org/pkgs/{package_path}/{package_path}/")),
		"homebrew" => homebrew_package_url(package),
		"npm" => Some(format!("https://www.npmjs.com/package/{package_path}")),
		"pypi" => Some(format!("https://pypi.org/project/{package_path}/")),
		_ => None,
	}
}

fn homebrew_package_url(package: &str) -> Option<String> {
	if let Some(name) = package.strip_prefix("homebrew/core/") {
		return Some(format!("https://formulae.brew.sh/formula/{}.html", encode_path_segment(name)));
	}
	package
		.strip_prefix("homebrew/cask/")
		.map(|name| format!("https://formulae.brew.sh/cask/{}.html", encode_path_segment(name)))
}

fn encode_package_path(package: &str) -> String {
	package.split('/').map(encode_path_segment).collect::<Vec<_>>().join("/")
}

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum SearchKind {
	#[default]
	All,
	Executable,
	Project,
}

impl SearchKind {
	fn as_str(self) -> &'static str {
		match self {
			Self::All => "all",
			Self::Executable => "executable",
			Self::Project => "project",
		}
	}
}

#[derive(Deserialize)]
struct SearchQuery {
	q: Option<String>,
	#[serde(default)]
	kind: SearchKind,
	#[serde(default)]
	channel: String,
	#[serde(default = "first_page")]
	page: u32,
}

fn first_page() -> u32 {
	1
}

impl SearchQuery {
	fn query(&self) -> &str {
		self.q.as_deref().unwrap_or_default().trim()
	}

	fn page_url(&self, page: u32) -> String {
		format!(
			"/?q={}&kind={}&channel={}&page={page}",
			encode_path_segment(self.query()),
			self.kind.as_str(),
			encode_path_segment(&self.channel),
		)
	}
}

async fn home(State(database): State<Arc<Database>>, search: Result<Query<SearchQuery>, QueryRejection>) -> Response {
	let search = match search {
		Ok(Query(search)) => search,
		Err(_) => {
			return error_response(
				&database,
				StatusCode::BAD_REQUEST,
				"Invalid search",
				"Select a type using the form below. Page numbers start at 1.",
			);
		},
	};
	database_task(database, move |database| search_page(database, search)).await
}

fn search_page(database: &Database, search: SearchQuery) -> Response {
	if search.page == 0 || (!search.channel.is_empty() && !CHANNELS.contains(&search.channel.as_str())) {
		return error_response(
			database,
			StatusCode::BAD_REQUEST,
			"Invalid search",
			"Select a channel using the form below. Page numbers start at 1.",
		);
	}
	if search.q.is_none() && search.kind == SearchKind::All && search.channel.is_empty() && search.page == 1 {
		return render_response(
			database,
			StatusCode::OK,
			"home.html",
			"Executable name database",
			context! { stats => database.stats, channels => &database.channels, example => &database.example },
		);
	}
	let query = search.query();
	if query.chars().count() > MAX_QUERY_CHARACTERS {
		return error_response(database, StatusCode::BAD_REQUEST, "Query too long", "Use at most 100 characters.");
	}

	match database.search(&search) {
		Ok(results) if search.page > 1 && results.rows.is_empty() => error_response(
			database,
			StatusCode::NOT_FOUND,
			"No results on this page",
			"Start a new search or browse names.",
		),
		Ok(results) => render_response(
			database,
			StatusCode::OK,
			"search.html",
			&if query.is_empty() { "Browse".to_owned() } else { format!("Search: {query}") },
			context! {
				query, results, kind => search.kind, channel => search.channel,
				related_name_limit => SEARCH_RELATED_NAME_LIMIT,
			},
		),
		Err(error) => database_error(database, error),
	}
}

/* SQLite work runs off the async executor; each task owns a read-only connection. */
async fn database_task(database: Arc<Database>, task: impl FnOnce(&Database) -> Response + Send + 'static) -> Response {
	let worker_database = database.clone();
	match tokio::task::spawn_blocking(move || task(&worker_database)).await {
		Ok(response) => response,
		Err(error) => database_error(&database, error.into()),
	}
}

async fn project(State(database): State<Arc<Database>>, AxumPath(project): AxumPath<String>) -> Response {
	database_task(database, move |database| project_page(database, &project)).await
}

fn project_page(database: &Database, project: &str) -> Response {
	match database.project_providers(project) {
		Ok(providers) if providers.is_empty() => error_response(
			database,
			StatusCode::NOT_FOUND,
			"Project not found",
			"No project with this exact ID is listed. Check spelling and case.",
		),
		Ok(providers) => match database.project_links(project) {
			Ok(links) => {
				let provider_count = providers.len() as u64;
				let executables = group_project_executables(providers);
				let executable_count = executables.len() as u64;
				render_response(
					database,
					StatusCode::OK,
					"project.html",
					project,
					context! { project, links, executables, executable_count, provider_count },
				)
			},
			Err(error) => database_error(database, error),
		},
		Err(error) => database_error(database, error),
	}
}

async fn executable(State(database): State<Arc<Database>>, AxumPath(executable): AxumPath<String>) -> Response {
	database_task(database, move |database| executable_page(database, &executable)).await
}

fn executable_page(database: &Database, executable: &str) -> Response {
	match database.executable_providers(executable) {
		Ok(providers) if providers.is_empty() => error_response(
			database,
			StatusCode::NOT_FOUND,
			"Executable not found",
			"No executable with this exact name is listed. Check spelling and case.",
		),
		Ok(providers) => {
			let provider_count = providers.len() as u64;
			let projects = group_executable_projects(providers);
			let project_count = projects.len() as u64;
			render_response(
				database,
				StatusCode::OK,
				"executable.html",
				executable,
				context! { executable, projects, project_count, provider_count },
			)
		},
		Err(error) => database_error(database, error),
	}
}

async fn about(State(database): State<Arc<Database>>) -> Response {
	render_response(
		&database,
		StatusCode::OK,
		"dataset.html",
		"Dataset",
		context! { metadata => &database.metadata, stats => database.stats, channels => &database.channels },
	)
}

async fn stylesheet() -> Response {
	match stylesheet_content() {
		Ok(stylesheet) => (
			StatusCode::OK,
			[
				(header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8")),
				(header::CACHE_CONTROL, HeaderValue::from_static(asset_cache_control())),
				(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
			],
			stylesheet,
		)
			.into_response(),
		Err(error) => {
			eprintln!("website stylesheet error: {error:#}");
			(StatusCode::INTERNAL_SERVER_ERROR, "stylesheet unavailable").into_response()
		},
	}
}

async fn not_found(State(database): State<Arc<Database>>) -> Response {
	error_response(&database, StatusCode::NOT_FOUND, "Page not found", "Check the address or use the search below.")
}

fn database_error(database: &Database, error: anyhow::Error) -> Response {
	eprintln!("website database error: {error:#}");
	error_response(
		database,
		StatusCode::INTERNAL_SERVER_ERROR,
		"Lookup unavailable",
		"The lookup failed. Try again later.",
	)
}

fn error_response(database: &Database, status: StatusCode, title: &str, message: &str) -> Response {
	render_response(database, status, "error.html", title, context! { status => status.as_u16(), title, message })
}

fn render_response(database: &Database, status: StatusCode, template_name: &str, title: &str, page: Value) -> Response {
	match render_page(database, template_name, title, page) {
		Ok(body) => html_response(status, body),
		Err(error) => {
			eprintln!("website template error: {error:#}");
			(StatusCode::INTERNAL_SERVER_ERROR, "The page could not be displayed.").into_response()
		},
	}
}

fn render_page(database: &Database, template_name: &str, title: &str, page: Value) -> Result<String> {
	let environment = template_environment()?;
	let template = environment
		.get_template(template_name)
		.with_context(|| format!("load website template {template_name}"))?;
	template
		.render(context! {
			site => context! {
				title,
				release => database.metadata_value("dataset_release"),
				channel_keys => CHANNELS,
				section => if template_name == "dataset.html" { "dataset" } else { "index" },
			},
			page,
		})
		.with_context(|| format!("render website template {template_name}"))
}

fn template_environment() -> Result<Environment<'static>> {
	let mut environment = Environment::new();
	environment.add_filter("number", format_number);
	environment.add_filter("path_segment", encode_path_segment);
	environment.add_filter("channel_label", |channel: &str| channel_label(channel).to_owned());

	#[cfg(debug_assertions)]
	environment.set_loader(path_loader(Path::new(env!("CARGO_MANIFEST_DIR")).join("web/templates")));

	#[cfg(not(debug_assertions))]
	for (name, source) in [
		("base.html", include_str!("../web/templates/base.html")),
		("error.html", include_str!("../web/templates/error.html")),
		("executable.html", include_str!("../web/templates/executable.html")),
		("home.html", include_str!("../web/templates/home.html")),
		("dataset.html", include_str!("../web/templates/dataset.html")),
		("components.html", include_str!("../web/templates/components.html")),
		("project.html", include_str!("../web/templates/project.html")),
		("search.html", include_str!("../web/templates/search.html")),
	] {
		environment.add_template(name, source)?;
	}

	Ok(environment)
}

#[cfg(debug_assertions)]
fn stylesheet_content() -> Result<String> {
	let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/style.css");
	std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

#[cfg(not(debug_assertions))]
fn stylesheet_content() -> Result<String> {
	Ok(include_str!("../web/style.css").to_owned())
}

const fn asset_cache_control() -> &'static str {
	if cfg!(debug_assertions) { "no-store" } else { "public, max-age=3600" }
}

fn html_response(status: StatusCode, body: String) -> Response {
	(
		status,
		[
			(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
			(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
			(
				header::CONTENT_SECURITY_POLICY,
				HeaderValue::from_static(
					"default-src 'none'; style-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
				),
			),
		],
		body,
	)
		.into_response()
}

fn encode_path_segment(value: &str) -> String {
	let mut encoded = String::with_capacity(value.len());
	for byte in value.bytes() {
		if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
			encoded.push(char::from(byte));
		} else {
			encoded.push_str(&format!("%{byte:02X}"));
		}
	}
	encoded
}

fn channel_label(channel: &str) -> &str {
	match channel {
		"arch" => "Arch Linux",
		"debian" => "Debian",
		"fedora" => "Fedora",
		"homebrew" => "Homebrew",
		"pypi" => "PyPI",
		_ => channel,
	}
}

fn format_number(value: u64) -> String {
	let digits = value.to_string();
	let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
	for (index, character) in digits.chars().enumerate() {
		if index > 0 && (digits.len() - index).is_multiple_of(3) {
			formatted.push(',');
		}
		formatted.push(character);
	}
	formatted
}

#[cfg(test)]
#[path = "web/tests.rs"]
mod query_tests;

#[cfg(test)]
mod tests {
	use std::fs;

	use super::query_tests::search_query;

	use tempfile::tempdir;

	use super::{
		Database, canonical_package_url, encode_path_segment, format_number, group_executable_projects,
		group_project_executables, render_page, stylesheet_content,
	};
	use crate::release::{SqliteMetadata, generate_sqlite_artifact};

	const PROVIDERS: &str = concat!(
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"crates.io\",\"package\":\"helix-term\"}\n",
		"{\"project\":\"helix\",\"executable\":\"hx\",\"channel\":\"debian\",\"package\":\"helix\"}\n",
		"{\"project\":\"ripgrep\",\"executable\":\"rg\",\"channel\":\"crates.io\",\"package\":\"ripgrep\"}\n",
	);

	#[test]
	fn opens_compressed_database_and_queries_both_directions() {
		let root = tempdir().unwrap();
		let input = root.path().join("providers.jsonl");
		let curations = root.path().join("curations");
		let projects = curations.join("projects");
		let artifact = root.path().join("bnck.sqlite.zst");
		fs::create_dir_all(&projects).unwrap();
		fs::write(
			projects.join("helix.toml"),
			concat!(
				"id = 'helix'\nname = 'Helix'\n",
				"homepage = 'https://helix-editor.com/'\n",
				"repository = 'https://github.com/helix-editor/helix'\n",
			),
		)
		.unwrap();
		fs::write(projects.join("ripgrep.toml"), "id = 'ripgrep'\nname = 'ripgrep'\n").unwrap();
		fs::write(&input, PROVIDERS).unwrap();
		generate_sqlite_artifact(
			&input,
			&curations,
			&artifact,
			&SqliteMetadata {
				dataset_release: "2026-04-16.1",
				generated_at: "2026-04-16T12:00:00Z",
				generator_version: "0.1.0",
				generator_revision: "0123456789abcdef",
			},
		)
		.unwrap();
		let artifact_before = fs::read(&artifact).unwrap();

		let database = Database::open(&artifact).unwrap();
		assert_eq!(database.stats.providers, 3);
		assert_eq!(database.stats.projects, 2);
		assert_eq!(database.search(&search_query("HX")).unwrap().rows[0].name, "helix");
		assert_eq!(database.project_providers("helix").unwrap().len(), 2);
		assert_eq!(database.executable_providers("rg").unwrap()[0].project, "ripgrep");
		assert!(database.connection().unwrap().execute("DELETE FROM providers", []).is_err());
		let home = render_page(
			&database,
			"home.html",
			"Executable name database",
			minijinja::context! { stats => database.stats, channels => &database.channels, example => &database.example },
		)
		.unwrap();
		assert!(home.contains("<h1>Executable lookup</h1>"));
		assert!(home.contains("href='/executables/rg'"));
		assert!(home.contains("aria-label='bnck(1): bin name check, home'"));
		assert!(home.contains("<span class='brand-description'>bin name check</span>"));
		assert!(!home.contains("brand-mark"));
		assert!(!home.contains("The executable index"));

		let results = database.search(&search_query("<script>")).unwrap();
		let search = render_page(
			&database,
			"search.html",
			"Search",
			minijinja::context! { query => "<script>", results, kind => "all", channel => "" },
		)
		.unwrap();
		assert!(search.contains("&lt;script&gt;"));
		assert!(!search.contains("<script>"));

		let project_providers = database.project_providers("helix").unwrap();
		let links = database.project_links("helix").unwrap();
		assert_eq!(links[0].link_type, "homepage");
		assert_eq!(links[0].url, "https://helix-editor.com/");
		assert_eq!(links[1].link_type, "repository");
		assert_eq!(links[1].url, "https://github.com/helix-editor/helix");
		let project_provider_count = project_providers.len() as u64;
		let executables = group_project_executables(project_providers);
		let executable_count = executables.len() as u64;
		let project = render_page(
			&database,
			"project.html",
			"helix",
			minijinja::context! {
				project => "helix",
				links,
				provider_count => project_provider_count,
				executables,
				executable_count,
			},
		)
		.unwrap();
		assert!(project.contains("https:&#x2f;&#x2f;crates.io&#x2f;crates&#x2f;helix-term"));
		assert!(project.contains("https:&#x2f;&#x2f;packages.debian.org&#x2f;helix"));
		let heading = project.split("<div class='content-layout'>").next().unwrap();
		assert!(heading.contains("Homepage:"));
		assert!(heading.contains("Repository:"));
		assert!(heading.contains(concat!(
			"<a href='https:&#x2f;&#x2f;helix-editor.com&#x2f;' rel='external'>",
			"https:&#x2f;&#x2f;helix-editor.com&#x2f;</a>"
		)));
		assert!(project.contains("<code>helix-term</code>"));
		assert_eq!(project.matches("href='/executables/hx'").count(), 1);

		let executable_providers = database.executable_providers("hx").unwrap();
		let executable_provider_count = executable_providers.len() as u64;
		let projects = group_executable_projects(executable_providers);
		let project_count = projects.len() as u64;
		let executable = render_page(
			&database,
			"executable.html",
			"hx",
			minijinja::context! {
				executable => "hx",
				provider_count => executable_provider_count,
				projects,
				project_count,
			},
		)
		.unwrap();
		assert!(executable.contains("crates.io"));
		assert!(executable.contains("Debian"));
		assert_eq!(executable.matches("href='/projects/helix'").count(), 1);

		let dataset = render_page(
			&database,
			"dataset.html",
			"Dataset",
			minijinja::context! { metadata => &database.metadata, stats => database.stats, channels => &database.channels },
		)
		.unwrap();
		assert!(dataset.contains("<h2>Fields</h2>"));
		assert!(!dataset.contains("SQLite"));
		assert!(!dataset.contains("SHA-256"));
		assert!(!dataset.contains("SELECT"));
		assert!(dataset.contains("id='bnck-database'"));
		assert!(dataset.contains("2026-04-16.1"));
		assert!(dataset.contains("2026-04-16 12:00:00 UTC"));

		let error = render_page(
			&database,
			"error.html",
			"Not found",
			minijinja::context! { status => 404, title => "Not found", message => "Missing" },
		)
		.unwrap();
		assert!(error.contains("Error 404"));
		assert!(stylesheet_content().unwrap().contains(".relation-card"));
		assert_eq!(fs::read(&artifact).unwrap(), artifact_before);
	}

	#[test]
	fn formats_template_values() {
		assert_eq!(encode_path_segment("cargo add"), "cargo%20add");
		assert_eq!(encode_path_segment("é"), "%C3%A9");
		assert_eq!(format_number(1_234_567), "1,234,567");
		assert_eq!(
			canonical_package_url("arch", "ripgrep").as_deref(),
			Some("https://archlinux.org/packages/?q=ripgrep")
		);
		assert_eq!(canonical_package_url("crates.io", "ripgrep").as_deref(), Some("https://crates.io/crates/ripgrep"));
		assert_eq!(canonical_package_url("debian", "ripgrep").as_deref(), Some("https://packages.debian.org/ripgrep"));
		assert_eq!(
			canonical_package_url("fedora", "ripgrep").as_deref(),
			Some("https://packages.fedoraproject.org/pkgs/ripgrep/ripgrep/")
		);
		assert_eq!(
			canonical_package_url("homebrew", "homebrew/core/ripgrep").as_deref(),
			Some("https://formulae.brew.sh/formula/ripgrep.html")
		);
		assert_eq!(
			canonical_package_url("npm", "@scope/tool").as_deref(),
			Some("https://www.npmjs.com/package/%40scope/tool")
		);
		assert_eq!(canonical_package_url("pypi", "black").as_deref(), Some("https://pypi.org/project/black/"));
	}
}
