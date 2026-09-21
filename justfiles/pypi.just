pypi_input := 'var/input/pypi'
pypi_output := 'var/acquired/pypi.jsonl'
pypi_wheel_cache := 'var/cache/pypi/wheels'

fetch-pypi package:
    mkdir -p '{{ pypi_input }}/simple' '{{ pypi_input }}/plans' '{{ pypi_input }}/wheels'
    curl --fail --location --retry 3 \
    	--header 'Accept: application/vnd.pypi.simple.v1+json' \
    	--output '{{ pypi_input }}/simple/{{ package }}.json' \
    	'https://pypi.org/simple/{{ package }}/'
    {{ bnck }} plan-pypi \
    	--simple '{{ pypi_input }}/simple/{{ package }}.json' \
    	--output-directory '{{ pypi_input }}/wheels' \
    	--output '{{ pypi_input }}/plans/{{ package }}.tsv'
    while IFS=$'\t' read -r url output; do \
    	curl --fail --location --retry 3 --continue-at - --output "$output" "$url"; \
    done < '{{ pypi_input }}/plans/{{ package }}.tsv'

import-pypi:
    mkdir -p 'var/acquired'
    {{ bnck }} import-pypi \
    	--input '{{ pypi_input }}/wheels' \
    	--output '{{ pypi_output }}' \
    	--scope 'index=https://pypi.org/simple/' \
    	--scope 'release-policy=latest-listed-wheel'

# Plan and fetch only explicitly curated PyPI packages, without the global project index.
fetch-pypi-metadata snapshot:
    mkdir -p "$(dirname '{{ snapshot }}')"
    mkdir '{{ snapshot }}'
    {{ bnck_bulk }} plan-pypi-metadata --curations curations \
        --output-directory '{{ snapshot }}/packages' --output '{{ snapshot }}/metadata.json'
    just fetch-registry-plan pypi '{{ snapshot }}/metadata.tsv'

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
