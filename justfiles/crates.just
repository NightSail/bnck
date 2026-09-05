crates_dump_url := 'https://static.crates.io/db-dump.tar.gz'
crates_dump_directory := 'var/input/crates-dump'
crates_api_directory := 'var/input/crates-api'
crates_output := 'var/acquired/crates-io.jsonl'

fetch-crates-dump:
    mkdir -p '{{ crates_dump_directory }}'
    curl --fail --location --retry 3 --continue-at - \
    	--output '{{ crates_dump_directory }}/db-dump.tar.gz' \
    	'{{ crates_dump_url }}'
    tar -xzf '{{ crates_dump_directory }}/db-dump.tar.gz' -C '{{ crates_dump_directory }}'

fetch-crate package:
    mkdir -p '{{ crates_api_directory }}'
    curl --fail --location --retry 3 \
    	--user-agent 'bnck/0.1 (dataset importer)' \
    	--output '{{ crates_api_directory }}/{{ package }}.json' \
    	'https://crates.io/api/v1/crates/{{ package }}'

import-crates-dump: build
    mkdir -p 'var/acquired'
    crates_csv="$(find '{{ crates_dump_directory }}' -name crates.csv -print -quit)"; \
    	versions_csv="$(find '{{ crates_dump_directory }}' -name versions.csv -print -quit)"; \
    	test -n "$crates_csv" && test -n "$versions_csv"; \
    	./target/debug/bnck import-crates-dump \
    		--crates "$crates_csv" \
    		--versions "$versions_csv" \
    		--output '{{ crates_output }}' \
    		--scope 'registry=crates.io' \
    		--scope 'release-policy=latest-non-yanked-stable'

import-crates-api: build
    mkdir -p 'var/acquired'
    ./target/debug/bnck import-crates-api \
    	--input '{{ crates_api_directory }}' \
    	--output '{{ crates_output }}' \
    	--scope 'registry=crates.io' \
    	--scope 'release-policy=latest-non-yanked-stable'
