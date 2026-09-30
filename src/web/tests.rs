use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use axum::body::to_bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::Response;
use tempfile::tempdir;

use super::{Database, SEARCH_PAGE_SIZE, SearchQuery, about, executable_page, home, project_page, render_page, search_page};
use crate::model::{Provider, write_json_lines};
use crate::release::{SqliteMetadata, generate_sqlite_artifact};

const EXACT_MATCH_TAG: &str = "<span class='exact-match'>Exact match</span>";

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

const MATRIX_RECORDS: &[(&str, &str, &str, &str)] = &[
	("tools", "tool", "debian", "tools"),
	("tools", "helper", "debian", "tools"),
	("tools", "tool", "debian", "tools-extra"),
	("tools", "helper", "arch", "tools"),
	("tools", "Tool", "arch", "tools"),
	("tools", "tool", "npm", "@scope/tools"),
	("tools", "helper", "homebrew", "custom/tap/tools"),
];

const MATRIX_EXECUTABLES: &[&str] = &["Tool", "helper", "tool"];

/* Observed executables per package, in the flattened channel/package order. */
const MATRIX_PACKAGE_COVERAGE: &[[bool; 3]] =
	&[[true, true, false], [false, true, true], [false, false, true], [false, true, false], [false, false, true]];

const INITIAL_RECORDS: &[(&str, &str, &str, &str)] = &[
	("alpha", "lookup", "debian", "alpha"),
	("alpha", "lookup", "npm", "alpha"),
	("alpha", "helper", "debian", "alpha"),
	("alpine", "lookup", "debian", "alpine"),
	("beta", "lookup", "npm", "beta"),
	("zeta", "other", "debian", "zeta"),
	("0tool", "lookup", "npm", "numbers"),
	("9tool", "lookup", "debian", "numbers"),
	("-tool", "lookup", "npm", "symbols"),
	(".tool", "lookup", "npm", "symbols"),
	("_tool", "lookup", "npm", "symbols"),
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
	SearchQuery { q: Some(query.to_owned()), channel: String::new(), initial: String::new(), page: 1 }
}

fn provider(record: (&str, &str, &str, &str)) -> Provider {
	Provider {
		project: record.0.to_owned(),
		executable: record.1.to_owned(),
		channel: record.2.to_owned(),
		package: record.3.to_owned(),
	}
}

pub(super) fn fixture(records: impl IntoIterator<Item = Provider>) -> Database {
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
	result.related.iter().map(|executable| executable.name.as_str()).collect()
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
	let channel_text = row.split_once("(1)</code></a>").unwrap().1.trim();
	let channel_text = channel_text.strip_suffix(EXACT_MATCH_TAG).unwrap_or(channel_text);
	let expected = if channels.is_empty() { String::new() } else { format!(": {channels}") };
	assert_eq!(channel_text.trim(), expected);
}

fn sample_database() -> Database {
	fixture(RECORDS.iter().copied().map(provider))
}

fn search_link_query(html: &str, label: &str) -> SearchQuery {
	let html = html.split_whitespace().collect::<Vec<_>>().join(" ");
	let link = html.split("</a>").find(|link| link.ends_with(label)).unwrap();
	let href = link.rsplit_once("href='").unwrap().1.split_once('\'').unwrap().0;
	let uri = href.replace("&#x2f;", "/").replace("&amp;", "&").parse().unwrap();
	let Query(search) = Query::<SearchQuery>::try_from_uri(&uri).unwrap();
	search
}

async fn response_text(response: Response) -> String {
	String::from_utf8(to_bytes(response.into_body(), 1_000_000).await.unwrap().to_vec()).unwrap()
}

async fn exported_providers(database: &Database, response: Response, query: serde_json::Value) -> Vec<Provider> {
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
	assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
	assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
	assert_eq!(response.headers()[header::CONTENT_SECURITY_POLICY], "default-src 'none'; frame-ancestors 'none'");
	let body = response_text(response).await;
	assert!(body.starts_with("{\n\t\"dataset_release\": "));
	assert!(body.ends_with('\n'));
	let export: serde_json::Value = serde_json::from_str(&body).unwrap();
	assert_eq!(export.as_object().unwrap().len(), 3);
	assert_eq!(export["dataset_release"], format!("bnck:release:{}", database.metadata_value("dataset_release")));
	assert_eq!(export["query"], query);
	let providers: Vec<Provider> = serde_json::from_value(export["providers"].clone()).unwrap();
	assert!(!providers.is_empty());
	assert!(providers.windows(2).all(|pair| pair[0] < pair[1]));
	providers
}

#[test]
fn search_groups_by_owner_and_ranks_each_projects_best_match() {
	let database = sample_database();
	let results = database.search(&search_query("hx")).unwrap();
	let names: Vec<_> = results.rows.iter().map(|row| row.name.as_str()).collect();
	assert_eq!(names, ["helix", "hx", "other", "case"]);
	assert_eq!(results.total, 4);
	let group = &results.rows[0];
	assert_eq!(group.related[0].channels, ["crates.io", "debian"]);
	assert_eq!(executable_names(group), ["hx"]);
	assert_eq!(executable_names(&results.rows[1]), ["hx-admin"]);
	assert_eq!(executable_names(&results.rows[2]), ["hx"]);
	assert_eq!(database.search(&search_query("HX")).unwrap().rows[0].name, "case");
	assert_eq!(database.executable_providers("HX").unwrap().len(), 1);
	assert_eq!(database.executable_providers("hx").unwrap().len(), 3);
	assert!(database.executable_providers("Hx").unwrap().is_empty());
	assert!(database.project_providers("Helix").unwrap().is_empty());
}

#[tokio::test]
async fn exact_match_tags_require_literal_case_sensitive_name_equality() {
	let database = fixture(RECORDS.iter().chain(SHARED_RECORDS).copied().map(provider));
	for (query, expected) in [
		("hx", 3),
		(" hx ", 3),
		("HX", 1),
		("Hx", 0),
		("eslint", 2),
		("helix", 1),
		("h", 0),
		("tool%_", 1),
		("%", 0),
		("étool", 1),
		("Étool", 0),
		("' OR 1=1 --", 0),
		("missing", 0),
		("", 0),
		("  ", 0),
	] {
		let html = response_text(search_page(&database, search_query(query))).await;
		assert_eq!(html.matches(EXACT_MATCH_TAG).count(), expected, "query: {query:?}");
	}
}

