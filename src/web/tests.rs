use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use axum::body::to_bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::Response;
use tempfile::tempdir;

use super::{
	Database, SEARCH_PAGE_SIZE, SearchKind, SearchQuery, about, executable_page, home, project_page, render_page,
	search_page,
};
use crate::model::{Provider, write_json_lines};
use crate::release::{SqliteMetadata, generate_sqlite_artifact};

const RECORDS: &[(&str, &str, &str, &str)] = &[
	("helix", "hx", "crates.io", "helix-term"),
	("helix", "hx", "debian", "helix"),
	("other", "hx", "npm", "@scope/other"),
	("ripgrep", "rg", "crates.io", "ripgrep"),
	("hx", "hx-admin", "debian", "hx-utils"),
	("case", "HX", "debian", "upper"),
	("literal", "tool%_", "npm", "@scope/literal"),
	("unicode", "étool", "pypi", "unicode"),
	("escaping", "<tool>&'", "npm", "@scope/escaping"),
];

pub(super) fn search_query(query: &str) -> SearchQuery {
	SearchQuery { q: Some(query.to_owned()), kind: SearchKind::All, channel: String::new(), page: 1 }
}

fn provider(record: (&str, &str, &str, &str)) -> Provider {
	Provider {
		project: record.0.to_owned(),
		executable: record.1.to_owned(),
		channel: record.2.to_owned(),
		package: record.3.to_owned(),
	}
}

fn fixture(records: impl IntoIterator<Item = Provider>) -> Database {
	let root = tempdir().unwrap();
	let projects = root.path().join("curations/projects");
	fs::create_dir_all(&projects).unwrap();
	let records: BTreeSet<_> = records.into_iter().collect();
	for record in &records {
		fs::write(
			projects.join(format!("{}.toml", record.project)),
			format!("id = '{}'\nname = '{}'\n", record.project, record.project),
		)
		.unwrap();
	}
	let input = root.path().join("providers.jsonl");
	write_json_lines(&input, records).unwrap();
	let artifact = root.path().join("bnck.sqlite.zst");
	generate_sqlite_artifact(
		&input,
		&root.path().join("curations"),
		&artifact,
		&SqliteMetadata {
			dataset_release: "2026-04-16.1",
			generated_at: "2026-04-16T12:00:00Z",
			generator_version: "0.1.0",
			generator_revision: "0123456789abcdef",
		},
	)
	.unwrap();
	Database::open(&artifact).unwrap()
}

fn sample_database() -> Database {
	fixture(RECORDS.iter().copied().map(provider))
}

async fn response_text(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), 1_000_000).await.unwrap().to_vec()).unwrap()
}

#[test]
fn search_preserves_identities_and_ranks_exact_matches_first() {
	let database = sample_database();
	let results = database.search(&search_query("hx")).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| (row.name.as_str(), row.kind.as_str())).collect();
	assert_eq!(names, [("hx", "executable"), ("hx", "project"), ("HX", "executable"), ("hx-admin", "executable")]);
	assert_eq!(results.rows[0].provider_count, 3);
	assert_eq!(results.rows[0].related_count, 2);
	assert_eq!(results.rows[0].channel_count, 3);
	assert_eq!(database.search(&search_query("HX")).unwrap().rows[0].name, "HX");
	assert_eq!(database.executable_providers("HX").unwrap().len(), 1);
	assert_eq!(database.executable_providers("hx").unwrap().len(), 3);
	assert!(database.executable_providers("Hx").unwrap().is_empty());
	assert!(database.project_providers("Helix").unwrap().is_empty());
}

#[test]
fn channel_and_kind_filters_count_only_matching_relations() {
	let database = sample_database();
	let mut search = search_query("hx");
	search.kind = SearchKind::Executable;
	search.channel = "npm".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].provider_count, 1);
	assert_eq!(results.rows[0].related_count, 1);
	assert_eq!(results.rows[0].channel_count, 1);
	search.kind = SearchKind::Project;
	assert_eq!(database.search(&search).unwrap().total, 0);
	search.channel = "debian".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].kind, "project");
	assert_eq!(
		database
			.channels
			.iter()
			.find(|channel| channel.name == "debian")
			.unwrap()
			.packages,
		3
	);
}

#[test]
fn search_treats_wildcards_and_sql_as_literal_text() {
	let database = sample_database();
	for query in ["%", "_", "tool%_"] {
		let results = database.search(&search_query(query)).unwrap();
		assert_eq!(results.total, 1);
		assert_eq!(results.rows[0].name, "tool%_");
	}
	assert_eq!(database.search(&search_query("' OR 1=1 --")).unwrap().total, 0);
	assert_eq!(database.search(&search_query("étool")).unwrap().total, 1);
	assert_eq!(database.search(&search_query("Étool")).unwrap().total, 0);
}

