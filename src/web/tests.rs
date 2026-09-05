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

const AGE_RECORDS: &[(&str, &str, &str, &str)] = &[
	("age", "age", "debian", "age"),
	("age", "age-inspect", "debian", "age"),
	("age", "age-keygen", "debian", "age"),
	("age", "age-plugin-batchpass", "debian", "age"),
	("age-plugin-yubikey", "age-plugin-yubikey", "debian", "age-plugin-yubikey"),
];

const RANKING_RECORDS: &[(&str, &str, &str, &str)] = &[
	("z-exact", "tool", "npm", "z-exact"),
	("tool-prefix", "unrelated", "npm", "tool-prefix"),
	("short-prefix", "tool-x", "npm", "short-prefix"),
	("a-substring", "xtool", "npm", "a-substring"),
	("a-substring", "tool-long-prefix", "npm", "a-substring"),
	("b-substring", "xtool", "npm", "b-substring"),
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

fn executable_names(result: &super::SearchResult) -> Vec<&str> {
	result
		.project_executables
		.iter()
		.map(|executable| executable.name.as_str())
		.collect()
}

fn assert_executable_row(html: &str, project: &str, executable: &str, channels: &str) {
	let href = format!(
		"href='/projects/{}#executable-{}'",
		super::encode_path_segment(project),
		super::encode_path_segment(executable),
	);
	assert_eq!(html.matches(&href).count(), 1);
	let row = html.split_once(&href).unwrap().1.split_once("</li>").unwrap().0;
	let row = row.split_whitespace().collect::<Vec<_>>().join(" ");
	let channel_text = row.split_once("(1)</code></a>: ").unwrap().1;
	let channel_text = channel_text.split_once("<span").map_or(channel_text, |(text, _)| text);
	assert_eq!(channel_text.trim(), channels);
}

fn sample_database() -> Database {
	fixture(RECORDS.iter().copied().map(provider))
}

async fn response_text(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), 1_000_000).await.unwrap().to_vec()).unwrap()
}

#[test]
fn search_groups_by_owner_and_ranks_each_projects_best_match() {
	let database = sample_database();
	let results = database.search(&search_query("hx")).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| row.name.as_str()).collect();
	assert_eq!(names, ["helix", "hx", "other", "case"]);
	assert_eq!(results.total, 4);
	let group = &results.rows[0];
	assert!(matches!(group.identity.kind, super::IdentityKind::Project));
	assert_eq!(group.identity.provider_count, 2);
	assert_eq!(group.identity.related_count, 1);
	assert_eq!(group.identity.channel_count, 2);
	assert_eq!(executable_names(group), ["hx"]);
	assert_eq!(executable_names(&results.rows[1]), ["hx-admin"]);
	assert_eq!(executable_names(&results.rows[2]), ["hx"]);
	assert_eq!(database.search(&search_query("HX")).unwrap().rows[0].name, "case");
	assert_eq!(database.executable_providers("HX").unwrap().len(), 1);
	assert_eq!(database.executable_providers("hx").unwrap().len(), 3);
	assert!(database.executable_providers("Hx").unwrap().is_empty());
	assert!(database.project_providers("Helix").unwrap().is_empty());
}

#[test]
fn executable_only_search_retains_flat_names_ranking_and_all_owners() {
	let database = sample_database();
	let mut search = search_query("hx");
	search.kind = SearchKind::Executable;
	let results = database.search(&search).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| row.name.as_str()).collect();
	assert_eq!(names, ["hx", "HX", "hx-admin"]);
	assert_eq!(results.total, 3);
	let identity = &results.rows[0].identity;
	assert!(matches!(identity.kind, super::IdentityKind::Executable));
	assert_eq!(identity.provider_count, 3);
	assert_eq!(identity.related_count, 2);
	assert_eq!(identity.channel_count, 3);
	assert_eq!(identity.related_names, ["helix", "other"]);
}

