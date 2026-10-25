#!/usr/bin/env bash
# Resolve the dataset release tag used to build the site.
set -euo pipefail

tag="${DATASET_TAG:-}"
if [ -z "$tag" ]; then
	tag="$(
		git ls-remote --tags --refs origin 'refs/tags/bnck-dataset-*' |
			sed 's#.*refs/tags/##' |
			sort -V |
			tail -n 1
	)"
fi
if ! [[ "$tag" =~ ^bnck-dataset-[0-9]{4}-[0-9]{2}-[0-9]{2}\.[1-9][0-9]*$ ]]; then
	printf 'Invalid dataset release tag: %s\n' "$tag" >&2
	exit 1
fi

printf 'tag=%s\n' "$tag" >> "$GITHUB_OUTPUT"
printf 'Building the site from dataset release `%s`\n' "$tag" >> "$GITHUB_STEP_SUMMARY"
