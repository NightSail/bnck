crates_dump_url := 'https://static.crates.io/db-dump.tar.gz'
crates_dump_directory := 'var/input/crates-dump'
crates_output := 'var/acquired/crates-io.jsonl'

fetch-crates-dump directory=crates_dump_directory:
    mkdir -p "$(dirname '{{ directory }}')"
    mkdir '{{ directory }}'
    curl --fail --location --retry 3 --connect-timeout 20 \
    	--output '{{ directory }}/db-dump.tar.gz.tmp' \
    	'{{ crates_dump_url }}'
    mv '{{ directory }}/db-dump.tar.gz.tmp' '{{ directory }}/db-dump.tar.gz'
    tar -xzf '{{ directory }}/db-dump.tar.gz' -C '{{ directory }}' --wildcards \
    	'*/data/crates.csv' '*/data/versions.csv' '*/metadata.json'
    @{{ size }} 'Pulled into {{ directory }}' '{{ directory }}'

import-crates-dump directory=crates_dump_directory:
    mkdir -p 'var/acquired'
    mapfile -t crates_csv < <(find '{{ directory }}' -name crates.csv -print); \
    	mapfile -t versions_csv < <(find '{{ directory }}' -name versions.csv -print); \
    	test "${#crates_csv[@]}" -eq 1 && test "${#versions_csv[@]}" -eq 1; \
    	{{ bnck_bulk }} import-crates-dump \
    		--crates "${crates_csv[0]}" \
    		--versions "${versions_csv[0]}" \
    		--curations curations \
    		--output '{{ crates_output }}' \
    		--scope 'registry=crates.io' \
    		--scope 'release-policy=latest-non-yanked-stable'
    @{{ size }} 'Wrote {{ crates_output }}' '{{ crates_output }}'