#[tokio::test]
async fn exact_match_tags_identify_projects_and_shared_executables_in_ranked_order() {
	let database = sample_database();
	let html = response_text(search_page(&database, search_query("hx"))).await;
	let mut groups = html.split("<article class='name-result'").skip(1);
	for (project, is_exact_project, is_exact_executable) in
		[("helix", false, true), ("hx", true, false), ("other", false, true), ("case", false, false)]
	{
		let group = groups.next().unwrap().split_once("</article>").unwrap().0;
		let (heading, executables) = group.split_once("</header>").unwrap();
		assert!(heading.contains(&format!("href='/projects/{project}'")));
		assert_eq!(heading.contains(EXACT_MATCH_TAG), is_exact_project);
		assert_eq!(executables.contains(EXACT_MATCH_TAG), is_exact_executable);
	}
	assert!(groups.next().is_none());
	assert_executable_row(&html, "helix", "hx", "crates.io, Debian");
	assert_executable_row(&html, "other", "hx", "npm");
}

#[tokio::test]
async fn exact_match_tags_follow_search_filters() {
	let database = sample_database();
	for (channel, initial, expected) in [("npm", "", 1), ("debian", "H", 2), ("pypi", "", 0), ("", "Z", 0)] {
		let search = SearchQuery { channel: channel.to_owned(), initial: initial.to_owned(), ..search_query("hx") };
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.matches(EXACT_MATCH_TAG).count(), expected);
	}
}

#[test]
fn channel_filters_count_only_matching_relations() {
	let database = sample_database();
	let mut search = search_query("hx");
	search.channel = "npm".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].name, "other");
	let related = &results.rows[0].related;
	assert_eq!(related.len(), 1);
	assert_eq!(related[0].name, "hx");
	assert_eq!(related[0].channels, ["npm"]);
	search.channel = "debian".to_owned();
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 3);
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
	for (channel, channels) in [("", 3), ("npm", 1)] {
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!((results.total, results.start, results.end), (1, 1, 1));
		let group = &results.rows[0];
		assert_eq!(group.related.len(), 1);
		assert_eq!(group.related[0].channels.len(), channels);
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
		assert_eq!(results.rows[0].related.len(), 4);
		assert_eq!(results.rows[1].name, "age-plugin-yubikey");
		assert_eq!(executable_names(&results.rows[1]), ["age-plugin-yubikey"]);
	}
	let results = database.search(&search_query("age-keygen")).unwrap();
	assert_eq!(results.total, 1);
	assert_eq!(results.rows[0].name, "age");
	assert_eq!(results.rows[0].related[0].name, "age-keygen");

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
	let search = search_query("tool");
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "tool");
	assert_eq!(results.rows[1].name, "upper");
	assert_eq!(executable_names(&results.rows[0]), ["tool"]);
	assert_eq!(executable_names(&results.rows[1]), ["Tool"]);
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
	let database = fixture((0..68).flat_map(|index| {
		(0..4).map(move |command| {
			provider((&format!("owner-{index:03}"), &format!("command-{command}"), "debian", "toolbox"))
		})
	}));
	let mut search = search_query("");
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
			assert!(!next.contains("kind="));
		}
		for row in results.rows {
			assert_eq!(row.related.len(), 4);
			assert!(names.insert(row.name));
		}
	}
	assert_eq!(names.len(), 68);
	search.page = 4;
	assert_eq!(search_page(&database, search).status(), StatusCode::NOT_FOUND);
}

#[test]
fn search_paginates_projects_without_splitting_executable_lists() {
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
			assert!(!next.contains("kind="));
		}
		for row in results.rows {
			assert_eq!(row.related.len(), if row.name == "tool-000" { 40 } else { 2 });
			assert!(names.insert(row.name));
		}
	}
	assert_eq!(names.len(), 68);
	search.page = 4;
	assert_eq!(search_page(&database, search).status(), StatusCode::NOT_FOUND);
}

#[test]
fn initial_filters_match_project_names_and_keep_complete_executable_lists() {
	let database = fixture(INITIAL_RECORDS.iter().copied().map(provider));
	for (initial, expected) in [
		("#", vec!["-tool", ".tool", "0tool", "9tool", "_tool"]),
		("A", vec!["alpha", "alpine"]),
		("B", vec!["beta"]),
		("Z", vec!["zeta"]),
	] {
		let search = SearchQuery { initial: initial.to_owned(), ..search_query("") };
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, expected.len() as u64);
		assert_eq!(results.rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(), expected);
		assert_eq!(
			results.initial_links.iter().map(|link| link.label.as_str()).collect::<Vec<_>>(),
			["All", "#", "A", "B", "Z"]
		);
		assert_eq!(results.initial_links.iter().filter(|link| link.is_current).count(), 1);
	}
	let search = SearchQuery { initial: "A".to_owned(), ..search_query("LOOKUP") };
	let results = database.search(&search).unwrap();
	assert_eq!(results.total, 2);
	assert_eq!(results.rows[0].name, "alpha");
	assert_eq!(executable_names(&results.rows[0]), ["lookup", "helper"]);
	assert_eq!(results.rows[0].related[0].channels, ["debian", "npm"]);
}

#[test]
fn available_initials_follow_query_and_channel_but_not_the_selected_initial() {
	let database = fixture(INITIAL_RECORDS.iter().copied().map(provider));
	for (query, channel, initial, expected) in [
		("lookup", "", "", vec!["All", "#", "A", "B"]),
		("lookup", "debian", "A", vec!["All", "#", "A"]),
		("alpha", "", "B", vec!["All", "A"]),
		("missing", "", "", vec!["All"]),
	] {
		let search = SearchQuery { channel: channel.to_owned(), initial: initial.to_owned(), ..search_query(query) };
		let results = database.search(&search).unwrap();
		assert_eq!(results.initial_links.iter().map(|link| link.label.as_str()).collect::<Vec<_>>(), expected);
		if query == "alpha" || query == "missing" {
			assert_eq!(results.total, 0);
		}
	}
}

#[test]
fn initial_filters_apply_before_pagination_and_choices_cover_all_pages() {
	let database = fixture(
		(0..35)
			.map(|index| provider((&format!("alpha-{index:03}"), "tool", "npm", "tools")))
			.chain([provider(("zeta", "tool", "npm", "tools")), provider(("0tool", "tool", "npm", "tools"))]),
	);
	let mut search = SearchQuery { initial: "A".to_owned(), channel: "npm".to_owned(), ..search_query("tool") };
	let mut names = BTreeSet::new();
	for page in 1..=2 {
		search.page = page;
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 35);
		assert_eq!(results.rows.len(), if page == 1 { 30 } else { 5 });
		assert_eq!(
			results.initial_links.iter().map(|link| link.label.as_str()).collect::<Vec<_>>(),
			["All", "#", "A", "Z"]
		);
		for row in results.rows {
			assert!(row.name.starts_with('a'));
			assert!(names.insert(row.name));
		}
		let url = if page == 1 { results.next.unwrap() } else { results.previous.unwrap() };
		let Query(parsed) = Query::<SearchQuery>::try_from_uri(&url.parse().unwrap()).unwrap();
		assert_eq!(parsed.initial, "A");
		assert_eq!(parsed.query(), "tool");
		assert_eq!(parsed.channel, "npm");
		assert_eq!(parsed.page, if page == 1 { 2 } else { 1 });
	}
	assert_eq!(names.len(), 35);
}

