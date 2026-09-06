npm_input := 'var/input/npm'
npm_output := 'var/acquired/npm.jsonl'

fetch-npm package:
    mkdir -p "$(dirname '{{ npm_input }}/{{ package }}.json')"
    curl --fail --location --retry 3 \
    	--output '{{ npm_input }}/{{ package }}.json' \
    	'https://registry.npmjs.org/{{ package }}'

import-npm:
    mkdir -p 'var/acquired'
    {{ bnck }} import-npm \
    	--input '{{ npm_input }}' \
    	--output '{{ npm_output }}' \
    	--scope 'registry=https://registry.npmjs.org' \
    	--scope 'release-policy=latest-dist-tag'

# Inclusive keyset pagination retains page responses without mirroring npm tarballs.
fetch-npm-index directory:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "$(dirname '{{ directory }}')"
    mkdir '{{ directory }}'
    url='https://replicate.npmjs.com/_all_docs?limit=10000'
    page=0
    while true; do
        output="$(printf '{{ directory }}/%06d.json' "$page")"
        curl --fail --location --compressed --retry 3 --connect-timeout 20 --max-time 180 \
            --output "$output.tmp" "$url"
        mv "$output.tmp" "$output"
        {{ bnck_bulk }} plan-npm-page --input "$output" --output '{{ directory }}/next-url'
        if ! IFS= read -r url < '{{ directory }}/next-url'; then break; fi
        page=$((page + 1))
        sleep 0.1
    done
    touch '{{ directory }}/complete'
