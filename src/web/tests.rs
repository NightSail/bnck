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

const SHARED_RECORDS: &[(&str, &str, &str, &str)] = &[
	("eslint", "eslint", "debian", "eslint"),
	("eslint", "eslint", "npm", "eslint"),
	("eslint", "eslint", "npm", "eslint-alias"),
	("eslint", "eslint", "homebrew", "homebrew/core/eslint"),
];

const AERC_RECORDS: &[(&str, &str, &str, &str)] = &[
	("aerc", "aerc", "debian", "aerc"),
	("aerc", "carddav-query", "debian", "aerc"),
	("aerc", "carddav-query", "homebrew", "homebrew/core/aerc"),
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
fn search_groups_exact_names_and_ranks_exact_matches_first() {
	let database = sample_database();
	let results = database.search(&search_query("hx")).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| row.name.as_str()).collect();
	assert_eq!(names, ["hx", "HX", "hx-admin"]);
	assert_eq!(results.total, 3);
	let group = &results.rows[0];
	assert_eq!(group.identities.len(), 2);
	assert!(group.synonymous_project.is_none());
	let executable = &group.identities[0];
	assert!(matches!(executable.kind, super::IdentityKind::Executable));
	assert_eq!(executable.provider_count, 3);
	assert_eq!(executable.related_count, 2);
	assert_eq!(executable.channel_count, 3);
	assert_eq!(executable.related_names, ["helix", "other"]);
	let project = &group.identities[1];
	assert!(matches!(project.kind, super::IdentityKind::Project));
	assert_eq!(project.related_names, ["hx-admin"]);
	assert_eq!(project.channel_count, 1);
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
	assert_eq!(results.rows[0].identities.len(), 1);
	let identity = &results.rows[0].identities[0];
	assert_eq!(identity.provider_count, 1);
	assert_eq!(identity.related_count, 1);
	assert_eq!(identity.channel_count, 1);
	assert_eq!(identity.related_names, ["other"]);
	search.kind = SearchKind::Project;
	assert_eq!(database.search(&search).unwrap().total, 0);
	search.channel = "debian".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].identities.len(), 1);
	assert!(matches!(results.rows[0].identities[0].kind, super::IdentityKind::Project));
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
fn synonymous_summaries_keep_counts_filtered_without_double_counting() {
	let database = fixture(SHARED_RECORDS.iter().copied().map(provider));
	let mut search = search_query("eslint");
	for (channel, providers, channels) in [("", 4, 3), ("npm", 2, 1)] {
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!((results.total, results.start, results.end), (1, 1, 1));
		let group = &results.rows[0];
		let summary = group.synonymous_project.as_ref().unwrap();
		assert_eq!(summary.other_executable_count, 0);
		assert!(summary.other_executables.is_empty());
		assert_eq!(group.identities.len(), 2);
		for identity in &group.identities {
			assert_eq!(identity.provider_count, providers);
			assert_eq!(identity.channel_count, channels);
			assert_eq!(identity.related_count, 1);
			assert_eq!(identity.related_names, ["eslint"]);
		}
	}
	search.kind = SearchKind::Project;
	let results = database.search(&search).unwrap();
	assert_eq!(results.rows[0].identities.len(), 1);
	assert!(results.rows[0].synonymous_project.is_none());
}

#[test]
fn channel_filters_do_not_hide_competing_projects_in_synonymous_summaries() {
	let records = SHARED_RECORDS.iter().copied().chain([("other", "eslint", "debian", "other")]);
	let database = fixture(records.map(provider));
	for channel in ["", "npm"] {
		let mut search = search_query("eslint");
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 1);
		let group = &results.rows[0];
		assert_eq!(group.identities.len(), 2);
		assert!(group.synonymous_project.is_none());
	}
}

#[test]
fn other_executables_do_not_prevent_synonymous_summaries_and_respect_channel_filters() {
	let records = SHARED_RECORDS
		.iter()
		.copied()
		.chain([("eslint", "eslint-helper", "debian", "eslint")]);
	let database = fixture(records.map(provider));
	for (channel, count) in [("", 1), ("npm", 0)] {
		let mut search = search_query("eslint");
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		let summary = results.rows[0].synonymous_project.as_ref().unwrap();
		assert_eq!(summary.other_executable_count, count);
		assert_eq!(summary.other_executables.len() as u64, count);
		if count > 0 {
			assert_eq!(summary.other_executables, ["eslint-helper"]);
			assert_eq!(results.rows[1].name, "eslint-helper");
		}
	}
}

#[test]
fn identical_counts_and_spelling_do_not_establish_a_synonymous_relationship() {
	let database = fixture(
		[("collision", "different", "npm", "collision"), ("other", "collision", "npm", "other")]
			.into_iter()
			.map(provider),
	);
	let results = database.search(&search_query("collision")).unwrap();
	assert_eq!(results.total, 1);
	let group = &results.rows[0];
	assert_eq!(group.identities.len(), 2);
	assert!(group.synonymous_project.is_none());
	assert_eq!(group.identities[0].related_names, ["other"]);
	assert_eq!(group.identities[1].related_names, ["different"]);
}