#[test]
fn initial_links_preserve_search_context_encode_symbols_and_reset_the_page() {
	let search =
		SearchQuery { initial: "A".to_owned(), channel: "npm".to_owned(), page: 4, ..search_query(" a & é?# ") };
	let links = search.initial_links(["#", "A", "Z"].map(str::to_owned));
	assert!(links[1].url.contains("initial=%23"));
	for link in links {
		let Query(parsed) = Query::<SearchQuery>::try_from_uri(&link.url.parse().unwrap()).unwrap();
		assert_eq!(parsed.query(), "a & é?#");
		assert_eq!(parsed.channel, "npm");
		assert_eq!(parsed.page, 1);
		assert_eq!(parsed.initial, if link.label == "All" { "" } else { &link.label });
		assert_eq!(link.is_current, link.label == "A");
	}
}

#[test]
fn channel_links_preserve_query_and_initial_while_resetting_pagination() {
	for channel in ["", "npm"] {
		let search =
			SearchQuery { channel: channel.to_owned(), initial: "#".to_owned(), page: 4, ..search_query(" a & é?# ") };
		let links = search.channel_links();
		assert_eq!(links.len(), super::CHANNELS.len() + 1);
		assert_eq!(links.iter().filter(|link| link.is_current).count(), 1);
		for (link, expected_channel) in links.iter().zip(std::iter::once("").chain(super::CHANNELS)) {
			let Query(parsed) = Query::<SearchQuery>::try_from_uri(&link.url.parse().unwrap()).unwrap();
			assert_eq!(parsed.query(), "a & é?#");
			assert_eq!(parsed.initial, "#");
			assert_eq!(parsed.channel, expected_channel);
			assert_eq!(parsed.page, 1);
			assert_eq!(link.is_current, channel == expected_channel);
		}
	}
}

#[tokio::test]
async fn channel_choices_remain_available_outside_the_search_form() {
	for database in [sample_database(), fixture([])] {
		let search = SearchQuery { channel: "npm".to_owned(), ..search_query("hx") };
		let html = response_text(search_page(&database, search)).await;
		let navigation = html.split_once("<nav class='channel-filter'").unwrap().1;
		let navigation = navigation.split_once("</nav>").unwrap().0;
		assert!(navigation.contains("aria-label='Filter matches by channel'"));
		assert_eq!(navigation.matches("<a ").count(), super::CHANNELS.len() + 1);
		assert_eq!(navigation.matches("aria-current='true'").count(), 1);
		assert!(navigation.contains("aria-current='true'>npm</a>"));
		assert!(navigation.contains(">All channels</a>"));
		for channel in super::CHANNELS {
			assert!(navigation.contains(&format!(">{}</a>", super::channel_label(channel))));
		}
		assert!(html.find("</form>").unwrap() < html.find("class='channel-filter'").unwrap());
		assert!(html.find("class='channel-filter'").unwrap() < html.find("class='initial-filter'").unwrap());
	}
}

#[tokio::test]
async fn clearing_a_filter_preserves_the_query_and_other_filter_and_returns_to_page_one() {
	const QUERY: &str = "a & é?#";
	let database = fixture((0..35).map(|index| provider((&format!("alpha-{index:02}"), QUERY, "npm", "tools"))));
	let search = SearchQuery {
		channel: "npm".to_owned(),
		initial: "A".to_owned(),
		page: 2,
		..search_query(&format!(" {QUERY} "))
	};
	let response = search_page(&database, search);
	assert_eq!(response.status(), StatusCode::OK);
	let html = response_text(response).await;
	for (label, channel, initial) in [("Clear channel", "", "A"), ("Clear initial", "npm", "")] {
		let parsed = search_link_query(&html, label);
		assert_eq!(parsed.query(), QUERY);
		assert_eq!(parsed.channel, channel);
		assert_eq!(parsed.initial, initial);
		assert_eq!(parsed.page, 1);
		assert_eq!(search_page(&database, parsed).status(), StatusCode::OK);
	}
}

#[tokio::test]
async fn empty_results_offer_independent_filter_recovery_for_search_and_browse() {
	let database = sample_database();
	for query in ["hx", ""] {
		let search = SearchQuery { channel: "npm".to_owned(), initial: "Z".to_owned(), ..search_query(query) };
		let html = response_text(search_page(&database, search)).await;
		let empty_state = html.split_once("<div class='empty-state'>").unwrap().1;
		let empty_state = empty_state.split_once("</div>").unwrap().0;
		for (filter, channel, initial) in [("channels", "", "Z"), ("initials", "npm", "")] {
			let label = if query.is_empty() {
				format!("Browse all {filter}")
			} else {
				format!("Search all {filter} for <code>“{query}”</code>")
			};
			let parsed = search_link_query(empty_state, &label);
			assert_eq!(parsed.query(), query);
			assert_eq!(parsed.channel, channel);
			assert_eq!(parsed.initial, initial);
			assert_eq!(parsed.page, 1);
		}
		let parsed = search_link_query(empty_state, "Clear both filters");
		assert_eq!(parsed.query(), query);
		assert!(parsed.channel.is_empty() && parsed.initial.is_empty());
		assert_eq!(parsed.page, 1);
		assert!(database.search(&parsed).unwrap().total > 0);
	}
}

#[tokio::test]
async fn filter_notices_and_recovery_actions_follow_active_filters_even_without_matches() {
	let database = fixture([]);
	for (channel, initial) in [("", ""), ("npm", ""), ("", "Z"), ("npm", "#")] {
		let search =
			SearchQuery { channel: channel.to_owned(), initial: initial.to_owned(), ..search_query("missing") };
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.contains(">Clear channel</a>"), !channel.is_empty());
		assert_eq!(html.contains(">Clear initial</a>"), !initial.is_empty());
		assert_eq!(html.contains("Search all channels"), !channel.is_empty());
		assert_eq!(html.contains("Search all initials"), !initial.is_empty());
		assert_eq!(html.contains(">Clear both filters</a>"), !channel.is_empty() && !initial.is_empty());
		if !initial.is_empty() {
			let label = if initial == "#" { "Numbers and symbols" } else { initial };
			assert!(html.contains(&format!("<strong>{label}</strong>")));
		}
	}
	let html = response_text(search_page(&database, search_query(""))).await;
	assert!(html.contains("No names are listed in this release."));
	assert!(!html.contains("class='search-recovery'"));
}

