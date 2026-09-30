#!/usr/bin/env bash
# Print a job's concluding data or artifact size, e.g. "Pulled into var/input/arch: 3.4MiB".
set -euo pipefail
label="$1"
path="$2"
if test ! -e "$path"; then
	printf 'Cannot measure missing path: %s\n' "$path" >&2
	exit 1
fi
bytes="$(du -sb -- "$path" | cut -f1)"
printf '\n%s: %s\n' "$label" "$(numfmt --to=iec-i --suffix=B "$bytes")"
