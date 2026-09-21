#!/usr/bin/env bash
set -euo pipefail

{
	printf '%s' "${0##*/}"
	printf '\t%s' "$@"
	printf '\n'
} >> "$BNCK_TEST_LOG"

if test "$1" = "${BNCK_TEST_FAIL_JOB:-}"; then
	printf 'Simulated failure: %s\n' "$1" >&2
	exit 1
fi

case "$1" in
	fetch-crates-dump)
		mkdir -p "$2"
		;;
	fetch-npm)
		mkdir -p "$2"
		printf '{}\n' > "$2/metadata.json"
		touch "$2/complete"
		;;
	fetch-pypi-index)
		printf '{}\n' > "$2"
		;;
	fetch-registry-metadata)
		mkdir -p "$4"
		printf '{}\n' > "$4/metadata.json"
		;;
	fetch-pypi-wheels)
		printf '{}\n' > "$2/wheels.json"
		;;
	import-crates-dump)
		if test -n "${BNCK_TEST_RESELECT:-}"; then
			printf '%s\n' "$BNCK_TEST_RESELECT" > "$BNCK_TEST_CURRENT"
		fi
		;;
	import-npm|import-pypi)
		;;
	*)
		printf 'Unexpected test command: %s\n' "$1" >&2
		exit 1
		;;
esac