#[tokio::test]
async fn initial_filter_renders_available_links_above_results_and_on_empty_searches() {
	let database = fixture(INITIAL_RECORDS.iter().copied().map(provider));
	let search = SearchQuery { q: None, initial: "A".to_owned(), channel: "debian".to_owned(), ..search_query("") };
	let html = response_text(search_page(&database, search)).await;
	let (_, navigation) = html.split_once("<nav class='initial-filter'").unwrap();
	let (navigation, _) = navigation.split_once("</nav>").unwrap();
	assert!(navigation.contains("aria-label='Filter projects by initial'"));
	assert_eq!(navigation.matches("<a ").count(), 4);
	assert_eq!(navigation.matches("aria-current='true'").count(), 1);
	assert!(navigation.contains("Numbers and symbols"));
	assert!(!navigation.contains(">B</a>"));
	assert!(html.find("class='initial-filter'").unwrap() < html.find("class='name-results'").unwrap());
	assert!(html.contains("<input type='hidden' name='initial' value='A'>"));
	assert!(html.contains("2 matching projects"));
	let html = response_text(search_page(&database, search_query("missing"))).await;
	assert!(html.contains("No matches"));
	let (_, navigation) = html.split_once("<nav class='initial-filter'").unwrap();
	let (navigation, _) = navigation.split_once("</nav>").unwrap();
	assert_eq!(navigation.matches("<a ").count(), 1);
	assert!(navigation.contains(">All</a>"));
}

