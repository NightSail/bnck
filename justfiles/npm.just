npm_input := 'var/input/npm'
npm_output := 'var/acquired/npm.jsonl'

# Fetch every explicitly curated npm package; an explicit snapshot does not change the default selection.
fetch-npm snapshot='':
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot='{{ snapshot }}'
    selection=''
    if test -z "$snapshot"; then
        mkdir -p '{{ npm_input }}'
        inputs="$(cd '{{ npm_input }}' && pwd -P)"
        selection="$inputs/current"
        if test -d "$selection"; then
            printf 'npm input selection must be a file: %s\n' "$selection" >&2
            exit 1
        fi
        snapshot="$(mktemp -d "$inputs/$(date -u '+%Y-%m-%dT%H%M%SZ').XXXXXX")"
    else
        mkdir -p "$(dirname "$snapshot")"
        mkdir "$snapshot"
        snapshot="$(cd "$snapshot" && pwd -P)"
    fi
    printf 'Fetching npm release metadata into %s\n' "$snapshot"
    {{ bnck_bulk }} plan-npm --curations curations \
        --output-directory "$snapshot/packages" --output "$snapshot/metadata.json"
    just resume-npm "$snapshot"
    if test -n "$selection"; then
        printf '%s\n' "$snapshot" > "$snapshot/current"
        mv -f "$snapshot/current" "$selection"
    fi
    {{ size }} "Pulled into $snapshot" "$snapshot"

# Resume only the retained plan, without selecting a different snapshot for default imports.
resume-npm snapshot:
    test -f '{{ snapshot }}/metadata.json'
    just fetch-registry-plan npm '{{ snapshot }}/metadata.tsv'
    touch '{{ snapshot }}/complete'

# Import the last completed standalone fetch, or the exact snapshot supplied by the caller.
import-npm snapshot='' output=npm_output:
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot='{{ snapshot }}'
    if test -z "$snapshot"; then
        selection='{{ npm_input }}/current'
        if test ! -f "$selection"; then
            printf 'No completed npm fetch. Run just fetch-npm first.\n' >&2
            exit 1
        fi
        if ! IFS= read -r snapshot < "$selection" || test ! -d "$snapshot"; then
            printf 'Invalid npm input selection: %s\n' "$selection" >&2
            exit 1
        fi
    fi
    if test ! -f "$snapshot/complete"; then
        printf 'Incomplete npm snapshot: %s; run just resume-npm with this path.\n' "$snapshot" >&2
        exit 1
    fi
    mkdir -p "$(dirname '{{ output }}')"
    {{ bnck_bulk }} import-npm --plan "$snapshot/metadata.json" --output '{{ output }}' \
        --scope 'registry=https://registry.npmjs.org' --scope 'release-policy=latest-dist-tag'
    {{ size }} 'Wrote {{ output }}' '{{ output }}'
