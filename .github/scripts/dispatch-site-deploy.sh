#!/usr/bin/env bash
# Dispatch the Site workflow for a newly published dataset release.
set -euo pipefail

tag="${TAG:?dataset release tag is required}"
ref="${REF:?workflow ref is required}"

gh workflow run site.yml \
	--repo "$GITHUB_REPOSITORY" \
	--ref "$ref" \
	--field "dataset_tag=$tag"
printf 'Dispatched site deployment for dataset release `%s`\n' "$tag" >> "$GITHUB_STEP_SUMMARY"
