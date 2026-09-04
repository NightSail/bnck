pypi_input := 'var/input/pypi'
pypi_output := 'var/acquired/pypi.jsonl'

fetch-pypi package: build
    mkdir -p '{{ pypi_input }}/simple' '{{ pypi_input }}/plans' '{{ pypi_input }}/wheels'
    curl --fail --location --retry 3 \
    	--header 'Accept: application/vnd.pypi.simple.v1+json' \
    	--output '{{ pypi_input }}/simple/{{ package }}.json' \
    	'https://pypi.org/simple/{{ package }}/'
    ./target/debug/bnck plan-pypi \
    	--simple '{{ pypi_input }}/simple/{{ package }}.json' \
    	--output-directory '{{ pypi_input }}/wheels' \
    	--output '{{ pypi_input }}/plans/{{ package }}.tsv'
    while IFS=$'\t' read -r url output; do \
    	curl --fail --location --retry 3 --continue-at - --output "$output" "$url"; \
    done < '{{ pypi_input }}/plans/{{ package }}.tsv'

import-pypi: build
    mkdir -p 'var/acquired'
    ./target/debug/bnck import-pypi \
    	--input '{{ pypi_input }}/wheels' \
    	--output '{{ pypi_output }}'
