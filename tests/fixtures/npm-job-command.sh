#!/usr/bin/env bash
set -euo pipefail

{
	printf '%s' "${0##*/}"
	printf '\t%s' "$@"
	printf '\n'
} >> "$BNCK_TEST_LOG"

case "${0##*/}" in
	sleep) exit 0 ;;
	bnck-test)
		case "$1" in
			plan-npm)
				if test -n "${BNCK_TEST_PLAN_FAILURE:-}"; then exit 1; fi
				shift
				while test "$#" -gt 0; do
					case "$1" in
						--output-directory) directory="$2" ;;
						--output) plan="$2" ;;
						--curations) ;;
						*) exit 1 ;;
					esac
					shift 2
				done
				mkdir -p "$directory"
				printf '{}\n' > "$plan"
				{
					printf 'https://registry.npmjs.org/one/latest\t%s/one.json\n' "$directory"
					printf 'https://registry.npmjs.org/%%40scope%%2Ftwo/latest\t%s/two.json\n' "$directory"
				} > "${plan%.json}.tsv"
				;;
			import-npm) ;;
			*) exit 1 ;;
		esac
		;;
	curl)
		url="${!#}"
		while test "$#" -gt 0; do
			if test "$1" = --output; then output="$2"; fi
			shift
		done
		status=200
		if test "${output##*/}" = "${BNCK_TEST_RESPONSE_FILE:-}"; then status="$BNCK_TEST_RESPONSE_STATUS"; fi
		printf '{"response":"%s"}\n' "$status" > "$output"
		printf '%s' "$status"
		if test "$status" -ge 400; then exit 22; fi
		;;
	*) exit 1 ;;
esac
