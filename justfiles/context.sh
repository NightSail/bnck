#!/usr/bin/env bash
# Write acquisition context records consumed by bnck import commands.
set -euo pipefail

command="${1:?context command is required}"
file="${2:?context file is required}"
shift 2

case "$command" in
	reset)
		test $# -eq 0
		printf 'bnck-acquisition-context\t1\n' > "$file"
		;;
	input)
		test $# -eq 7
		# Replace an existing record for the same path while preserving the rest.
		temporary="$(mktemp "${file}.XXXXXX")"
		if test -f "$file"; then
			awk -F '\t' -v path="$1" '!($1 == "input" && $2 == path)' "$file" > "$temporary"
		fi
		mv "$temporary" "$file"
		printf 'input\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" "$6" "$7" >> "$file"
		;;
	boundary)
		test $# -eq 2
		temporary="$(mktemp "${file}.XXXXXX")"
		if test -f "$file"; then
			awk -F '\t' -v key="$1" '!($1 == "boundary" && $2 == key)' "$file" > "$temporary"
		fi
		mv "$temporary" "$file"
		printf 'boundary\t%s\t%s\n' "$1" "$2" >> "$file"
		;;
	*)
		printf 'unknown context command: %s\n' "$command" >&2
		exit 2
		;;
esac
