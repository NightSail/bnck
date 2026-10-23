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
    context="$snapshot/acquisition-context.tsv"
    {{ context }} reset "$context"
    {{ bnck_bulk }} plan-npm --curations curations \
        --output-directory "$snapshot/packages" --output "$snapshot/metadata.json"
    {{ context }} input "$context" "$snapshot/metadata.json" '' 'application/json' \
        "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" '' '' ''
    just resume-npm "$snapshot"
    if test -n "$selection"; then
        printf '%s\n' "$snapshot" > "$snapshot/current"
        mv -f "$snapshot/current" "$selection"
    fi
    {{ size }} "Pulled into $snapshot" "$snapshot"

# Resume only the retained plan, without selecting a different snapshot for default imports.
resume-npm snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot='{{ snapshot }}'
    test -f "$snapshot/metadata.json"
    context="$snapshot/acquisition-context.tsv"
    if test ! -f "$context"; then
        {{ context }} reset "$context"
        {{ context }} input "$context" "$snapshot/metadata.json" '' 'application/json' \
            "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" '' '' ''
    fi
    just fetch-registry-plan npm "$snapshot/metadata.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    while IFS=$'\t' read -r url path; do
        {{ context }} input "$context" "$path" "$url" 'application/json' "$retrieved_at" '' '' ''
        status="${path%.json}.status"
        if test -f "$status"; then
            {{ context }} input "$context" "$status" '' 'text/plain' "$retrieved_at" '' '' ''
        fi
    done < "$snapshot/metadata.tsv"
    touch "$snapshot/complete"

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
        --context "$snapshot/acquisition-context.tsv" \
        --scope 'registry=https://registry.npmjs.org' --scope 'release-policy=latest-dist-tag'
    {{ size }} 'Wrote {{ output }}' '{{ output }}'