#[tokio::test]
async fn landing_example_reuses_age_search_result_with_three_executables_and_project_link() {
	let records = [
		("age", "age", "arch", "age"),
		("age", "age", "fedora", "age"),
		("age", "age", "homebrew", "homebrew/core/age"),
		("age", "age", "debian", "age-extra"),
		("age", "age-plugin-pq", "debian", "age"),
		("age", "age-plugin-tag", "debian", "age"),
		("age", "age-plugin-tagpq", "debian", "age"),
		("age", "age-plugin-tagtest", "debian", "age"),
		("other", "age", "pypi", "unrelated"),
	];
	let database = fixture(RECORDS.iter().chain(AGE_RECORDS).chain(&records).copied().map(provider));
	let example = database.example.as_ref().unwrap();
	assert_eq!(example.name, "age");
	let results = database.search(&search_query("age")).unwrap();
	assert_eq!(executable_names(example), executable_names(&results.rows[0]));
	assert_eq!(example.related.len(), 8);
	assert_eq!(example.related[0].channels, ["arch", "debian", "fedora", "homebrew"]);

	let html = response_text(search_page(&database, SearchQuery { q: None, ..search_query("") })).await;
	let (_, card) = html.split_once("<aside class='relation-card'").unwrap();
	let (card, _) = card.split_once("</aside>").unwrap();
	let card = card.split_whitespace().collect::<Vec<_>>().join(" ");
	assert!(card.contains("class='name-result'"));
	assert_eq!(card.matches("class='related-name'").count(), 3);
	for relation in &example.related[..3] {
		assert!(card.contains(&format!("href='/projects/age#executable-{}'", relation.name)));
	}
	assert!(card.contains("Arch Linux, Debian, Fedora, Homebrew"));
	assert!(card.contains("class='more-related text-link' href='&#x2f;projects&#x2f;age'> 5 more executables</a>"));
	assert!(!card.contains("age-plugin-"));
	assert!(!card.contains("<details"));
	assert!(!card.contains("Package"));
	assert!(!card.contains(EXACT_MATCH_TAG));
	assert!(!html.contains("Example lookup"));
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
	assert!(html.contains("No executables listed in this release."));
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
async fn project_exports_include_all_executables_channels_and_packages_for_one_owner() {
	let records: Vec<_> = SHARED_RECORDS
		.iter()
		.chain(AGE_RECORDS)
		.chain(MATRIX_RECORDS)
		.copied()
		.map(provider)
		.collect();
	let database = Arc::new(fixture(records.clone()));
	for project in ["eslint", "age", "tools"] {
		let response = super::exports::project(State(database.clone()), Path(project.to_owned())).await;
		let exported = exported_providers(&database, response, serde_json::json!({ "project": project })).await;
		let mut expected: Vec<_> = records.iter().filter(|provider| provider.project == project).cloned().collect();
		expected.sort();
		assert_eq!(exported, expected);
	}
}

#[tokio::test]
async fn executable_exports_preserve_exact_names_and_include_every_owner() {
	let records: Vec<_> = RECORDS
		.iter()
		.copied()
		.chain([("json-tool", "tool.json", "npm", "@scope/json-tool")])
		.map(provider)
		.collect();
	let database = Arc::new(fixture(records.clone()));
	for executable in ["hx", "HX", "tool%_", "étool", "<tool>&'", "tool.json"] {
		let response = super::exports::executable(State(database.clone()), Path(executable.to_owned())).await;
		let exported = exported_providers(&database, response, serde_json::json!({ "executable": executable })).await;
		let mut expected: Vec<_> = records
			.iter()
			.filter(|provider| provider.executable == executable)
			.cloned()
			.collect();
		expected.sort();
		assert_eq!(exported, expected);
	}
}

#[tokio::test]
async fn repeated_exports_are_identical_and_leave_the_database_unchanged() {
	let database = Arc::new(sample_database());
	let before = fs::read(&database.path).unwrap();
	let first = super::exports::executable(State(database.clone()), Path("hx".to_owned())).await;
	let second = super::exports::executable(State(database.clone()), Path("hx".to_owned())).await;
	assert_eq!(response_text(first).await, response_text(second).await);
	assert_eq!(fs::read(&database.path).unwrap(), before);
}

#[tokio::test]
async fn missing_export_identities_return_json_errors_instead_of_empty_results() {
	for database in [Arc::new(sample_database()), Arc::new(fixture([]))] {
		for name in ["missing", "Helix", "Hx", "%", "' OR 1=1 --"] {
			for response in [
				super::exports::project(State(database.clone()), Path(name.to_owned())).await,
				super::exports::executable(State(database.clone()), Path(name.to_owned())).await,
			] {
				assert_eq!(response.status(), StatusCode::NOT_FOUND);
				assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
				assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
				assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
				let error: serde_json::Value = serde_json::from_str(&response_text(response).await).unwrap();
				assert_eq!(error.as_object().unwrap().len(), 1);
				assert_eq!(
					error["error"],
					"No accepted mappings for this exact name are listed in this release. Check spelling and case."
				);
			}
		}
	}
}

#[tokio::test]
async fn export_failures_return_json_without_exposing_database_details() {
	let database = Arc::new(sample_database());
	fs::remove_file(&database.path).unwrap();
	for response in [
		super::exports::project(State(database.clone()), Path("helix".to_owned())).await,
		super::exports::executable(State(database.clone()), Path("hx".to_owned())).await,
	] {
		assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
		assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
		assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
		let error: serde_json::Value = serde_json::from_str(&response_text(response).await).unwrap();
		assert_eq!(error, serde_json::json!({ "error": "The export could not be generated. Try again later." }));
	}
}

#[tokio::test]
async fn detail_pages_offer_inline_json_links_and_progressive_copy_controls() {
	let database = sample_database();
	let mut pages = vec![(project_page(&database, "helix"), "/projects/helix/providers.json".to_owned())];
	for executable in ["hx", "HX", "tool%_", "étool", "<tool>&'"] {
		let path = super::encode_path_segment(executable);
		pages.push((executable_page(&database, executable), format!("/executables/{path}/providers.json")));
	}
	for (response, path) in pages {
		let policy = response.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
		assert!(policy.contains("script-src 'self'"));
		assert!(policy.contains("connect-src 'self'"));
		assert!(!policy.contains("unsafe-inline") && !policy.contains("unsafe-eval"));
		let html = response_text(response).await;
		assert!(
			html.replace("&#x2f;", "/")
				.contains(&format!("href='{path}' data-view-json>View JSON</a>"))
		);
		assert!(html.contains("type='button' data-copy-json hidden>Copy JSON</button>"));
		assert!(html.contains("class='export-status visually-hidden' role='status' aria-live='polite'></p>"));
		assert_eq!(html.matches("src='/exports.js'").count(), 1);
		assert!(html.contains("<script type='module' src='/exports.js'></script>"));
		assert!(!html.contains("download") && !html.contains("<tool>"));
	}
	for response in [
		project_page(&database, "missing"),
		executable_page(&database, "missing"),
		search_page(&database, search_query("hx")),
		search_page(&database, SearchQuery { q: None, ..search_query("") }),
	] {
		let html = response_text(response).await;
		assert!(!html.contains("/exports.js"));
		assert!(!html.contains("data-json-export"));
	}
}

#[tokio::test]
async fn copy_script_is_served_as_a_same_origin_asset() {
	let response = super::exports::script().await;
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(response.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
	assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
	assert_eq!(response.headers()[header::CACHE_CONTROL], super::asset_cache_control());
	let script = response_text(response).await;
	assert!(script.contains("navigator.clipboard"));
	assert!(script.contains("Copy failed. Use View JSON to copy manually."));
}

#[tokio::test]
async fn search_shortcut_script_is_served_with_the_standard_asset_headers() {
	let response = super::search_script().await;
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(response.headers()[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
	assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
	assert_eq!(response.headers()[header::CACHE_CONTROL], super::asset_cache_control());
	let script = response_text(response).await;
	assert!(script.contains("aria-keyshortcuts"));
	assert!(script.contains("event.key !== '/'"));
}

#[tokio::test]
async fn search_shortcut_is_loaded_once_on_main_and_detail_pages() {
	let database = Arc::new(sample_database());
	for response in [
		search_page(&database, SearchQuery { q: None, ..search_query("") }),
		search_page(&database, search_query("hx")),
		search_page(&database, SearchQuery { page: 0, ..search_query("") }),
		project_page(&database, "helix"),
		executable_page(&database, "hx"),
		about(State(database.clone())).await,
	] {
		let html = response_text(response).await;
		assert_eq!(html.matches("<script type='module' src='/search.js'></script>").count(), 1);
		assert!(!html.contains("onkeydown="));
	}
}

#[test]
fn project_matrix_orders_and_deduplicates_exact_channel_package_relations() {
	let database = fixture(MATRIX_RECORDS.iter().copied().map(provider));
	let mut providers = database.project_providers("tools").unwrap();
	providers.reverse();
	providers.extend(database.project_providers("tools").unwrap());
	let matrix = super::build_project_matrix(providers);
	let columns: Vec<_> = matrix
		.channels
		.iter()
		.flat_map(|channel| {
			channel
				.packages
				.iter()
				.map(|package| (channel.channel.as_str(), package.reference.name.as_str()))
		})
		.collect();
	assert_eq!(
		columns,
		[
			("arch", "tools"),
			("debian", "tools"),
			("debian", "tools-extra"),
			("homebrew", "custom/tap/tools"),
			("npm", "@scope/tools"),
		]
	);
	assert_eq!(matrix.executables, MATRIX_EXECUTABLES);
	let observed: Vec<_> = matrix
		.channels
		.iter()
		.flat_map(|channel| channel.packages.iter().map(|package| package.observed.clone()))
		.collect();
	assert_eq!(observed, MATRIX_PACKAGE_COVERAGE);
}

#[tokio::test]
async fn project_matrix_renders_grouped_headers_links_and_exact_coverage() {
	let database = fixture(MATRIX_RECORDS.iter().copied().map(provider));
	let html = response_text(project_page(&database, "tools")).await;
	assert_eq!(html.matches("<table ").count(), 1);
	assert_eq!(html.matches("scope='col'>Package</th>").count(), 1);
	assert_eq!(html.matches("class='matrix-executable' scope='col'").count(), MATRIX_EXECUTABLES.len());
	assert_eq!(html.matches("class='matrix-name' scope='row'").count(), MATRIX_PACKAGE_COVERAGE.len());
	assert_eq!(html.matches("scope='rowgroup'").count(), 4);
	assert!(html.contains("colspan='4'"));
	assert!(html.contains("<span class='matrix-channel-label'>Debian</span>"));
	assert!(html.contains("<span class='matrix-channel-label'>Arch Linux</span>"));
	assert!(html.contains("role='region' aria-labelledby='providers-title' tabindex='0'"));
	assert!(html.contains("aria-describedby='matrix-legend'"));
	assert!(html.contains("Observed in this package."));
	assert!(html.contains("Not observed in this dataset."));
	assert!(html.contains("An unlisted command may still be available."));
	assert!(!html.contains("3 executables."));
	assert!(!html.contains("provider record"));
	for url in [
		"https:&#x2f;&#x2f;archlinux.org&#x2f;packages&#x2f;?q=tools",
		"https:&#x2f;&#x2f;packages.debian.org&#x2f;tools",
		"https:&#x2f;&#x2f;packages.debian.org&#x2f;tools-extra",
		"https:&#x2f;&#x2f;www.npmjs.com&#x2f;package&#x2f;%40scope&#x2f;tools",
	] {
		assert_eq!(html.matches(&format!("href='{url}'")).count(), 1);
	}
	assert_eq!(html.matches("class='package-link'").count(), 4);
	assert!(html.contains("<code>custom&#x2f;tap&#x2f;tools</code>"));
	for executable in MATRIX_EXECUTABLES {
		assert_eq!(html.matches(&format!("id='executable-{executable}'")).count(), 1);
		assert!(html.contains(&format!("href='/executables/{executable}'")));
	}
	let rows: Vec<_> = html
		.split("<tr>")
		.filter(|row| row.contains("class='matrix-name' scope='row'"))
		.collect();
	assert_eq!(rows.len(), MATRIX_PACKAGE_COVERAGE.len());
	for (row, observed) in rows.iter().zip(MATRIX_PACKAGE_COVERAGE) {
		let cells: Vec<_> = row.split("<td>").skip(1).collect();
		assert_eq!(cells.len(), observed.len());
		for (cell, is_observed) in cells.iter().zip(observed) {
			assert_eq!(cell.contains("class='visually-hidden'>Observed</span>"), *is_observed);
			assert_eq!(cell.contains("class='visually-hidden'>Not observed</span>"), !is_observed);
		}
	}
}

#[tokio::test]
async fn project_matrix_shows_shared_package_once_and_excludes_other_projects() {
	let database = fixture(AGE_RECORDS.iter().copied().map(provider));
	let html = response_text(project_page(&database, "age")).await;
	assert_eq!(html.matches("class='matrix-name' scope='row'").count(), 1);
	assert_eq!(html.matches("class='matrix-executable' scope='col'").count(), 4);
	assert_eq!(html.matches("class='visually-hidden'>Observed</span>").count(), 4);
	assert!(!html.contains("class='visually-hidden'>Not observed</span>"));
	assert!(!html.contains("age-plugin-yubikey"));
	assert_eq!(html.matches("href='https:&#x2f;&#x2f;packages.debian.org&#x2f;age'").count(), 1);
}

#[tokio::test]
async fn project_matrix_escapes_names_and_preserves_encoded_executable_links() {
	let database = fixture([provider(("tools", "<tool>&'", "npm", "@scope/<package>&'"))]);
	let html = response_text(project_page(&database, "tools")).await;
	assert!(html.contains("id='executable-&lt;tool&gt;&amp;&#x27;'"));
	assert!(html.contains("href='/executables/%3Ctool%3E%26%27'"));
	assert!(html.contains("<code>&lt;tool&gt;&amp;&#x27;</code>"));
	assert!(html.contains("<code>@scope&#x2f;&lt;package&gt;&amp;&#x27;</code>"));
	assert!(html.contains("%40scope&#x2f;%3Cpackage%3E%26%27"));
	assert!(!html.contains("<tool>"));
	assert!(!html.contains("<package>"));
	assert!(!html.contains("1 executable."));
	assert!(!html.contains("provider record"));
}

#[tokio::test]
async fn renders_one_project_summary_with_explicit_executable_links() {
	let database = fixture(SHARED_RECORDS.iter().copied().map(provider));
	let html = response_text(search_page(&database, search_query("eslint"))).await;
	assert_eq!(html.matches("class='name-result'").count(), 1);
	assert_eq!(html.matches("<code><mark>eslint</mark></code>").count(), 1);
	assert_eq!(html.matches("<code><mark>eslint</mark>(1)</code>").count(), 1);
	assert!(!html.contains("Project:"));
	assert!(!html.contains("class='result-summary'"));
	assert!(!html.contains("3 channels"));
	assert!(!html.contains("4 provider records"));
	assert_executable_row(&html, "eslint", "eslint", "Debian, Homebrew, npm");
	assert_eq!(html.matches("href='/projects/eslint'").count(), 1);
	let normalized = html.split_whitespace().collect::<Vec<_>>().join(" ");
	assert!(normalized.contains(&format!(
		"<h3 id='result-1'> <a href='/projects/eslint'> <code><mark>eslint</mark></code></a> {EXACT_MATCH_TAG} </h3>"
	)));
	assert!(!html.contains("Project details"));
	assert!(html.contains("1 matching project"));
	assert!(!html.contains("class='results-table'"));
	assert!(!html.contains("name='kind'"));
	assert!(!html.contains("class='result-type'"));
	assert!(!html.contains("Provided by"));
	assert!(!html.contains("Provides"));
	assert_eq!(html.matches(EXACT_MATCH_TAG).count(), 2);
	assert!(!html.contains("↗"));
}

#[tokio::test]
async fn project_groups_include_nonprefixed_executables_and_use_project_counts() {
	let database = fixture(AERC_RECORDS.iter().copied().map(provider));
	for (channel, channels) in [("", 2), ("debian", 1)] {
		let mut search = search_query("aerc");
		search.channel = channel.to_owned();
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 1);
		let group = &results.rows[0];
		assert_eq!(executable_names(group), ["aerc", "carddav-query"]);
		assert_eq!(group.related[1].channels.len(), channels);
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.matches("class='name-result'").count(), 1);
		assert_eq!(html.matches("<code><mark>aerc</mark></code>").count(), 1);
		assert_executable_row(&html, "aerc", "aerc", if channel.is_empty() { "Debian" } else { "" });
		assert_executable_row(&html, "aerc", "carddav-query", if channel.is_empty() { "Debian, Homebrew" } else { "" });
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
	assert_executable_row(&html, "aerc", "carddav-query", "");
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

#[test]
fn highlighting_preserves_spelling_and_matches_search_case_rules() {
	for (name, query, expected) in [
		("tool-tool", "TOOL", "[tool]-[tool]"),
		("tool", "tool", "[tool]"),
		("tool", "oo", "t[oo]l"),
		("tool", "", "tool"),
		("tool", "other", "tool"),
		("", "tool", ""),
		("éTool", "tO", "é[To]ol"),
		("éTool", "é", "[é]Tool"),
		("éTool", "É", "éTool"),
		("tool%_.", "%_.", "tool[%_.]"),
		("tool", ".*", "tool"),
	] {
		let parts = super::highlight_parts(name, query);
		assert_eq!(parts.iter().map(|part| part.text).collect::<String>(), name);
		let highlighted: String = parts
			.iter()
			.map(|part| if part.is_match { format!("[{}]", part.text) } else { part.text.to_owned() })
			.collect();
		assert_eq!(highlighted, expected);
	}
}

#[tokio::test]
async fn search_highlights_matches_in_project_and_executable_names() {
	let database = fixture(
		[("tool-tool", "Tool-helper-tool", "npm", "tools"), ("tool-tool", "helper", "npm", "tools")]
			.into_iter()
			.map(provider),
	);
	let html = response_text(search_page(&database, search_query("  TOOL  "))).await;
	assert!(html.contains("<code><mark>tool</mark>-<mark>tool</mark></code>"));
	assert!(html.contains("<code><mark>Tool</mark>-helper-<mark>tool</mark>(1)</code>"));
	assert!(html.contains("<code>helper(1)</code>"));
	assert_eq!(html.matches("<mark>").count(), 4);
	assert_executable_row(&html, "tool-tool", "Tool-helper-tool", "npm");
	for query in ["", "  "] {
		let html = response_text(search_page(&database, search_query(query))).await;
		assert!(!html.contains("<mark>"));
		assert!(html.contains("<code>tool-tool</code>"));
		assert!(html.contains("<code>Tool-helper-tool(1)</code>"));
	}
}

#[tokio::test]
async fn highlighted_names_remain_escaped_and_detail_links_remain_encoded() {
	let executable = "<tool>&'";
	let database = fixture([provider(("owner", executable, "debian", "tools"))]);
	for (query, expected) in [
		("", "&lt;tool&gt;&amp;&#x27;"),
		(executable, "<mark>&lt;tool&gt;&amp;&#x27;</mark>"),
		("<tool>", "<mark>&lt;tool&gt;</mark>&amp;&#x27;"),
		("tool", "&lt;<mark>tool</mark>&gt;&amp;&#x27;"),
		("&'", "&lt;tool&gt;<mark>&amp;&#x27;</mark>"),
	] {
		let html = response_text(search_page(&database, search_query(query))).await;
		assert!(html.contains("<code>owner</code>"));
		assert!(html.contains(&format!("<code>{expected}(1)</code>")));
		assert_eq!(html.contains(EXACT_MATCH_TAG), query == executable);
		assert!(!html.contains(executable));
		assert!(html.contains("href='/projects/owner'"));
		assert_executable_row(&html, "owner", executable, "Debian");
	}
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
		assert_eq!(html.contains("<details class='more-related'>"), count > 3);
		if count > 3 {
			let (preview, expanded) = html.split_once("<details class='more-related'>").unwrap();
			assert_eq!(preview.matches("class='related-name'").count(), 3);
			assert!(expanded.contains("1 more executable"));
			assert_executable_row(expanded, "tools", "command-3", "npm");
			let html = response_text(search_page(&database, search_query("command"))).await;
			let expanded = html.split_once("<details class='more-related'>").unwrap().1;
			assert!(expanded.contains("<mark>command</mark>-3"));
			let search = search_query("command-3");
			assert_eq!(database.search(&search).unwrap().rows[0].related[0].name, "command-3");
			let html = response_text(search_page(&database, search)).await;
			let (preview, expanded) = html.split_once("<details class='more-related'>").unwrap();
			assert_executable_row(preview, "tools", "command-3", "npm");
			assert!(preview.contains("<mark>command-3</mark>"));
			assert_eq!(preview.matches(EXACT_MATCH_TAG).count(), 1);
			assert!(!expanded.contains(EXACT_MATCH_TAG));
		}
	}
}

#[tokio::test]
async fn channel_filtered_previews_and_disclosures_omit_channel_lists() {
	const EXECUTABLES: [&str; 4] = ["age", "age-inspect", "age-keygen", "age-plugin-batchpass"];
	let database = fixture(AGE_RECORDS.iter().copied().map(provider));
	for query in ["", "age", "age-plugin-batchpass"] {
		let search = SearchQuery { channel: "debian".to_owned(), ..search_query(query) };
		let html = response_text(search_page(&database, search)).await;
		assert!(html.contains("Showing matches in <strong>Debian</strong>"));
		assert!(html.contains("<details class='more-related'>"));
		for executable in EXECUTABLES {
			assert_executable_row(&html, "age", executable, "");
		}
		let unfiltered = response_text(search_page(&database, search_query(query))).await;
		for executable in EXECUTABLES {
			assert_executable_row(&unfiltered, "age", executable, "Debian");
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
	assert_executable_row(&html, "other", "hx", "");
}

#[tokio::test]
async fn search_previews_three_executables_and_expands_the_remainder() {
	for count in [1, 2, 3, 4, 6] {
		let database = fixture((0..count).flat_map(|index| {
			let executable = format!("related-{index}");
			/* Multiple packages in one channel must not duplicate rows or channel labels. */
			[provider(("tools", &executable, "npm", "tools")), provider(("tools", &executable, "npm", "alias"))]
		}));
		let search = search_query("tools");
		let results = database.search(&search).unwrap();
		assert_eq!(results.total, 1);
		let related = &results.rows[0].related;
		assert_eq!(related.len(), count);
		for (index, relation) in related.iter().enumerate() {
			assert_eq!(relation.name, format!("related-{index}"));
			assert_eq!(relation.channels, ["npm"]);
		}
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html.matches("class='name-result'").count(), 1);
		assert_eq!(html.matches("class='related-name'").count(), count);
		assert!(!html.contains("Provides"));
		assert!(!html.contains("Provided by"));
		if count > 3 {
			let (preview, expanded) = html.split_once("<details class='more-related'>").unwrap();
			assert_eq!(preview.matches("class='related-name'").count(), 3);
			assert!(!preview.contains("related-3"));
			assert!(expanded.contains("related-3"));
			let plural = if count == 4 { "" } else { "s" };
			assert!(expanded.contains(&format!("{} more executable{plural}", count - 3)));
		} else {
			assert!(!html.contains("<details class='more-related'>"));
		}
	}
}

#[tokio::test]
async fn detail_pages_offer_one_unfiltered_search_form_in_the_header() {
	let database = sample_database();
	for response in [project_page(&database, "helix"), executable_page(&database, "hx")] {
		let html = response_text(response).await;
		assert_eq!(html.matches("<form ").count(), 1);
		assert_eq!(html.matches("id='q'").count(), 1);
		assert_eq!(html.matches("id='search-hint'").count(), 1);
		assert!(html.contains("<label class='visually-hidden' for='q'>Executable name or project ID</label>"));
		let header = html.split_once("<header class='site-header has-search'>").unwrap().1;
		let header = header.split_once("</header>").unwrap().0;
		let navigation = header.split_once("<nav aria-label='Main navigation'>").unwrap().1;
		let navigation = navigation.split_once("</nav>").unwrap().0;
		assert!(!navigation.contains(">Search</a>"));
		assert!(navigation.find("</form>").unwrap() < navigation.find("href='/about'").unwrap());
		assert!(header.contains("<p class='visually-hidden' id='search-hint'>Search by full or partial name.</p>"));
		let form = header.split_once("<form class='search-form'").unwrap().1;
		let form = form.split_once("</form>").unwrap().0;
		assert!(form.contains("action='/' method='get' role='search'"));
		assert!(form.contains("name='q' type='search' value=''"));
		assert!(form.contains("placeholder='Press / to'"));
		assert!(form.contains("maxlength='100'"));
		assert!(form.contains("aria-describedby='search-hint'"));
		assert!(form.contains("type='submit'"));
		for field in ["channel", "initial", "page"] {
			assert!(!form.contains(&format!("name='{field}'")));
		}
		assert!(!form.contains("<select"));
		let main = html.split_once("<main id='main' tabindex='-1'>").unwrap().1;
		let main = main.split_once("</main>").unwrap().0;
		assert!(!main.contains("<form "));
	}
}

#[tokio::test]
async fn other_pages_keep_their_existing_forms_without_a_header_search() {
	let database = Arc::new(sample_database());
	for (response, form_count) in [
		(search_page(&database, SearchQuery { q: None, ..search_query("") }), 1),
		(search_page(&database, search_query("hx")), 1),
		(search_page(&database, SearchQuery { page: 0, ..search_query("") }), 1),
		(about(State(database.clone())).await, 0),
	] {
		let html = response_text(response).await;
		let header = html.split_once("<header class='site-header'>").unwrap().1;
		let header = header.split_once("</header>").unwrap().0;
		assert!(!header.contains("<form "));
		assert!(header.contains(">Search</a>"));
		assert_eq!(html.matches("<form ").count(), form_count);
		assert_eq!(html.matches("id='q'").count(), form_count);
		assert_eq!(html.matches("id='search-hint'").count(), form_count);
	}
}

#[tokio::test]
async fn web_pages_hide_provider_record_details() {
	for database in [Arc::new(sample_database()), Arc::new(fixture([]))] {
		let mut pages = vec![
			search_page(&database, SearchQuery { q: None, ..search_query("") }),
			project_page(&database, "helix"),
			executable_page(&database, "hx"),
			about(State(database.clone())).await,
		];
		for channel in ["", "debian"] {
			let search = SearchQuery { channel: channel.to_owned(), ..search_query("") };
			pages.push(search_page(&database, search));
		}
		for response in pages {
			let html = response_text(response).await;
			assert!(!html.to_ascii_lowercase().contains("provider record"));
			assert!(!html.contains("provider_count"));
			assert!(!html.contains("name='kind'"));
			assert!(!html.contains("kind="));
			assert!(!html.contains("<th scope='col'>Records</th>"));
		}
	}
}

#[tokio::test]
async fn search_boxes_have_no_channel_selector() {
	let database = sample_database();
	for search in [
		SearchQuery { q: None, ..search_query("") },
		search_query("hx"),
		SearchQuery { channel: "debian".to_owned(), ..search_query("hx") },
		SearchQuery { page: 0, ..search_query("") },
	] {
		let html = response_text(search_page(&database, search)).await;
		let (_, form) = html.split_once("<form class='search-form'").unwrap();
		let (form, _) = form.split_once("</form>").unwrap();
		assert!(form.contains("name='q'"));
		assert!(form.contains("placeholder='Press / to'"));
		assert!(form.contains("type='submit'"));
		assert!(!form.contains("<select"));
		assert!(!form.contains("search-filters"));
		assert!(!form.contains("for='channel'"));
	}
}

#[tokio::test]
async fn channel_browse_links_preserve_context_when_refining_search() {
	let database = sample_database();
	let home = response_text(search_page(&database, SearchQuery { q: None, ..search_query("") })).await;
	assert!(home.contains("href='/?q=&channel=debian'"));
	assert!(!home.contains("name='channel'"));
	let search = SearchQuery { channel: "debian".to_owned(), ..search_query("hx") };
	let html = response_text(search_page(&database, search)).await;
	assert!(html.contains("<input type='hidden' name='channel' value='debian'>"));
	assert!(html.contains("Showing matches in <strong>Debian</strong>"));
	assert!(html.contains("href='/?q='>Reset search</a>"));
	assert_executable_row(&html, "helix", "hx", "");
}

#[tokio::test]
async fn legacy_type_parameters_use_the_same_project_grouped_search() {
	let database = sample_database();
	let expected = response_text(search_page(&database, search_query("hx"))).await;
	for kind in ["all", "project", "executable", "unknown"] {
		let uri = format!("/?q=hx&kind={kind}");
		let Query(search) = Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap()).unwrap();
		let html = response_text(search_page(&database, search)).await;
		assert_eq!(html, expected);
		assert!(!html.contains("name='kind'"));
		assert!(!html.contains("kind="));
	}
}

#[tokio::test]
async fn handles_invalid_queries_and_missing_identities() {
	let database = sample_database();
	for uri in ["/?page=0", "/?channel=unknown", &format!("/?q={}", "x".repeat(101))] {
		let Query(search) = Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap()).unwrap();
		assert_eq!(search_page(&database, search).status(), StatusCode::BAD_REQUEST);
	}
	for initial in ["a", "AA", "1", "%", "É", "' OR 1=1 --"] {
		let search = SearchQuery { initial: initial.to_owned(), ..search_query("") };
		assert_eq!(search_page(&database, search).status(), StatusCode::BAD_REQUEST);
	}
	for uri in ["/?page=-1", "/?page=abc", "/?page=4294967296"] {
		assert!(Query::<SearchQuery>::try_from_uri(&uri.parse().unwrap()).is_err());
	}
	assert_eq!(executable_page(&database, "Hx").status(), StatusCode::NOT_FOUND);
	assert_eq!(project_page(&database, "unknown").status(), StatusCode::NOT_FOUND);
	let search = search_query("é".repeat(100).as_str());
	assert_eq!(search_page(&database, search).status(), StatusCode::OK);
	let database = Arc::new(database);
	for uri in ["/?page=-1", "/?page=abc"] {
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
