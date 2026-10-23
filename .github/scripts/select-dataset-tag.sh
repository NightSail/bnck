#!/usr/bin/env bash
# Select the dataset release tag the site should be built from.
set -euo pipefail

if [ "$CHANGED" = 'true' ]; then
	tag="$NEW_TAG"
else
	tag="$PREVIOUS_TAG"
fi
test -n "$tag"
printf 'tag=%s\n' "$tag" >> "$GITHUB_OUTPUT"