#[test]
fn case_variants_remain_separate_name_groups() {
	let database = fixture(
		[("tool", "tool", "npm", "tool"), ("upper", "Tool", "debian", "upper")]
			.into_iter()
			.map(provider),
	);
	let results = database.search(&search_query("tool")).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "tool");
	assert_eq!(results.rows[1].name, "Tool");
	assert!(results.rows[0].synonymous_project.is_some());
	assert_eq!(results.rows[1].identities.len(), 1);
	assert!(results.rows[1].synonymous_project.is_none());
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
fn all_types_paginates_name_groups_without_splitting_identity_views() {
	let database = fixture((0..68).map(|index| {
		let name = format!("tool-{index:03}");
		provider((&name, &name, "debian", &name))
	}));
	let mut search = search_query("tool");
	search.channel = "debian".to_owned();
	let mut names = BTreeSet::new();
	for page in 1..=3 {
		search.page = page;
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 68);
		assert_eq!(results.rows.len(), if page < 3 { 30 } else { 8 });
		assert_eq!(results.start, u64::from(page - 1) * SEARCH_PAGE_SIZE + 1);
		assert_eq!(results.end, (u64::from(page) * SEARCH_PAGE_SIZE).min(68));
		assert_eq!(results.previous.is_some(), page > 1);
		assert_eq!(results.next.is_some(), page < 3);
		if let Some(next) = results.next {
			let Query(parsed) = Query::<SearchQuery>::try_from_uri(&next.parse().unwrap()).unwrap();
			assert_eq!(parsed.page, page + 1);
			assert_eq!(parsed.query(), "tool");
			assert_eq!(parsed.channel, "debian");
			assert!(parsed.kind == SearchKind::All);
		}
		for row in results.rows {
			assert_eq!(row.identities.len(), 2);
			assert!(row.synonymous_project.is_some());
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
async fn renders_synonymous_summary_once_with_two_explicit_destinations() {
	let database = fixture(SHARED_RECORDS.iter().copied().map(provider));
	let html = response_text(search_page(&database, search_query("eslint"))).await;
	assert_eq!(html.matches("class='name-result'").count(), 1);
	assert_eq!(html.matches("<code>eslint</code>").count(), 1);
	assert_eq!(html.matches("Project with synonymous executable").count(), 1);
	assert!(!html.contains("Executable and project"));
	assert!(!html.contains("Other executables:"));
	assert_eq!(html.matches("3 channels").count(), 1);
	assert_eq!(html.matches("4 provider records").count(), 1);
	assert_eq!(html.matches("href='/executables/eslint'").count(), 1);
	assert_eq!(html.matches("href='/projects/eslint'").count(), 1);
	assert!(html.contains("Executable providers"));
	assert!(html.contains("Project details"));
	assert!(html.contains("1 matching name"));
	assert!(!html.contains("class='results-table'"));
	assert!(!html.contains("class='identity-summary'"));
	for kind in [SearchKind::Executable, SearchKind::Project] {
		let mut search = search_query("eslint");
		search.kind = kind;
		let html = response_text(search_page(&database, search)).await;
		assert!(html.contains("class='results-table'"));
		assert!(!html.contains("class='name-result'"));
		assert!(!html.contains("Project with synonymous executable"));
		assert!(html.contains("class='related-name'"));
	}
}

#[tokio::test]
async fn synonymous_project_lists_other_executables_separately_and_uses_project_counts() {
	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	for (channel, providers, channels) in [("", 3, 2), ("debian", 2, 1)] {
		let mut search = search_query("aerc");
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 1);
		let group = &results.rows[0];
		let summary = group.synonymous_project.as_ref().unwrap();
		assert_eq!(summary.other_executable_count, 1);
		assert_eq!(summary.other_executables, ["carddav-query"]);
		assert_eq!(group.identities[0].provider_count, 1);
		assert_eq!(group.identities[1].provider_count, providers);
		assert_eq!(group.identities[1].channel_count, channels);
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.matches("class='name-result'").count(), 1);
		assert_eq!(html.matches("<code>aerc</code>").count(), 1);
		assert_eq!(html.matches("Project with synonymous executable").count(), 1);
		assert_eq!(html.matches("Other executables:").count(), 1);
		assert_eq!(html.matches("href='/executables/carddav-query'").count(), 1);
		assert_eq!(html.matches("href='/executables/aerc'").count(), 1);
		assert_eq!(html.matches("href='/projects/aerc'").count(), 1);
		assert!(html.contains(&format!("{providers} provider records")));
		assert!(!html.contains("1 provider record"));
		assert!(!html.contains("class='identity-summary'"));
	}
	let results = database.search(&search_query("carddav-query")).unwrap();
	assert_eq!(results.rows[0].name, "carddav-query");
	assert!(results.rows[0].synonymous_project.is_none());
	assert_eq!(results.rows[0].identities[0].related_names, ["aerc"]);
}

