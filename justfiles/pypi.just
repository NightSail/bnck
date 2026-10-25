pypi_input := 'var/input/pypi'
pypi_output := 'var/acquired/pypi.jsonl'
pypi_wheel_cache := 'var/cache/pypi/wheels'

fetch-pypi package:
    #!/usr/bin/env bash
    set -euo pipefail
    package='{{ package }}'
    input='{{ pypi_input }}'
    context="$input/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    mkdir -p "$input/simple" "$input/plans" "$input/wheels"
    {{ context }} reset "$context"
    curl --fail --location --retry 3 \
        --user-agent '{{ bnck_user_agent }}' \
        --header 'Accept: application/vnd.pypi.simple.v1+json' \
        --output "$input/simple/$package.json" \
        "https://pypi.org/simple/$package/"
    {{ context }} input "$context" "$input/simple/$package.json" \
        "https://pypi.org/simple/$package/" 'application/vnd.pypi.simple.v1+json' "$retrieved_at" '' '' ''
    {{ bnck }} plan-pypi \
        --simple "$input/simple/$package.json" \
        --output-directory "$input/wheels" \
        --output "$input/plans/$package.tsv"
    {{ context }} input "$context" "$input/plans/$package.tsv" '' 'text/tab-separated-values' \
        "$retrieved_at" '' '' ''
    while IFS=$'\t' read -r url output; do
        curl --fail --location --retry 3 --user-agent '{{ bnck_user_agent }}' --continue-at - --output "$output" "$url"
        {{ context }} input "$context" "$output" "$url" 'application/zip' "$retrieved_at" '' '' ''
    done < "$input/plans/$package.tsv"
    {{ size }} 'Pulled into {{ pypi_input }}' '{{ pypi_input }}'

import-pypi:
    mkdir -p 'var/acquired'
    {{ bnck }} import-pypi \
    	--input '{{ pypi_input }}/wheels' \
    	--output '{{ pypi_output }}' --context '{{ pypi_input }}/acquisition-context.tsv' \
    	--scope 'index=https://pypi.org/simple/' \
    	--scope 'release-policy=latest-listed-wheel'
    @{{ size }} 'Wrote {{ pypi_output }}' '{{ pypi_output }}'

# Plan and fetch only explicitly curated PyPI packages, without the global project index.
fetch-pypi-metadata snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot='{{ snapshot }}'
    context="$snapshot/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    mkdir -p "$(dirname "$snapshot")"
    mkdir "$snapshot"
    {{ context }} reset "$context"
    {{ bnck_bulk }} plan-pypi-metadata --curations curations \
        --output-directory "$snapshot/packages" --output "$snapshot/metadata.json"
    {{ context }} input "$context" "$snapshot/metadata.json" '' 'application/json' "$retrieved_at" '' '' ''
    just fetch-registry-plan pypi "$snapshot/metadata.tsv"
    while IFS=$'\t' read -r url path; do
        {{ context }} input "$context" "$path" "$url" 'application/vnd.pypi.simple.v1+json' "$retrieved_at" '' '' ''
        status="${path%.json}.status"
        if test -f "$status"; then
            {{ context }} input "$context" "$status" '' 'text/plain' "$retrieved_at" '' '' ''
        fi
    done < "$snapshot/metadata.tsv"
    {{ size }} 'Pulled into {{ snapshot }}' '{{ snapshot }}'

fetch-pypi-wheels snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    snapshot='{{ snapshot }}'
    context="$snapshot/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    if test ! -f "$context"; then
        {{ context }} reset "$context"
    fi
    mkdir -p '{{ pypi_wheel_cache }}'
    {{ bnck_bulk }} plan-pypi-snapshot --metadata "$snapshot/metadata.json" \
        --output-directory '{{ pypi_wheel_cache }}' --output "$snapshot/wheels.json"
    {{ context }} input "$context" "$snapshot/wheels.json" '' 'application/json' "$retrieved_at" '' '' ''
    while IFS=$'\t' read -r url output; do
        if test ! -f "$output"; then
            curl --fail --location --retry 3 --connect-timeout 20 --max-time 600 \
                --user-agent '{{ bnck_user_agent }}' --proto '=https' --proto-redir '=https' --output "$output.tmp" "$url"
            mv "$output.tmp" "$output"
        fi
        {{ context }} input "$context" "$output" "$url" 'application/zip' "$retrieved_at" '' '' ''
    done < "$snapshot/wheels.tsv"
    {{ size }} 'Pulled into {{ pypi_wheel_cache }}' '{{ pypi_wheel_cache }}'
