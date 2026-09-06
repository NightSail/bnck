#[cfg(test)]
#[path = "static_site_tests.rs"]
mod tests;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use minijinja::{Environment, Value, context};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::exports::{ExportQuery, export_bytes};
use super::{
	Database, HTML_CSP, ProjectLink, ProjectProvider, SiteAssets, build_project_matrix, channel_label,
	group_executable_projects, render_page_with_assets, template_environment,
};
use crate::model::{CHANNELS, validate_executable, validate_project_id};

const APPLICATION_ASSETS: [(&str, &str); 7] = [
	("style.css", include_str!("../../web/style.css")),
	("search.js", include_str!("../../web/search.js")),
	("exports.js", include_str!("../../web/exports.js")),
	("query.js", include_str!("../../web/query.js")),
	("results.js", include_str!("../../web/results.js")),
	("browser.js", include_str!("../../web/browser.js")),
	("dataset.js", include_str!("../../web/dataset.js")),
];

#[derive(Serialize)]
struct BrowserDataset<'a> {
	schema_version: u32,
	dataset_release: &'a str,
	generated_at: &'a str,
	channels: Vec<BrowserChannel>,
	projects: Vec<BrowserProject>,
}

#[derive(Serialize)]
struct BrowserChannel {
	name: &'static str,
	label: &'static str,
}

#[derive(Serialize)]
struct BrowserProject {
	name: String,
	links: Vec<ProjectLink>,
	providers: Vec<ProjectProvider>,
}

pub fn generate(source: &Path, output: &Path) -> Result<()> {
	ensure!(!output.exists(), "website output already exists: {}", output.display());
	let database = Database::open(source)?;
	let parent = output
		.parent()
		.filter(|path| !path.as_os_str().is_empty())
		.unwrap_or(Path::new("."));
	fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	/* A failed build must not leave a partially published website. */
	let staging = tempfile::Builder::new().prefix(".bnck-site-").tempdir_in(parent)?;
	build(&database, staging.path())?;
	ensure!(!output.exists(), "website output already exists: {}", output.display());
	fs::rename(staging.path(), output).with_context(|| format!("publish website to {}", output.display()))?;
	Ok(())
}

fn build(database: &Database, root: &Path) -> Result<()> {
	let projects = read_names(database, "SELECT DISTINCT project FROM providers ORDER BY project")?;
	let executables = read_names(database, "SELECT DISTINCT executable FROM providers ORDER BY executable")?;
	for project in &projects {
		validate_project_id(project)?;
		validate_file_name(project)?;
	}
	for executable in &executables {
		validate_executable(executable)?;
		validate_file_name(executable)?;
	}
	let dataset = browser_dataset(database, &projects)?;
	let application_assets = read_application_assets()?;
	let assets = write_assets(root, &application_assets, &serde_json::to_vec(&dataset)?)?;
	let site = Site { database, root, assets, environment: template_environment()? };
	site.write_overview()?;
	for project in &projects {
		site.write_project(project)?;
	}
	for executable in &executables {
		site.write_executable(executable)?;
	}
	write_headers(root)
}

fn write_headers(root: &Path) -> Result<()> {
	let mut headers = format!("/*\n  X-Content-Type-Options: nosniff\n  Content-Security-Policy: {HTML_CSP}\n");
	/* Avoid overlapping cache rules: some hosts combine rather than override matching headers. */
	for path in ["/", "/index.html", "/about", "/about/*", "/projects/*", "/executables/*", "/404.html"] {
		headers.push_str(&format!("\n{path}\n  Cache-Control: no-cache\n"));
	}
	headers.push_str("\n/assets/*\n  Cache-Control: public, max-age=31536000, immutable\n");
	write_file(root, "_headers", headers.as_bytes())
}

fn read_names(database: &Database, sql: &str) -> Result<Vec<String>> {
	let connection = database.connection()?;
	let mut statement = connection.prepare(sql)?;
	Ok(statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?)
}

fn validate_file_name(name: &str) -> Result<()> {
	ensure!(
		!matches!(name, "." | "..") && !name.contains(['/', '\\']),
		"identity {name:?} cannot be represented as a static page path"
	);
	Ok(())
}

