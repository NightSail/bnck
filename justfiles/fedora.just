fedora_release := env_var_or_default('FEDORA_RELEASE', '43')
fedora_arch := env_var_or_default('FEDORA_ARCH', 'x86_64')
fedora_base_url := env_var_or_default('FEDORA_BASE_URL', 'https://download.fedoraproject.org/pub/fedora/linux/releases/' + fedora_release + '/Everything/' + fedora_arch + '/os')
fedora_input := 'var/input/fedora'
fedora_output := 'var/acquired/fedora.jsonl'

fetch-fedora:
    #!/usr/bin/env bash
    set -euo pipefail
    input='{{ fedora_input }}'
    context="$input/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    rm -rf "$input/repodata"
    mkdir -p "$input/repodata"
    {{ context }} reset "$context"
    curl --fail --location --retry 3 \
        --output "$input/repomd.xml" \
        '{{ fedora_base_url }}/repodata/repomd.xml'
    {{ context }} input "$context" "$input/repomd.xml" \
        '{{ fedora_base_url }}/repodata/repomd.xml' 'application/xml' "$retrieved_at" '' '' ''
    {{ bnck }} plan-fedora \
        --repomd "$input/repomd.xml" \
        --base-url '{{ fedora_base_url }}' \
        --output-directory "$input/repodata" \
        --output "$input/downloads.tsv"
    while IFS=$'\t' read -r url output; do
        curl --fail --location --retry 3 --continue-at - --output "$output" "$url"
        case "$output" in
            *.zst|*.zstd) media_type='application/zstd' ;;
            *.gz) media_type='application/gzip' ;;
            *.xz) media_type='application/x-xz' ;;
            *) media_type='application/octet-stream' ;;
        esac
        {{ context }} input "$context" "$output" "$url" "$media_type" "$retrieved_at" '' '' ''
    done < "$input/downloads.tsv"
    {{ size }} 'Pulled into {{ fedora_input }}' '{{ fedora_input }}'

import-fedora:
    mkdir -p 'var/acquired'
    primary="$(find '{{ fedora_input }}/repodata' -type f -name 'primary-*' -print -quit)"; \
    	filelists="$(find '{{ fedora_input }}/repodata' -type f -name 'filelists-*' -print -quit)"; \
    	test -n "$primary" && test -n "$filelists"; \
    	{{ bnck }} import-fedora \
    		--primary "$primary" \
    		--filelists "$filelists" \
    		--output '{{ fedora_output }}' --context '{{ fedora_input }}/acquisition-context.tsv' \
    		--scope 'release={{ fedora_release }}' \
    		--scope 'repository=everything' \
    		--scope 'architecture={{ fedora_arch }}'
    @{{ size }} 'Wrote {{ fedora_output }}' '{{ fedora_output }}'
