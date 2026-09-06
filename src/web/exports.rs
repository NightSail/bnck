use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::{Database, asset_cache_control};
use crate::model::Provider;

#[derive(Serialize)]
struct ProviderExport {
	dataset_release: String,
	query: ExportQuery,
	providers: Vec<Provider>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ExportQuery {
	Project(String),
	Executable(String),
}

impl ExportQuery {
	fn read_providers(&self, database: &Database) -> Result<Vec<Provider>> {
		match self {
			Self::Project(project) => {
				let providers = database.project_providers(project)?;
				Ok(providers
					.into_iter()
					.map(|provider| Provider {
						project: project.clone(),
						executable: provider.executable,
						channel: provider.channel,
						package: provider.package,
					})
					.collect())
			},
			Self::Executable(executable) => {
				let providers = database.executable_providers(executable)?;
				Ok(providers
					.into_iter()
					.map(|provider| Provider {
						project: provider.project,
						executable: executable.clone(),
						channel: provider.channel,
						package: provider.package,
					})
					.collect())
			},
		}
	}
}

pub(super) async fn project(State(database): State<Arc<Database>>, Path(project): Path<String>) -> Response {
	run_export(database, ExportQuery::Project(project)).await
}

pub(super) async fn executable(State(database): State<Arc<Database>>, Path(executable): Path<String>) -> Response {
	run_export(database, ExportQuery::Executable(executable)).await
}

pub(super) async fn script() -> Response {
	match script_content() {
		Ok(script) => (
			[
				(header::CONTENT_TYPE, HeaderValue::from_static("text/javascript; charset=utf-8")),
				(header::CACHE_CONTROL, HeaderValue::from_static(asset_cache_control())),
				(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
			],
			script,
		)
			.into_response(),
		Err(error) => {
			eprintln!("website export script error: {error:#}");
			(StatusCode::INTERNAL_SERVER_ERROR, "export script unavailable").into_response()
		},
	}
}

#[cfg(debug_assertions)]
fn script_content() -> Result<String> {
	let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/exports.js");
	std::fs::read_to_string(path).context("read JSON export script")
}

#[cfg(not(debug_assertions))]
fn script_content() -> Result<String> {
	Ok(include_str!("../../web/exports.js").to_owned())
}

async fn run_export(database: Arc<Database>, query: ExportQuery) -> Response {
	/* Keep both SQLite work and potentially large JSON serialization off the async executor. */
	let result = tokio::task::spawn_blocking(move || export_response(&database, query)).await;
	let mut response = match result {
		Ok(Ok(response)) => response,
		Ok(Err(error)) => export_error(error),
		Err(error) => export_error(error.into()),
	};
	response
		.headers_mut()
		.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
	response.headers_mut().insert(
		header::CONTENT_SECURITY_POLICY,
		HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
	);
	response
}

fn export_response(database: &Database, query: ExportQuery) -> Result<Response> {
	let providers = query.read_providers(database)?;
	if providers.is_empty() {
		return Ok(json_error(
			StatusCode::NOT_FOUND,
			"No accepted mappings for this exact name are listed in this release. Check spelling and case.",
		));
	}
	let export = ProviderExport {
		dataset_release: format!("bnck:release:{}", database.metadata_value("dataset_release")),
		query,
		providers,
	};
	let mut body = Vec::new();
	let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
	let mut serializer = serde_json::Serializer::with_formatter(&mut body, formatter);
	export.serialize(&mut serializer).context("serialize provider export")?;
	body.push(b'\n');
	Ok(([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body).into_response())
}

fn export_error(error: anyhow::Error) -> Response {
	eprintln!("website export error: {error:#}");
	json_error(StatusCode::INTERNAL_SERVER_ERROR, "The export could not be generated. Try again later.")
}

fn json_error(status: StatusCode, message: &str) -> Response {
	(status, Json(serde_json::json!({ "error": message }))).into_response()
}
