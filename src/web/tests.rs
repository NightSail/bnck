use std::collections::BTreeSet;
use std::fs;
use std::sync::Arc;

use axum::body::to_bytes;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::Response;
use tempfile::tempdir;

use super::{Database, SEARCH_PAGE_SIZE, SearchQuery, about, executable_page, home, project_page, render_page, search_page};
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

const MATRIX_RECORDS: &[(&str, &str, &str, &str)] = &[
	("tools", "tool", "debian", "tools"),
	("tools", "helper", "debian", "tools"),
	("tools", "tool", "debian", "tools-extra"),
	("tools", "helper", "arch", "tools"),
	("tools", "Tool", "arch", "tools"),
	("tools", "tool", "npm", "@scope/tools"),
	("tools", "helper", "homebrew", "custom/tap/tools"),
];

const MATRIX_COVERAGE: &[(&str, [bool; 5])] = &[
	("Tool", [true, false, false, false, false]),
	("helper", [true, true, false, true, false]),
	("tool", [false, true, true, false, true]),
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
	SearchQuery { q: Some(query.to_owned()), channel: String::new(), page: 1 }
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
	let channel_text = row.split_once("(1)</code></a>: ").unwrap().1;
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
				.map(|package| (channel.channel.as_str(), package.name.as_str()))
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
	assert_eq!(matrix.executables.len(), MATRIX_COVERAGE.len());
	for (row, (name, observed)) in matrix.executables.iter().zip(MATRIX_COVERAGE) {
		assert_eq!(row.executable, *name);
		assert_eq!(row.observed, observed);
	}
}

#[tokio::test]
async fn project_matrix_renders_grouped_headers_links_and_exact_coverage() {
	let database = fixture(MATRIX_RECORDS.iter().copied().map(provider));
	let html = response_text(project_page(&database, "tools")).await;
	assert_eq!(html.matches("<table ").count(), 1);
	assert_eq!(html.matches("scope='colgroup'").count(), 4);
	assert_eq!(html.matches("class='matrix-package' scope='col'").count(), 5);
	let normalized = html.split_whitespace().collect::<Vec<_>>().join(" ");
	assert!(
		normalized.contains(concat!(
			"<th scope='colgroup' colspan='2'> ",
			"<span class='matrix-channel'>Debian</span> </th>"
		))
	);
	assert!(html.contains("<colgroup span='2'></colgroup>"));
	assert!(html.contains("scope='col' rowspan='2'>Executable</th>"));
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
	for (name, observed) in MATRIX_COVERAGE {
		let anchor = format!("<tr id='executable-{name}'>");
		assert_eq!(html.matches(&anchor).count(), 1);
		let row = html.split_once(&anchor).unwrap().1.split_once("</tr>").unwrap().0;
		assert!(row.contains("scope='row'"));
		assert!(row.contains(&format!("href='/executables/{name}'")));
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
	assert_eq!(html.matches("class='matrix-package'").count(), 1);
	assert_eq!(html.matches("scope='row'").count(), 4);
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
	assert_eq!(html.matches("<code>eslint</code>").count(), 1);
	assert_eq!(html.matches("<code>eslint(1)</code>").count(), 1);
	assert!(!html.contains("Project:"));
	assert!(!html.contains("class='result-summary'"));
	assert!(!html.contains("3 channels"));
	assert!(!html.contains("4 provider records"));
	assert_executable_row(&html, "eslint", "eslint", "Debian, Homebrew, npm");
	assert_eq!(html.matches("href='/projects/eslint'").count(), 1);
	let normalized = html.split_whitespace().collect::<Vec<_>>().join(" ");
	assert!(normalized.contains("<h3 id='result-1'> <a href='/projects/eslint'> <code>eslint</code></a> </h3>"));
	assert!(!html.contains("Project details"));
	assert!(html.contains("1 matching project"));
	assert!(!html.contains("class='results-table'"));
	assert!(!html.contains("name='kind'"));
	assert!(!html.contains("class='result-type'"));
	assert!(!html.contains("Provided by"));
	assert!(!html.contains("Provides"));
	assert!(!html.contains("class='exact-badge'"));
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
		assert_eq!(html.contains("<details class='more-related'>"), count > 3);
		if count > 3 {
			let (preview, expanded) = html.split_once("<details class='more-related'>").unwrap();
			assert_eq!(preview.matches("class='related-name'").count(), 3);
			assert!(expanded.contains("1 more executable"));
			assert_executable_row(expanded, "tools", "command-3", "npm");
			let html = response_text(search_page(&database, search_query("command-3"))).await;
			let preview = html.split_once("<details class='more-related'>").unwrap().0;
			assert_executable_row(preview, "tools", "command-3", "npm");
			assert!(preview.contains("<code>command-3(1)</code>"));
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
