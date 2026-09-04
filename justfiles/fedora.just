fedora_base_url := env_var_or_default('FEDORA_BASE_URL', 'https://download.fedoraproject.org/pub/fedora/linux/releases/43/Everything/x86_64/os')
fedora_input := 'var/input/fedora'
fedora_output := 'var/acquired/fedora.jsonl'

fetch-fedora: build
    rm -rf '{{ fedora_input }}/repodata'
    mkdir -p '{{ fedora_input }}/repodata'
    curl --fail --location --retry 3 \
    	--output '{{ fedora_input }}/repomd.xml' \
    	'{{ fedora_base_url }}/repodata/repomd.xml'
    ./target/debug/bnck plan-fedora \
    	--repomd '{{ fedora_input }}/repomd.xml' \
    	--base-url '{{ fedora_base_url }}' \
    	--output-directory '{{ fedora_input }}/repodata' \
    	--output '{{ fedora_input }}/downloads.tsv'
    while IFS=$'\t' read -r url output; do \
    	curl --fail --location --retry 3 --continue-at - --output "$output" "$url"; \
    done < '{{ fedora_input }}/downloads.tsv'

import-fedora: build
    mkdir -p 'var/acquired'
    primary="$(find '{{ fedora_input }}/repodata' -type f -name 'primary-*' -print -quit)"; \
    	filelists="$(find '{{ fedora_input }}/repodata' -type f -name 'filelists-*' -print -quit)"; \
    	test -n "$primary" && test -n "$filelists"; \
    	./target/debug/bnck import-fedora \
    		--primary "$primary" \
    		--filelists "$filelists" \
    		--output '{{ fedora_output }}'
