mod curation;
mod importers;
mod matcher;
mod model;
mod release;

use std::path::PathBuf;

use anyhow::Result;
use anstyle::{AnsiColor, Style};
use clap::{Parser, Subcommand};

const IDENTIFIER_STYLE: Style = AnsiColor::Cyan.on_default();
const LABEL_STYLE: Style = AnsiColor::BrightBlue.on_default();
const NAME_STYLE: Style = Style::new().bold();

#[derive(Debug, Parser)]
#[command(name = "bnck")]
#[command(about = "Build the bnck executable-name dataset")]
struct Cli {
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
		crates: PathBuf,
		#[arg(long)]
		versions: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportCratesApi {
		#[arg(long)]
		input: PathBuf,
		#[arg(long)]
		output: PathBuf,
	},
	ImportNpm {
		#[arg(long)]
		input: PathBuf,
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
	ImportPypi {
		#[arg(long)]
		input: PathBuf,
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
		#[arg(long)]
		output: PathBuf,
		#[arg(long)]
		dataset_release: String,
		#[arg(long)]
		generated_at: String,
		#[arg(long)]
		generator_revision: String,
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
}

fn main() -> Result<()> {
	let cli = Cli::parse();
	let count = match cli.command {
		Command::ImportDebian { packages, sources, contents, output } => {
			importers::debian::import(&packages, &sources, &contents, &output)?
		},
		Command::PlanFedora { repomd, base_url, output_directory, output } => {
			importers::fedora::plan(&repomd, &base_url, &output_directory, &output)?
		},
		Command::ImportFedora { primary, filelists, output } => {
			importers::fedora::import(&primary, &filelists, &output)?
		},
		Command::ImportArch { input, output } => importers::arch::import(&input, &output)?,
		Command::ImportHomebrew { formulae, casks, core, output } => {
			importers::homebrew::import(&formulae, &casks, core.as_deref(), &output)?
		},
		Command::ImportCratesDump { crates, versions, output } => {
			importers::crates_io::import_dump(&crates, &versions, &output)?
		},
		Command::ImportCratesApi { input, output } => importers::crates_io::import_api(&input, &output)?,
		Command::ImportNpm { input, output } => importers::npm::import(&input, &output)?,
		Command::PlanPypi { simple, output_directory, output } => {
			importers::pypi::plan(&simple, &output_directory, &output)?
		},
		Command::ImportPypi { input, output } => importers::pypi::import(&input, &output)?,
		Command::Generate { acquisitions, curations, output } => {
			curation::generate(&acquisitions, &curations, &output)?
		},
		Command::GenerateSqlite { input, output, dataset_release, generated_at, generator_revision } => {
			let metadata = release::SqliteMetadata {
				dataset_release: &dataset_release,
				generated_at: &generated_at,
				generator_version: env!("CARGO_PKG_VERSION"),
				generator_revision: &generator_revision,
			};
			release::generate_sqlite_artifact(&input, &output, &metadata)?.provider_count
		},
		Command::SuggestPackages { name, homepage, repository, acquisitions, limit } => {
			let query = matcher::ProjectQuery { name, homepage, repository };
			let suggestions = matcher::suggest_packages(&acquisitions, &query, limit)?;
			print_package_suggestions(&suggestions);
			return Ok(());
		},
		Command::SuggestProjects { channel, package, acquisitions, curations, limit } => {
			let suggestions = matcher::suggest_projects(&acquisitions, &curations, &channel, &package, limit)?;
			print_project_suggestions(&suggestions);
			return Ok(());
		},
	};
	eprintln!("wrote {count} records");
	Ok(())
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
