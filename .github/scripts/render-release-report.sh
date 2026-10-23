#!/usr/bin/env bash
# Render the GitHub Release report for a dataset release from its manifest.
set -euo pipefail

manifest="${1:?usage: render-release-report.sh <manifest.json>}"
test -f "$manifest" || {
	printf 'Manifest not found: %s\n' "$manifest" >&2
	exit 1
}

release="$(jq -er '.release' "$manifest")"
generated_at="$(jq -er '.generated_at' "$manifest")"
provider_count="$(jq -er '.dataset.provider_count' "$manifest")"
association_count="$(jq -er '.dataset.name_associations.association_count // 0' "$manifest")"

printf 'Dataset release `%s`, generated at `%s`.\n\n' "$release" "$generated_at"
printf '| Dataset | Records |\n'
printf '| --- | ---: |\n'
printf '| Provider relations | %s |\n' "$provider_count"
printf '| Name associations | %s |\n\n' "$association_count"
printf '### Channels\n\n'
printf '| Channel | Status | Observed packages | Provider records |\n'
printf '| --- | --- | ---: | ---: |\n'
jq -r '
	.channels
	| group_by(.name)
	| map({
		name: .[0].name,
		status: ([.[].status] | unique | join(", ")),
		package_count: (map(.package_count) | add),
		provider_count: (map(.provider_count) | add)
	})
	| .[]
	| "| \(.name) | \(.status) | \(.package_count) | \(.provider_count) |"
' "$manifest"
