arch_mirror := env_var_or_default('ARCH_MIRROR', 'https://geo.mirror.pkgbuild.com')
arch_arch := env_var_or_default('ARCH_ARCH', 'x86_64')
arch_input := 'var/input/arch'
arch_output := 'var/acquired/arch.jsonl'

fetch-arch:
    #!/usr/bin/env bash
    set -euo pipefail
    input='{{ arch_input }}'
    context="$input/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    mkdir -p "$input/archives" "$input/headers"
    {{ context }} reset "$context"
    for repository in core extra; do
        for database in db files; do
            header="$input/headers/$repository.$database.headers"
            output="$input/archives/$repository.$database"
            url='{{ arch_mirror }}/'"$repository/os/{{ arch_arch }}/$repository.$database"
            curl --fail --location --retry 3 \
                --user-agent '{{ bnck_user_agent }}' \
                --dump-header "$header" \
                --output "$output" \
                "$url"
            {{ context }} input "$context" "$output" "$url" 'application/gzip' "$retrieved_at" '' '' ''
            last_modified="$(awk 'BEGIN { IGNORECASE = 1 } /^last-modified:/ { sub(/^[^:]*:[ ]*/, ""); sub(/\r$/, ""); print; exit }' "$header")"
            if test -n "$last_modified"; then
                {{ context }} boundary "$context" "$repository-$database-last-modified" "$last_modified"
            fi
            rm -f "$header"
        done
    done
    rm -rf "$input/extracted"
    for repository in core extra; do
        mkdir -p "$input/extracted/$repository"
        tar -xf "$input/archives/$repository.files" -C "$input/extracted/$repository"
    done
    {{ size }} 'Pulled into {{ arch_input }}' '{{ arch_input }}'

import-arch:
    mkdir -p 'var/acquired'
    {{ bnck }} import-arch \
    	--input '{{ arch_input }}/extracted' \
    	--output '{{ arch_output }}' --context '{{ arch_input }}/acquisition-context.tsv' \
    	--scope 'repositories=core,extra' \
    	--scope 'architecture={{ arch_arch }}'
    @{{ size }} 'Wrote {{ arch_output }}' '{{ arch_output }}'