#[tokio::test]
async fn channel_with_only_other_executables_shows_a_project_summary() {
	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	let mut search = search_query("aerc");
	search.channel = "homebrew".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].identities.len(), 1);
	assert!(results.rows[0].synonymous_project.is_none());
	let html = response_text(search_page(&database, search)).await;
	assert!(!html.contains("Project with synonymous executable"));
	assert!(html.contains("<h4>Project</h4>"));
	assert!(html.contains("href='/executables/carddav-query'"));
	assert!(!html.contains("href='/executables/aerc'"));
}

#[tokio::test]
async fn other_executable_links_escape_names_and_encode_paths() {
	let records = SHARED_RECORDS
		.iter()
		.copied()
		.chain([("eslint", "<tool>&'", "debian", "eslint")]);
	let database = fixture(records.map(provider));
	let html = response_text(search_page(&database, search_query("eslint"))).await;
	assert!(html.contains("Project with synonymous executable"));
	assert!(html.contains("Other executables:"));
	assert!(html.contains("href='/executables/%3Ctool%3E%26%27'"));
	assert!(html.contains("&lt;tool&gt;&amp;&#x27;"));
	assert!(!html.contains("<tool>"));
}

#[tokio::test]
async fn other_executable_link_limit_excludes_the_synonymous_name() {
	for count in [3, 4] {
		let other_executables =
			(0..count).map(|index| provider(("tools", &format!("command-{index}"), "npm", "tools")));
		let database = fixture(
			[provider(("tools", "tools", "npm", "tools"))]
				.into_iter()
				.chain(other_executables),
		);
		let search = search_query("tools");
		let results = database.search(&search).unwrap();
		let summary = results.rows[0].synonymous_project.as_ref().unwrap();
		assert_eq!(summary.other_executable_count, count);
		let html = response_text(search_page(&database, search)).await;
		assert!(html.contains("Project with synonymous executable"));
		assert!(html.contains("Other executables:"));
		assert_eq!(html.matches("<code>tools</code>").count(), 1);
		if count == 3 {
			assert_eq!(summary.other_executables, ["command-0", "command-1", "command-2"]);
			assert_eq!(html.matches("class='related-name'").count(), 3);
		} else {
			assert!(summary.other_executables.is_empty());
			assert!(!html.contains("class='related-name'"));
			assert!(
				html.split_whitespace()
					.collect::<Vec<_>>()
					.join(" ")
					.contains("Other executables: 4")
			);
		}
	}
}

#[tokio::test]
async fn renders_distinct_identity_summaries_and_filtered_related_links() {
	let database = sample_database();
	let html = response_text(search_page(&database, search_query("hx"))).await;
	assert_eq!(html.matches("<h3 id='result-1'><code>hx</code></h3>").count(), 1);
	assert!(html.contains("<h4>Executable</h4>"));
	assert!(html.contains("<h4>Project</h4>"));
	assert!(!html.contains("Project with synonymous executable"));
	assert!(html.contains("href='/projects/helix'"));
	assert!(html.contains("href='/projects/other'"));
	assert!(html.contains("href='/executables/hx-admin'"));
	let mut search = search_query("hx");
	search.channel = "npm".to_owned();
	let html = response_text(search_page(&database, search)).await;
	assert_eq!(html.matches("class='identity-summary'").count(), 1);
	assert!(html.contains("href='/projects/other'"));
	assert!(!html.contains("href='/projects/helix'"));
	assert!(!html.contains("href='/projects/hx'"));
	let html = response_text(search_page(&database, search_query("escaping"))).await;
	assert!(html.contains("href='/executables/%3Ctool%3E%26%27'"));
	assert!(html.contains("&lt;tool&gt;&amp;&#x27;"));
	assert!(!html.contains("<tool>"));
}

#[tokio::test]
async fn relationship_summaries_link_small_sets_and_count_large_sets() {
	for count in [3, 4] {
		let database =
			fixture((0..count).map(|index| provider(("tools", &format!("command-{index}"), "npm", "tools"))));
		let search = search_query("tools");
		let results = database.search(&search).unwrap();
		let identity = &results.rows[0].identities[0];
		assert_eq!(identity.related_count, count);
		let html = response_text(search_page(&database, search)).await;
		if count == 3 {
			assert_eq!(identity.related_names, ["command-0", "command-1", "command-2"]);
			assert_eq!(html.matches("class='related-name'").count(), 3);
		} else {
			assert!(identity.related_names.is_empty());
			assert!(!html.contains("class='related-name'"));
			assert!(html.contains("4 executables"));
		}
	}
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