#[test]
fn channel_and_kind_filters_count_only_matching_relations() {
	let database = sample_database();
	let mut search = search_query("hx");
	search.kind = SearchKind::Executable;
	search.channel = "npm".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert!(results.rows[0].project_executables.is_empty());
	let identity = &results.rows[0].identity;
	assert_eq!(identity.provider_count, 1);
	assert_eq!(identity.related_count, 1);
	assert_eq!(identity.channel_count, 1);
	assert_eq!(identity.related_names, ["other"]);
	search.kind = SearchKind::Project;
	assert_eq!(database.search(&search).unwrap().total, 0);
	search.channel = "debian".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert!(matches!(results.rows[0].identity.kind, super::IdentityKind::Project));
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
fn project_summaries_keep_counts_filtered_without_double_counting() {
	let database = fixture(SHARED_RECORDS.iter().copied().map(provider));
	let mut search = search_query("eslint");
	for (channel, providers, channels) in [("", 4, 3), ("npm", 2, 1)] {
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!((results.total, results.start, results.end), (1, 1, 1));
		let group = &results.rows[0];
		assert_eq!(group.identity.provider_count, providers);
		assert_eq!(group.identity.channel_count, channels);
		assert_eq!(group.identity.related_count, 1);
		assert_eq!(executable_names(group), ["eslint"]);
	}
}

#[test]
fn channel_filters_apply_to_project_matches_counts_and_executables() {
	let records = SHARED_RECORDS
		.iter()
		.copied()
		.chain([("other", "eslint", "debian", "other"), ("eslint", "eslint-helper", "debian", "eslint")]);
	let database = fixture(records.map(provider));
	let mut search = search_query("eslint");
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(executable_names(&results.rows[0]), ["eslint", "eslint-helper"]);
	assert_eq!(results.rows[1].name, "other");
	search.channel = "npm".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(executable_names(&results.rows[0]), ["eslint"]);
	assert_eq!(results.rows[0].identity.provider_count, 2);
	search.q = Some("eslint-helper".to_owned());
	assert_eq!(database.search(&search).unwrap().total, 0);
	assert_eq!(database.executable_providers("eslint").unwrap().len(), 5);
}

#[test]
fn companion_executables_group_by_ownership_not_prefix() {
	let database = fixture(AGE_RECORDS.iter().copied().map(provider));
	for query in ["", "age"] {
		let results = database.search(&search_query(query)).unwrap();
		assert_eq!(results.total, 2);
		assert_eq!(results.rows[0].name, "age");
		assert_eq!(results.rows[0].project_executables.len(), 4);
		assert_eq!(results.rows[1].name, "age-plugin-yubikey");
		assert_eq!(executable_names(&results.rows[1]), ["age-plugin-yubikey"]);
	}
	let results = database.search(&search_query("age-keygen")).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].name, "age");
	assert_eq!(results.rows[0].project_executables[0].name, "age-keygen");

	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	let results = database.search(&search_query("")).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(executable_names(&results.rows[0]), ["aerc", "carddav-query"]);
}

#[test]
fn matching_project_and_executable_names_do_not_establish_ownership() {
	let database = fixture(
		[("collision", "different", "npm", "collision"), ("other", "collision", "npm", "other")]
			.into_iter()
			.map(provider),
	);
	let results = database.search(&search_query("collision")).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "collision");
	assert_eq!(executable_names(&results.rows[0]), ["different"]);
	assert_eq!(results.rows[1].name, "other");
	assert_eq!(executable_names(&results.rows[1]), ["collision"]);
}

#[test]
fn case_variants_preserve_exact_executable_identities() {
	let database = fixture(
		[("tool", "tool", "npm", "tool"), ("upper", "Tool", "debian", "upper")]
			.into_iter()
			.map(provider),
	);
	let mut search = search_query("tool");
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "tool");
	assert_eq!(results.rows[1].name, "upper");
	assert_eq!(executable_names(&results.rows[0]), ["tool"]);
	assert_eq!(executable_names(&results.rows[1]), ["Tool"]);
	search.kind = SearchKind::Executable;
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "tool");
	assert_eq!(results.rows[1].name, "Tool");
}

#[test]
fn project_ranking_uses_length_only_within_the_best_match_class() {
	let database = fixture(RANKING_RECORDS.iter().copied().map(provider));
	let results = database.search(&search_query("tool")).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| row.name.as_str()).collect();
	assert_eq!(names, ["z-exact", "short-prefix", "tool-prefix", "a-substring", "b-substring"]);
	assert_eq!(executable_names(&results.rows[3]), ["tool-long-prefix", "xtool"]);
}

