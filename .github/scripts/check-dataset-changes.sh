#!/usr/bin/env bash
# Flag whether the generated dataset differs from the previous release.
set -euo pipefail

current_sha="$(sha256sum var/providers.jsonl | cut -d' ' -f1)"
changed=true
if [ "$FORCE" != 'true' ] && [ -n "$PREVIOUS_TAG" ]; then
	mkdir -p var/latest-release
	gh release download "$PREVIOUS_TAG" --pattern manifest.json --dir var/latest-release
	previous_sha="$(jq -r '.dataset.sha256' var/latest-release/manifest.json)"
	if ! [[ "$previous_sha" =~ ^[0-9a-f]{64}$ ]]; then
		printf 'Invalid dataset digest in %s\n' "$PREVIOUS_TAG" >&2
		exit 1
	fi
	if [ "$current_sha" = "$previous_sha" ]; then
		changed=false
	fi
fi

printf 'changed=%s\n' "$changed" >> "$GITHUB_OUTPUT"
printf 'Dataset SHA-256: `%s`\n' "$current_sha" >> "$GITHUB_STEP_SUMMARY"
if [ "$changed" = 'false' ]; then
	printf 'Dataset unchanged; skipping publication.\n' >> "$GITHUB_STEP_SUMMARY"
fi