#[test]
fn browse_paginates_every_result_without_duplicates() {
	let database =
		fixture((0..68).map(|index| provider(("tools", &format!("command-{index:03}"), "debian", "toolbox"))));
	let mut search = search_query("");
	search.kind = SearchKind::Executable;
	search.channel = "debian".to_owned();
	let mut names = BTreeSet::new();
	for page in 1..=3 {
		search.page = page;
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 68);
		assert_eq!(results.start, u64::from(page - 1) * SEARCH_PAGE_SIZE + 1);
		assert_eq!(results.previous.is_some(), page > 1);
		assert_eq!(results.next.is_some(), page < 3);
		if let Some(next) = results.next {
			let Query(parsed) = Query::<SearchQuery>::try_from_uri(&next.parse().unwrap()).unwrap();
			assert_eq!(parsed.page, page + 1);
			assert_eq!(parsed.channel, "debian");
			assert!(parsed.kind == SearchKind::Executable);
		}
		for row in results.rows {
			assert!(names.insert(row.name));
		}
	}
	assert_eq!(names.len(), 68);
	search.page = 4;
	assert_eq!(search_page(&database, search).status(), StatusCode::NOT_FOUND);
}

#[test]
fn pagination_encodes_query_parameters_and_handles_empty_data() {
	let query = search_query("a & é?#");
	let uri: Uri = query.page_url(2).parse().unwrap();
	let Query(parsed) = Query::<SearchQuery>::try_from_uri(&uri).unwrap();
	assert_eq!(parsed.query(), query.query());
	assert_eq!(parsed.page, 2);
	let database = fixture([]);
	let results = database.search(&search_query("")).unwrap();
	assert_eq!((results.total, results.start, results.end), (0, 0, 0));
	assert!(results.previous.is_none() && results.next.is_none());
	assert!(database.channels.is_empty());
	assert!(database.example.is_none());
	let html = render_page(
		&database,
		"home.html",
		"Home",
		minijinja::context! { stats => database.stats, channels => database.channels, example => database.example },
	)
	.unwrap();
	assert!(html.contains("No records in this release."));
}

#[tokio::test]
async fn renders_detail_ambiguity_and_escapes_dataset_names() {
	let database = sample_database();
	let response = executable_page(&database, "hx");
	assert_eq!(response.status(), StatusCode::OK);
	assert!(
		response.headers()[header::CONTENT_SECURITY_POLICY]
			.to_str()
			.unwrap()
			.contains("default-src 'none'")
	);
	let html = response_text(response).await;
	assert!(html.contains("class='ambiguity-note'"));
	assert_eq!(html.matches("href='/projects/helix'").count(), 1);
	assert_eq!(html.matches("href='/projects/other'").count(), 1);
	assert!(html.contains("<code>@scope&#x2f;other</code>"));
	assert!(html.contains("%40scope&#x2f;other"));
	let html = response_text(executable_page(&database, "<tool>&'")).await;
	assert!(!html.contains("<tool>"));
	assert!(html.contains("&lt;tool&gt;&amp;&#x27;"));
	let html = response_text(search_page(&database, search_query("<tool>&'"))).await;
	assert!(html.contains("href='/executables/%3Ctool%3E%26%27'"));
	let html = response_text(project_page(&database, "helix")).await;
	assert!(html.contains("No project links listed."));
	assert!(!html.contains("class='ambiguity-note'"));
}

#[tokio::test]
async fn handles_invalid_queries_and_missing_identities() {
	let database = sample_database();
	for uri in ["/?page=0", "/?channel=unknown", &format!("/?q={}", "x".repeat(101))] {
		let Query(search) = Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap()).unwrap();
		assert_eq!(search_page(&database, search).status(), StatusCode::BAD_REQUEST);
	}
	for uri in ["/?kind=package", "/?page=-1", "/?page=abc", "/?page=4294967296"] {
		assert!(Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap()).is_err());
	}
	assert_eq!(executable_page(&database, "Hx").status(), StatusCode::NOT_FOUND);
	assert_eq!(project_page(&database, "unknown").status(), StatusCode::NOT_FOUND);
	let search = search_query("é".repeat(100).as_str());
	assert_eq!(search_page(&database, search).status(), StatusCode::OK);
	let database = Arc::new(database);
	for uri in ["/?kind=package", "/?page=-1", "/?page=abc"] {
		let query = Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap());
		let response = home(State(database.clone()), query).await;
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
		assert!(response_text(response).await.contains("Invalid search"));
	}
	let query = Query::<SearchQuery>::try_from_uri(&"/".parse().unwrap());
	assert_eq!(home(State(database.clone()), query).await.status(), StatusCode::OK);
	let html = response_text(about(State(database)).await).await;
	assert!(html.contains("An unlisted command or package may still be available."));
	assert!(html.contains("2026-04-16.1"));
}