#[test]
fn search_treats_wildcards_and_sql_as_literal_text() {
	let database = sample_database();
	for query in ["%", "_", "tool%_"] {
		let results = database.search(&search_query(query)).unwrap();
		assert_eq!(results.total, 1);
		assert_eq!(results.rows[0].name, "literal");
		assert_eq!(executable_names(&results.rows[0]), ["tool%_"]);
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
fn all_types_paginates_projects_without_splitting_executable_lists() {
	let database = fixture((0..68).flat_map(|index| {
		let name = format!("tool-{index:03}");
		let executable_count = if index == 0 { 40 } else { 2 };
		(0..executable_count)
			.map(move |command| provider((&name, &format!("command-{index:03}-{command:02}"), "debian", &name)))
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
			assert_eq!(row.project_executables.len(), if row.name == "tool-000" { 40 } else { 2 });
			assert_eq!(row.identity.related_count, row.project_executables.len() as u64);
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
	assert!(html.contains("href='/projects/escaping#executable-%3Ctool%3E%26%27'"));
	let html = response_text(project_page(&database, "helix")).await;
	assert!(html.contains("No project links listed."));
	assert!(!html.contains("class='ambiguity-note'"));
}

#[tokio::test]
async fn renders_one_project_summary_with_explicit_executable_links() {
	let database = fixture(SHARED_RECORDS.iter().copied().map(provider));
	let html = response_text(search_page(&database, search_query("eslint"))).await;
	assert_eq!(html.matches("class='name-result'").count(), 1);
	assert_eq!(html.matches("<code>eslint</code>").count(), 1);
	assert_eq!(html.matches("<code>eslint(1)</code>").count(), 1);
	assert!(!html.contains("Project:"));
	assert!(!html.contains("class='result-summary'"));
	assert!(!html.contains("3 channels"));
	assert!(!html.contains("4 provider records"));
	assert_executable_row(&html, "eslint", "eslint", "Debian, Homebrew, npm");
	assert_eq!(html.matches("href='/projects/eslint'").count(), 1);
	assert!(html.contains(
		"<h3 id='result-1'>\n\t\t\t\t\t\t\t<a href='/projects/eslint'><code>eslint</code></a>\n\t\t\t\t\t\t</h3>"
	));
	assert!(!html.contains("Project details"));
	assert!(html.contains("1 matching project"));
	assert!(!html.contains("class='results-table'"));
	for kind in [SearchKind::Executable, SearchKind::Project] {
		let mut search = search_query("eslint");
		search.kind = kind;
		let html = response_text(search_page(&database, search)).await;
		assert!(html.contains("class='results-table'"));
		assert!(!html.contains("class='name-result'"));
		assert!(html.contains("class='related-name'"));
	}
}

#[tokio::test]
async fn project_groups_include_nonprefixed_executables_and_use_project_counts() {
	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	for (channel, providers, channels) in [("", 3, 2), ("debian", 2, 1)] {
		let mut search = search_query("aerc");
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 1);
		let group = &results.rows[0];
		assert_eq!(executable_names(group), ["aerc", "carddav-query"]);
		assert_eq!(group.identity.provider_count, providers);
		assert_eq!(group.identity.channel_count, channels);
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.matches("class='name-result'").count(), 1);
		assert_eq!(html.matches("<code>aerc</code>").count(), 1);
		assert_executable_row(&html, "aerc", "aerc", "Debian");
		assert_executable_row(
			&html,
			"aerc",
			"carddav-query",
			if channel.is_empty() { "Debian, Homebrew" } else { "Debian" },
		);
		assert_eq!(html.matches("href='/projects/aerc'").count(), 1);
		assert!(!html.contains("provider records"));
	}
	let results = database.search(&search_query("carddav-query")).unwrap();
	assert_eq!(results.rows[0].name, "aerc");
	assert_eq!(executable_names(&results.rows[0]), ["carddav-query", "aerc"]);
}

#[tokio::test]
async fn channel_with_only_other_executables_shows_a_project_summary() {
	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	let mut search = search_query("aerc");
	search.channel = "homebrew".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(executable_names(&results.rows[0]), ["carddav-query"]);
	let html = response_text(search_page(&database, search)).await;
	assert!(html.contains("<h3 id='result-1'>"));
	assert!(!html.contains("Project:"));
	assert_executable_row(&html, "aerc", "carddav-query", "Homebrew");
	assert!(!html.contains("href='/projects/aerc#executable-aerc'"));
}

#[tokio::test]
async fn grouped_executable_links_escape_names_and_encode_paths() {
	let records = SHARED_RECORDS
		.iter()
		.copied()
		.chain([("eslint", "<tool>&'", "debian", "eslint")]);
	let database = fixture(records.map(provider));
	let html = response_text(search_page(&database, search_query("eslint"))).await;
	assert_executable_row(&html, "eslint", "<tool>&'", "Debian");
	assert!(html.contains("&lt;tool&gt;&amp;&#x27;(1)"));
	assert!(!html.contains("<tool>"));
	let html = response_text(project_page(&database, "eslint")).await;
	assert!(html.contains("id='executable-&lt;tool&gt;&amp;&#x27;'"));
}

#[tokio::test]
async fn executable_rows_link_to_distinct_project_sections() {
	let names = ["tool", "Tool", "tool%_", "tool#part", "étool"];
	let database = fixture(names.map(|name| provider(("tools", name, "debian", "tools"))));
	let search_html = response_text(search_page(&database, search_query("tools"))).await;
	let project_html = response_text(project_page(&database, "tools")).await;
	for name in names {
		assert_executable_row(&search_html, "tools", name, "Debian");
		assert_eq!(project_html.matches(&format!("id='executable-{name}'")).count(), 1);
	}
}

#[tokio::test]
async fn grouped_lists_expand_without_javascript_and_keep_exact_matches_visible() {
	for count in [3, 4] {
		let database =
			fixture((0..count).map(|index| provider(("tools", &format!("command-{index}"), "npm", "tools"))));
		let html = response_text(search_page(&database, search_query(""))).await;
		assert_eq!(html.matches("class='name-result'").count(), 1);
		assert_eq!(html.matches("class='related-name'").count(), count);
		for index in 0..count {
			assert_executable_row(&html, "tools", &format!("command-{index}"), "npm");
		}
		assert_eq!(html.contains("<details class='more-executables'>"), count > 3);
		if count > 3 {
			let (preview, expanded) = html.split_once("<details class='more-executables'>").unwrap();
			assert_eq!(preview.matches("class='related-name'").count(), 3);
			assert!(expanded.contains("1 more executable"));
			assert_executable_row(expanded, "tools", "command-3", "npm");
			let html = response_text(search_page(&database, search_query("command-3"))).await;
			let preview = html.split_once("<details class='more-executables'>").unwrap().0;
			assert_executable_row(preview, "tools", "command-3", "npm");
			assert!(preview.contains("Exact match"));
		}
	}
}

#[tokio::test]
async fn renders_shared_commands_under_every_matching_owner() {
	let database = sample_database();
	let html = response_text(search_page(&database, search_query("hx"))).await;
	assert_eq!(html.matches("class='name-result'").count(), 4);
	assert!(html.contains("4 matching projects"));
	assert!(html.contains("href='/projects/helix'"));
	assert!(html.contains("href='/projects/other'"));
	assert_executable_row(&html, "helix", "hx", "crates.io, Debian");
	assert_executable_row(&html, "other", "hx", "npm");
	assert_executable_row(&html, "hx", "hx-admin", "Debian");
	let mut search = search_query("hx");
	search.channel = "npm".to_owned();
	let html = response_text(search_page(&database, search)).await;
	assert_eq!(html.matches("class='name-result'").count(), 1);
	assert!(html.contains("href='/projects/other'"));
	assert!(!html.contains("href='/projects/helix'"));
	assert!(!html.contains("href='/projects/hx'"));
	assert_executable_row(&html, "other", "hx", "npm");
}

#[tokio::test]
async fn type_specific_relationship_summaries_link_small_sets_and_count_large_sets() {
	for count in [3, 4] {
		let database =
			fixture((0..count).map(|index| provider(("tools", &format!("command-{index}"), "npm", "tools"))));
		let mut search = search_query("tools");
		search.kind = SearchKind::Project;
		let results = database.search(&search).unwrap();
		let identity = &results.rows[0].identity;
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
