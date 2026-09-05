use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::acquisition::{PackageObservation, SnapshotMetadata, write_acquisition};
use crate::model::Candidate;

pub fn import(
	formulae: &Path,
	casks: &Path,
	core: Option<&Path>,
	output: &Path,
	scope: std::collections::BTreeMap<String, String>,
) -> Result<usize> {
	let (mut candidates, mut package_observations) = import_formulae(formulae, core)?;
	let (cask_candidates, cask_packages) = import_casks(casks)?;
	candidates.extend(cask_candidates);
	package_observations.extend(cask_packages);
	let mut inputs = vec![formulae.to_owned(), casks.to_owned()];
	inputs.extend(core.map(Path::to_owned));
	write_acquisition(output, SnapshotMetadata::complete("homebrew", &inputs, scope), package_observations, candidates)
}

fn import_formulae(path: &Path, core: Option<&Path>) -> Result<(Vec<Candidate>, Vec<PackageObservation>)> {
	let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
	let formulae: Value = serde_json::from_slice(&data).with_context(|| format!("parse {}", path.display()))?;
	let formulae = formulae.as_array().context("formula response is not an array")?;
	let mut candidates = Vec::new();
	let mut packages = Vec::new();

	for formula in formulae {
		let Some(name) = formula.get("name").and_then(Value::as_str) else {
			continue;
		};
		let tap = formula.get("tap").and_then(Value::as_str).unwrap_or("homebrew/core");
		let package = format!("{tap}/{name}");
		let version = formula.pointer("/versions/stable").and_then(Value::as_str).unwrap_or_default();
		packages.push(PackageObservation::observed(&package, version));
		let mut executables = formula
			.get("executables")
			.and_then(Value::as_array)
			.into_iter()
			.flatten()
			.filter_map(Value::as_str)
			.map(str::to_owned)
			.collect::<Vec<_>>();
		let mut evidence = "declared-entry-point";

		if executables.is_empty()
			&& let (Some(core), Some(source)) = (core, formula.get("ruby_source_path").and_then(Value::as_str))
		{
			let source = core.join(source);
			if source.is_file() {
				executables = parse_formula_source(&fs::read_to_string(source)?);
				evidence = "static-recipe-action";
			}
		}

		for executable in executables {
			candidates.push(Candidate {
				channel: "homebrew".into(),
				package: package.clone(),
				version: version.to_owned(),
				variant: std::collections::BTreeMap::new(),
				artifact_sha256: None,
				executable,
				evidence: evidence.into(),
				source_package: None,
				homepage: string(formula, "homepage"),
				repository: formula_repository(formula),
				description: string(formula, "desc"),
			});
		}
	}
	Ok((candidates, packages))
}

fn import_casks(path: &Path) -> Result<(Vec<Candidate>, Vec<PackageObservation>)> {
	let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
	let casks: Value = serde_json::from_slice(&data).with_context(|| format!("parse {}", path.display()))?;
	let casks = casks.as_array().context("cask response is not an array")?;
	let mut candidates = Vec::new();
	let mut packages = Vec::new();

	for cask in casks {
		let Some(token) = cask.get("token").and_then(Value::as_str) else {
			continue;
		};
		let package = format!("homebrew/cask/{token}");
		let version = cask.get("version").and_then(Value::as_str).unwrap_or_default();
		packages.push(PackageObservation::observed(&package, version));
		for executable in cask_executables(cask) {
			candidates.push(Candidate {
				channel: "homebrew".into(),
				package: package.clone(),
				version: version.to_owned(),
				variant: std::collections::BTreeMap::new(),
				artifact_sha256: None,
				executable,
				evidence: "static-recipe-action".into(),
				source_package: None,
				homepage: string(cask, "homepage"),
				repository: None,
				description: string(cask, "desc"),
			});
		}
	}
	Ok((candidates, packages))
}

fn cask_executables(cask: &Value) -> Vec<String> {
	let mut executables = Vec::new();
	let Some(artifacts) = cask.get("artifacts").and_then(Value::as_array) else {
		return executables;
	};
	for artifact in artifacts {
		let Some(binary) = artifact.get("binary").and_then(Value::as_array) else {
			continue;
		};
		let path = artifact
			.get("target")
			.and_then(Value::as_str)
			.or_else(|| binary.first().and_then(Value::as_str));
		if let Some(name) = path.and_then(basename) {
			executables.push(name.to_owned());
		}
	}
	executables
}

fn formula_repository(formula: &Value) -> Option<String> {
	formula
		.pointer("/urls/head/url")
		.and_then(Value::as_str)
		.or_else(|| {
			formula
				.pointer("/urls/stable/url")
				.and_then(Value::as_str)
				.filter(|url| url.ends_with(".git"))
		})
		.map(str::to_owned)
}

fn parse_formula_source(source: &str) -> Vec<String> {
	let mut executables = Vec::new();
	for line in source.lines().map(str::trim) {
		let Some(arguments) = ["bin.install_symlink", "sbin.install_symlink", "bin.install", "sbin.install"]
			.into_iter()
			.find_map(|operation| line.strip_prefix(operation))
		else {
			continue;
		};
		let strings = ruby_strings(arguments);
		if arguments.contains("=>") {
			if let Some(name) = strings.last().and_then(|path| basename(path)) {
				executables.push(name.to_owned());
			}
		} else {
			executables.extend(strings.into_iter().filter_map(|path| basename(&path).map(str::to_owned)));
		}
	}
	executables
}

fn ruby_strings(value: &str) -> Vec<String> {
	let mut strings = Vec::new();
	let mut characters = value.char_indices().peekable();
	while let Some((_, character)) = characters.next() {
		if !matches!(character, '\'' | '"') {
			continue;
		}
		let quote = character;
		let mut string = String::new();
		let mut is_escaped = false;
		for (_, character) in characters.by_ref() {
			if is_escaped {
				string.push(character);
				is_escaped = false;
			} else if character == '\\' {
				is_escaped = true;
			} else if character == quote {
				strings.push(string);
				break;
			} else {
				string.push(character);
			}
		}
	}
	strings
}

fn basename(path: &str) -> Option<&str> {
	path.trim_end_matches('/').rsplit('/').next().filter(|name| !name.is_empty())
}

fn string(value: &Value, field: &str) -> Option<String> {
	value
		.get(field)
		.and_then(Value::as_str)
		.filter(|value| !value.is_empty())
		.map(str::to_owned)
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::{cask_executables, parse_formula_source};

	#[test]
	fn parses_direct_formula_install_actions() {
		let source =
			"bin.install \"target/release/rg\"\nbin.install \"foo\" => \"bar\"\nsystem \"make\", \"install\"\n";
		assert_eq!(parse_formula_source(source), ["rg", "bar"]);
	}

	#[test]
	fn reads_cask_binary_target() {
		let cask = json!({"artifacts": [{"binary": ["op"], "target": "$HOMEBREW_PREFIX/bin/op"}]});
		assert_eq!(cask_executables(&cask), ["op"]);
	}
}
