mod curation;
mod importers;
mod model;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
	};
	eprintln!("wrote {count} records");
	Ok(())
}
