#!/usr/bin/env bash
# Resolve the CalVer dataset release identifier and its tag.
set -euo pipefail

previous_tag="$(
	git ls-remote --tags --refs origin 'refs/tags/bnck-dataset-*' |
		sed 's#.*refs/tags/##' |
		sort -V |
		tail -n 1
)"

if [ -n "${RELEASE_INPUT:-}" ]; then
	release="$RELEASE_INPUT"
	if ! [[ "$release" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}\.[1-9][0-9]*$ ]]; then
		printf 'Release identifier must use YYYY-MM-DD.N: %s\n' "$release" >&2
		exit 1
	fi
	date_part="${release%%.*}"
	normalized_date="$(date -u -d "$date_part" '+%F')"
	if [ "$normalized_date" != "$date_part" ]; then
		printf 'Release date is invalid: %s\n' "$date_part" >&2
		exit 1
	fi
	tag="bnck-dataset-$release"
	if git ls-remote --tags origin "refs/tags/$tag" | grep -q .; then
		printf 'Tag already exists: %s\n' "$tag" >&2
		exit 1
	fi
	force=true
else
	date_part="$(date -u '+%F')"
	sequence="$(
		git ls-remote --tags --refs origin "refs/tags/bnck-dataset-$date_part.*" |
			sed "s#.*refs/tags/bnck-dataset-$date_part\.##" |
			sort -n |
			tail -n 1
	)"
	last_sequence="${sequence:-0}"
	release="$date_part.$((last_sequence + 1))"
	tag="bnck-dataset-$release"
	force=false
fi

printf 'release=%s\n' "$release" >> "$GITHUB_OUTPUT"
printf 'tag=%s\n' "$tag" >> "$GITHUB_OUTPUT"
printf 'previous_tag=%s\n' "$previous_tag" >> "$GITHUB_OUTPUT"
printf 'force=%s\n' "$force" >> "$GITHUB_OUTPUT"
printf 'Planned dataset release `%s`\n' "$release" >> "$GITHUB_STEP_SUMMARY"
