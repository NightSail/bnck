mod acquisition;
mod curation;
mod importers;
mod matcher;
mod model;
mod registry;
mod release;
mod web;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Result, ensure};
use anstyle::{AnsiColor, Style};
use clap::{Parser, Subcommand};

const IDENTIFIER_STYLE: Style = AnsiColor::Cyan.on_default();
const LABEL_STYLE: Style = AnsiColor::BrightBlue.on_default();
const NAME_STYLE: Style = Style::new().bold();

#[derive(Debug, Parser)]
#[command(name = "bnck")]
#[command(about = "Build the bnck executable-name dataset")]
struct Cli {
	#[arg(long = "scope", global = true, value_parser = parse_scope)]
	scope: Vec<(String, String)>,
	#[command(subcommand)]
	command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
	ImportDebian {
		#[arg(long)]
		packages: PathBuf,
		#[arg(long)]
		sources: PathBuf,
		#[arg(long)]
		contents: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	PlanFedora {
		#[arg(long)]
		repomd: PathBuf,
		#[arg(long)]
		base_url: String,
		#[arg(long)]
		output_directory: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportFedora {
		#[arg(long)]
		primary: PathBuf,
		#[arg(long)]
		filelists: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportArch {
		#[arg(long)]
		input: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportHomebrew {
		#[arg(long)]
		formulae: PathBuf,
		#[arg(long)]
		casks: PathBuf,
		#[arg(long)]
		core: Option<PathBuf>,
		#[arg(long)]
		output: PathBuf,
	},
	ImportCratesDump {
		#[arg(long)]
		curations: Option<PathBuf>,
		#[arg(long)]
		crates: PathBuf,
		#[arg(long)]
		versions: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	PlanNpmPage {
		#[arg(long)]
		input: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	PlanRegistry {
		#[arg(long)]
		channel: String,
		#[arg(long)]
		index: PathBuf,
		#[arg(long, default_value = "curations")]
		curations: PathBuf,
		#[arg(long)]
		output_directory: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportNpm {
		#[arg(long, required_unless_present = "plan", conflicts_with = "plan")]
		input: Option<PathBuf>,
		#[arg(long)]
		plan: Option<PathBuf>,
		#[arg(long)]
		output: PathBuf,
	},
	PlanPypi {
		#[arg(long)]
		simple: PathBuf,
		#[arg(long)]
		output_directory: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	PlanPypiSnapshot {
		#[arg(long)]
		metadata: PathBuf,
		#[arg(long)]
		output_directory: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportPypi {
		#[arg(long, required_unless_present = "plan", conflicts_with = "plan")]
		input: Option<PathBuf>,
		#[arg(long)]
		plan: Option<PathBuf>,
		#[arg(long)]
		output: PathBuf,
	},
	Generate {
		#[arg(long)]
		acquisitions: PathBuf,
		#[arg(long)]
		curations: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	GenerateSqlite {
		#[arg(long)]
		input: PathBuf,
		#[arg(long, default_value = "curations")]
		curations: PathBuf,
		#[arg(long)]
		output: PathBuf,
		#[arg(long)]
		dataset_release: String,
		#[arg(long)]
		generated_at: String,
		#[arg(long)]
		generator_revision: String,
	},
	AssembleRelease {
		#[arg(long)]
		input: PathBuf,
		#[arg(long)]
		acquisitions: PathBuf,
		#[arg(long, default_value = "curations")]
		curations: PathBuf,
		#[arg(long)]
		output: PathBuf,
		#[arg(long)]
		dataset_release: String,
		#[arg(long)]
		generated_at: String,
		#[arg(long)]
		generator_revision: String,
	},
	SuggestRegistryPackages {
		#[arg(long, default_value = "var/acquired")]
		acquisitions: PathBuf,
		#[arg(long, default_value = "curations")]
		curations: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	SuggestPackages {
		#[arg(long)]
		name: String,
		#[arg(long)]
		homepage: Option<String>,
		#[arg(long)]
		repository: Option<String>,
		#[arg(long, default_value = "var/acquired")]
		acquisitions: PathBuf,
		#[arg(long, default_value_t = 20)]
		limit: usize,
	},
	SuggestProjects {
		#[arg(long)]
		channel: String,
		#[arg(long)]
		package: String,
		#[arg(long, default_value = "var/acquired")]
		acquisitions: PathBuf,
		#[arg(long, default_value = "curations")]
		curations: PathBuf,
		#[arg(long, default_value_t = 20)]
		limit: usize,
	},
	GenerateSite {
		#[arg(long, default_value = "var/bnck.sqlite.zst")]
		database: PathBuf,
		#[arg(long, default_value = "var/site")]
		output: PathBuf,
	},
	Serve {
		#[arg(long, default_value = "var/bnck.sqlite.zst")]
		database: PathBuf,
		#[arg(long, default_value = "127.0.0.1:3000")]
		address: SocketAddr,
	},
}

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	let scope = scope_map(cli.scope)?;
	let count = match cli.command {
		Command::ImportDebian { packages, sources, contents, output } => {
			importers::debian::import(&packages, &sources, &contents, &output, scope)?
		},
		Command::PlanFedora { repomd, base_url, output_directory, output } => {
			importers::fedora::plan(&repomd, &base_url, &output_directory, &output)?
		},
		Command::ImportFedora { primary, filelists, output } => {
			importers::fedora::import(&primary, &filelists, &output, scope)?
		},
		Command::ImportArch { input, output } => importers::arch::import(&input, &output, scope)?,
		Command::ImportHomebrew { formulae, casks, core, output } => {
			importers::homebrew::import(&formulae, &casks, core.as_deref(), &output, scope)?
		},
		Command::ImportCratesDump { crates, versions, output, curations } => {
			importers::crates_io::import_dump(&crates, &versions, &output, scope, curations.as_deref())?
		},
		Command::PlanNpmPage { input, output } => registry::plan_npm_page(&input, &output)?,
		Command::PlanRegistry { channel, index, curations, output_directory, output } => {
			registry::plan_metadata(&channel, &index, &curations, &output_directory, &output)?
		},
		Command::ImportNpm { input, plan, output } => match plan {
			Some(plan) => importers::npm::import_plan(&plan, &output, scope)?,
			None => importers::npm::import(&input.expect("clap requires input or plan"), &output, scope)?,
		},
		Command::PlanPypi { simple, output_directory, output } => {
			importers::pypi::plan(&simple, &output_directory, &output)?
		},
		Command::PlanPypiSnapshot { metadata, output_directory, output } => {
			importers::pypi::plan_snapshot(&metadata, &output_directory, &output)?
		},
		Command::ImportPypi { input, plan, output } => match plan {
			Some(plan) => importers::pypi::import_plan(&plan, &output, scope)?,
			None => importers::pypi::import(&input.expect("clap requires input or plan"), &output, scope)?,
		},
		Command::Generate { acquisitions, curations, output } => {
			curation::generate(&acquisitions, &curations, &output)?
		},
		Command::GenerateSqlite { input, curations, output, dataset_release, generated_at, generator_revision } => {
			let metadata = release::SqliteMetadata {
				dataset_release: &dataset_release,
				generated_at: &generated_at,
				generator_version: env!("CARGO_PKG_VERSION"),
				generator_revision: &generator_revision,
			};
			release::generate_sqlite_artifact(&input, &curations, &output, &metadata)?.provider_count
		},
		Command::AssembleRelease {
			input,
			acquisitions,
			curations,
			output,
			dataset_release,
			generated_at,
			generator_revision,
		} => {
			let metadata = release::ReleaseMetadata {
				release: &dataset_release,
				generated_at: &generated_at,
				generator_revision: &generator_revision,
			};
			release::assemble_release(&input, &acquisitions, &curations, &output, &metadata)?.provider_count
		},
		Command::SuggestRegistryPackages { acquisitions, curations, output } => {
			let suggestions = matcher::suggest_registry_packages(&acquisitions, &curations)?;
			registry::write_json(&output, &suggestions)?;
			suggestions.len()
		},
		Command::SuggestPackages { name, homepage, repository, acquisitions, limit } => {
			let query = matcher::ProjectQuery { name, homepage, repository, links: Vec::new() };
			let suggestions = matcher::suggest_packages(&acquisitions, &query, limit)?;
			print_package_suggestions(&suggestions);
			return Ok(());
		},
		Command::SuggestProjects { channel, package, acquisitions, curations, limit } => {
			let suggestions = matcher::suggest_projects(&acquisitions, &curations, &channel, &package, limit)?;
			print_project_suggestions(&suggestions);
			return Ok(());
		},
		Command::GenerateSite { database, output } => {
			web::generate_site(&database, &output)?;
			eprintln!("wrote static website to {}", output.display());
			return Ok(());
		},
		Command::Serve { database, address } => {
			web::serve(&database, address).await?;
			return Ok(());
		},
	};
	eprintln!("wrote {count} records");
	Ok(())
}

fn parse_scope(value: &str) -> Result<(String, String), String> {
	let (key, value) = value.split_once('=').ok_or_else(|| "scope must use key=value".to_owned())?;
	if key.is_empty() || value.is_empty() {
		return Err("scope key and value must not be empty".to_owned());
	}
	Ok((key.to_owned(), value.to_owned()))
}

fn scope_map(values: Vec<(String, String)>) -> Result<BTreeMap<String, String>> {
	let mut scope = BTreeMap::new();
	for (key, value) in values {
		ensure!(scope.insert(key.clone(), value).is_none(), "duplicate scope key {key}");
	}
	Ok(scope)
}

fn print_package_suggestions(suggestions: &[matcher::PackageSuggestion]) {
	let widths = suggestion_widths(
		suggestions
			.iter()
			.map(|suggestion| (suggestion.channel.as_str(), suggestion.package.as_str())),
	);
	for suggestion in suggestions {
		print_suggestion_heading(suggestion.confidence, &suggestion.channel, &suggestion.package, widths);
		print_suggestion_field("evidence", &suggestion.evidence);
		print_suggestion_field("executables", &suggestion.executables);
		if !suggestion.homepages.is_empty() {
			print_suggestion_field("homepage", &suggestion.homepages);
		}
		if !suggestion.repositories.is_empty() {
			print_suggestion_field("repository", &suggestion.repositories);
		}
	}
}

fn print_project_suggestions(suggestions: &[matcher::ProjectSuggestion]) {
	let widths = suggestion_widths(
		suggestions
			.iter()
			.map(|suggestion| (suggestion.id.as_deref().unwrap_or("[uncurated]"), suggestion.name.as_str())),
	);
	for suggestion in suggestions {
		let id = suggestion.id.as_deref().unwrap_or("[uncurated]");
		print_suggestion_heading(suggestion.confidence, id, &suggestion.name, widths);
		print_suggestion_field("evidence", &suggestion.evidence);
		if !suggestion.homepages.is_empty() {
			print_suggestion_field("homepage", &suggestion.homepages);
		}
		if !suggestion.repositories.is_empty() {
			print_suggestion_field("repository", &suggestion.repositories);
		}
	}
}

#[derive(Clone, Copy, Default)]
struct SuggestionWidths {
	identifier: usize,
	name: usize,
}

fn suggestion_widths<'a>(headings: impl Iterator<Item = (&'a str, &'a str)>) -> SuggestionWidths {
	headings.fold(SuggestionWidths::default(), |mut widths, (identifier, name)| {
		widths.identifier = widths.identifier.max(identifier.chars().count());
		widths.name = widths.name.max(name.chars().count());
		widths
	})
}

fn print_suggestion_heading(confidence: matcher::Confidence, identifier: &str, name: &str, widths: SuggestionWidths) {
	let confidence_style = confidence_style(confidence);
	let identifier_width = widths.identifier;
	let name_width = widths.name;
	anstream::println!(
		"{IDENTIFIER_STYLE}{identifier:<identifier_width$}{IDENTIFIER_STYLE:#} \
		 {NAME_STYLE}{name:<name_width$}{NAME_STYLE:#} {confidence_style}{}{confidence_style:#}",
		confidence.as_str(),
	);
}

fn print_suggestion_field(label: &str, values: &[String]) {
	anstream::println!("    {LABEL_STYLE}{label}:{LABEL_STYLE:#} {}", values.join(", "));
}

fn confidence_style(confidence: matcher::Confidence) -> Style {
	match confidence {
		matcher::Confidence::Exact => AnsiColor::BrightGreen.on_default().bold(),
		matcher::Confidence::Strong => AnsiColor::Green.on_default().bold(),
		matcher::Confidence::Possible => AnsiColor::Yellow.on_default().bold(),
		matcher::Confidence::Weak => AnsiColor::BrightBlack.on_default().bold(),
	}
}