fn browser_dataset<'a>(database: &'a Database, projects: &[String]) -> Result<BrowserDataset<'a>> {
	let projects = projects
		.iter()
		.map(|name| {
			Ok(BrowserProject {
				name: name.clone(),
				links: database.project_links(name)?,
				providers: database.project_providers(name)?,
			})
		})
		.collect::<Result<_>>()?;
	Ok(BrowserDataset {
		schema_version: 1,
		dataset_release: database.metadata_value("dataset_release"),
		generated_at: database.metadata_value("generated_at"),
		channels: CHANNELS
			.into_iter()
			.map(|name| BrowserChannel { name, label: channel_label(name) })
			.collect(),
		projects,
	})
}

fn read_application_assets() -> Result<Vec<(&'static str, Vec<u8>)>> {
	let mut assets = Vec::new();
	for (name, embedded) in APPLICATION_ASSETS {
		let content = if cfg!(debug_assertions) {
			fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("web").join(name))?
		} else {
			embedded.as_bytes().to_vec()
		};
		assets.push((name, content));
	}
	Ok(assets)
}

fn write_assets(root: &Path, application_assets: &[(&str, Vec<u8>)], dataset: &[u8]) -> Result<SiteAssets> {
	/* Keep relative module imports together without invalidating application assets for dataset updates. */
	let mut digest = Sha256::new();
	for (name, content) in application_assets {
		digest.update(name.as_bytes());
		digest.update((content.len() as u64).to_le_bytes());
		digest.update(content);
	}
	let directory = format!("assets/app.{:x}", digest.finalize());
	for (name, content) in application_assets {
		write_file(root, &format!("{directory}/{name}"), content)?;
	}
	let dataset_path = format!("assets/dataset.{:x}.json", Sha256::digest(dataset));
	write_file(root, &dataset_path, dataset)?;
	Ok(SiteAssets { application: format!("/{directory}"), dataset: format!("/{dataset_path}") })
}

fn write_file(root: &Path, relative: &str, content: &[u8]) -> Result<()> {
	let path = root.join(relative);
	fs::create_dir_all(path.parent().context("website file has no parent")?)?;
	/* create_new also detects case collisions on case-insensitive build filesystems. */
	let mut file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&path)
		.with_context(|| format!("create website file {}", path.display()))?;
	file.write_all(content).with_context(|| format!("write {}", path.display()))
}

struct Site<'a> {
	database: &'a Database,
	root: &'a Path,
	assets: SiteAssets,
	environment: Environment<'static>,
}

impl Site<'_> {
	fn render(&self, path: &str, template: &str, title: &str, page: Value) -> Result<()> {
		let html =
			render_page_with_assets(&self.environment, self.database, template, title, page, Some(&self.assets))?;
		write_file(self.root, path, html.as_bytes())
	}

	fn write_overview(&self) -> Result<()> {
		let database = self.database;
		self.render(
			"index.html",
			"home.html",
			"",
			context! {
				stats => database.stats, channels => &database.channels, example => &database.example,
			},
		)?;
		self.render(
			"about/index.html",
			"dataset.html",
			"Dataset",
			context! {
				metadata => &database.metadata, stats => database.stats, channels => &database.channels,
			},
		)?;
		self.render(
			"404.html",
			"error.html",
			"Page not found",
			context! {
				status => 404, title => "Page not found", message => "Check the address or use the search below.",
			},
		)
	}

	fn write_project(&self, name: &str) -> Result<()> {
		self.render(
			&format!("projects/{name}/index.html"),
			"project.html",
			name,
			context! {
				project => name,
				links => self.database.project_links(name)?,
				matrix => build_project_matrix(self.database.project_providers(name)?),
			},
		)?;
		write_file(
			self.root,
			&format!("projects/{name}/providers.json"),
			&export_bytes(self.database, ExportQuery::Project(name.to_owned()))?,
		)
	}

	fn write_executable(&self, name: &str) -> Result<()> {
		let projects = group_executable_projects(self.database.executable_providers(name)?);
		self.render(
			&format!("executables/{name}/index.html"),
			"executable.html",
			name,
			context! {
				executable => name, project_count => projects.len(), projects,
			},
		)?;
		write_file(
			self.root,
			&format!("executables/{name}/providers.json"),
			&export_bytes(self.database, ExportQuery::Executable(name.to_owned()))?,
		)
	}
}
