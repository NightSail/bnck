#!/usr/bin/env bash
# Append acquisition diagnostics to the GitHub Actions job summary.
set -euo pipefail

report="${1:-var/diagnostics/report.json}"
summary="${GITHUB_STEP_SUMMARY:?GITHUB_STEP_SUMMARY is not set}"
detail_limit=50

if [ ! -f "$report" ]; then
	printf '### Acquisition diagnostics\n\nNo diagnostics report was produced.\n\n' >> "$summary"
	exit 0
fi

format="$(jq -er '.format' "$report")"
test "$format" = 'bnck-acquisition-diagnostics' || {
	printf 'Unexpected diagnostics report format: %s\n' "$format" >&2
	exit 1
}
warning_count="$(jq -er '.warning_count' "$report")"
baseline_created="$(jq -r '.baseline_created' "$report")"

printf '### Acquisition diagnostics\n\n' >> "$summary"
if [ "$warning_count" -eq 0 ]; then
	printf 'No warnings.\n\n' >> "$summary"
else
	printf '%s warning(s).\n\n' "$warning_count" >> "$summary"
	printf '| Kind | Warnings |\n| --- | ---: |\n' >> "$summary"
	jq -r '
		.warnings
		| group_by(.kind)
		| map({ kind: .[0].kind, count: length })
		| sort_by(.kind)
		| .[]
		| "| `\(.kind)` | \(.count) |"
	' "$report" >> "$summary"
	printf '\n<details>\n<summary>Warning details</summary>\n\n' >> "$summary"
	jq -r --argjson limit "$detail_limit" '.warnings[:$limit][] | "- `\(.kind)`: \(.message)"' "$report" >> "$summary"
	hidden_count=$((warning_count - detail_limit))
	if [ "$hidden_count" -gt 0 ]; then
		printf '\n- %s more warning(s); see the job log for the full report.\n' "$hidden_count" >> "$summary"
	fi
	printf '\n</details>\n\n' >> "$summary"
fi
if [ "$baseline_created" = 'true' ]; then
	printf 'Baseline created; later runs compare acquisition changes against it.\n\n' >> "$summary"
fi
