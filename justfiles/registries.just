pypi_wheel_cache := 'var/cache/pypi/wheels'

# Acquire fresh bulk catalogs, then metadata relevant to existing curated projects.
fetch-languages snapshot:
    mkdir -p "$(dirname '{{ snapshot }}')"
    mkdir '{{ snapshot }}'
    just fetch-crates-dump '{{ snapshot }}/crates'
    just fetch-npm-index '{{ snapshot }}/npm-index'
    just fetch-registry-metadata npm '{{ snapshot }}/npm-index' '{{ snapshot }}/npm'
    just fetch-pypi-index '{{ snapshot }}/pypi-index.json'
    just fetch-registry-metadata pypi '{{ snapshot }}/pypi-index.json' '{{ snapshot }}/pypi'
    just fetch-pypi-wheels '{{ snapshot }}/pypi'

import-languages snapshot:
    just import-crates-dump '{{ snapshot }}/crates'
    mkdir -p var/acquired
    {{ bnck_bulk }} import-npm --plan '{{ snapshot }}/npm/metadata.json' --output '{{ npm_output }}' \
    	--scope 'registry=https://registry.npmjs.org' --scope 'release-policy=latest-dist-tag' \
    	--scope 'catalog-policy=bounded-non-atomic-scan'
    {{ bnck_bulk }} import-pypi --plan '{{ snapshot }}/pypi/wheels.json' --output '{{ pypi_output }}' \
    	--scope 'index=https://pypi.org/simple/' --scope 'release-policy=latest-non-yanked-stable' \
    	--scope 'wheel-policy=prefer-universal-then-smallest' --scope 'catalog-policy=bounded-non-atomic-scan'

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

fetch-pypi-index output:
    test ! -e '{{ output }}'
    mkdir -p "$(dirname '{{ output }}')"
    curl --fail --location --compressed --retry 3 --connect-timeout 20 --max-time 180 \
    	--header 'Accept: application/vnd.pypi.simple.v1+json' --output '{{ output }}.tmp' 'https://pypi.org/simple/'
    mv '{{ output }}.tmp' '{{ output }}'

fetch-registry-metadata channel index snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    if test '{{ channel }}' = npm && test -d '{{ index }}'; then test -f '{{ index }}/complete'; fi
    mkdir -p "$(dirname '{{ snapshot }}')"
    mkdir '{{ snapshot }}'
    {{ bnck_bulk }} plan-registry --channel '{{ channel }}' --index '{{ index }}' --curations curations \
        --output-directory '{{ snapshot }}/packages' --output '{{ snapshot }}/metadata.json'
    just fetch-registry-plan '{{ channel }}' '{{ snapshot }}/metadata.tsv'

# Resume an existing plan without fetching completed responses again.
fetch-registry-plan channel downloads:
    #!/usr/bin/env bash
    set -euo pipefail
    case '{{ channel }}' in
        npm) header='Accept: application/json' ;;
        pypi) header='Accept: application/vnd.pypi.simple.v1+json' ;;
        *) printf 'Unknown registry channel\n' >&2; exit 1 ;;
    esac
    while IFS=$'\t' read -r url output; do
        if test -f "$output"; then continue; fi
        if ! status="$(curl --fail-with-body --location --compressed --retry 3 --connect-timeout 20 --max-time 180 \
            --proto '=https' --proto-redir '=https' --header "$header" \
            --write-out '%{http_code}' --output "$output.tmp" "$url")"; then
            case "$status" in
                404|410) printf 'Metadata unavailable: %s (HTTP %s)\n' "$url" "$status" >&2 ;;
                *) printf 'Metadata download failed: %s (HTTP %s)\n' "$url" "$status" >&2; exit 1 ;;
            esac
        fi
        printf '%s\n' "$status" > "${output%.json}.status"
        mv "$output.tmp" "$output"
        sleep 0.1
    done < '{{ downloads }}'

fetch-pypi-wheels snapshot:
    mkdir -p '{{ pypi_wheel_cache }}'
    {{ bnck_bulk }} plan-pypi-snapshot --metadata '{{ snapshot }}/metadata.json' \
    	--output-directory '{{ pypi_wheel_cache }}' --output '{{ snapshot }}/wheels.json'
    while IFS=$'\t' read -r url output; do \
    	if test ! -f "$output"; then \
    		curl --fail --location --retry 3 --connect-timeout 20 --max-time 600 \
    			--proto '=https' --proto-redir '=https' --output "$output.tmp" "$url"; \
    		mv "$output.tmp" "$output"; \
    	fi; \
    done < '{{ snapshot }}/wheels.tsv'
