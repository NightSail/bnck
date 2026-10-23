debian_base_url := env_var_or_default('DEBIAN_BASE_URL', 'https://deb.debian.org/debian')
debian_suite := env_var_or_default('DEBIAN_SUITE', 'stable')
debian_sid_suite := env_var_or_default('DEBIAN_SID_SUITE', 'unstable')
debian_component := env_var_or_default('DEBIAN_COMPONENT', 'main')
debian_arch := env_var_or_default('DEBIAN_ARCH', 'amd64')
debian_keyring := env_var_or_default('DEBIAN_KEYRING', '/usr/share/keyrings/debian-archive-keyring.gpg')
debian_input := 'var/input/debian'
debian_sid_input := 'var/input/debian-sid'
debian_output := 'var/acquired/debian.jsonl'
debian_sid_output := 'var/acquired/debian-sid.jsonl'

fetch-debian: fetch-debian-stable fetch-debian-sid

fetch-debian-stable: (_fetch-debian-suite debian_suite debian_input)

fetch-debian-sid: (_fetch-debian-suite debian_sid_suite debian_sid_input)

_fetch-debian-suite suite input:
    #!/usr/bin/env bash
    set -euo pipefail
    suite='{{ suite }}'
    input='{{ input }}'
    base='{{ debian_base_url }}'
    component='{{ debian_component }}'
    architecture='{{ debian_arch }}'
    keyring='{{ debian_keyring }}'
    context="$input/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    mkdir -p "$input"
    test -f "$keyring" || {
        printf 'Debian archive keyring not found: %s\n' "$keyring" >&2
        printf 'Install debian-archive-keyring or set DEBIAN_KEYRING.\n' >&2
        exit 1
    }
    {{ context }} reset "$context"
    curl --fail --location --retry 3         --output "$input/InRelease"         "$base/dists/$suite/InRelease"
    status="$input/gpgv.status"
    if ! gpgv --status-fd 1 --keyring "$keyring" "$input/InRelease" > "$status" 2> "$input/gpgv.err"; then
        cat "$input/gpgv.err" >&2
        exit 1
    fi
    signer="$(awk '$1 == "[GNUPG:]" && $2 == "VALIDSIG" { print $3; exit }' "$status")"
    test -n "$signer"
    rm -f "$status" "$input/gpgv.err"
    {{ context }} input "$context" "$input/InRelease"         "$base/dists/$suite/InRelease" 'application/pgp-signature' "$retrieved_at" openpgp verified "$signer"
    curl --fail --location --retry 3         --output "$input/Packages.xz"         "$base/dists/$suite/$component/binary-$architecture/Packages.xz"
    {{ context }} input "$context" "$input/Packages.xz"         "$base/dists/$suite/$component/binary-$architecture/Packages.xz" 'application/x-xz'         "$retrieved_at" '' '' ''
    curl --fail --location --retry 3         --output "$input/Sources.xz"         "$base/dists/$suite/$component/source/Sources.xz"
    {{ context }} input "$context" "$input/Sources.xz"         "$base/dists/$suite/$component/source/Sources.xz" 'application/x-xz'         "$retrieved_at" '' '' ''
    curl --fail --location --retry 3         --output "$input/Contents.gz"         "$base/dists/$suite/$component/Contents-$architecture.gz"
    {{ context }} input "$context" "$input/Contents.gz"         "$base/dists/$suite/$component/Contents-$architecture.gz" 'application/gzip'         "$retrieved_at" '' '' ''
    curl --fail --location --retry 3         --output "$input/Contents-all.gz"         "$base/dists/$suite/$component/Contents-all.gz"
    {{ context }} input "$context" "$input/Contents-all.gz"         "$base/dists/$suite/$component/Contents-all.gz" 'application/gzip'         "$retrieved_at" '' '' ''
    xz -dc "$input/Packages.xz" > "$input/Packages"
    xz -dc "$input/Sources.xz" > "$input/Sources"
    gzip -dc "$input/Contents.gz" "$input/Contents-all.gz" > "$input/Contents"
    {{ context }} input "$context" "$input/Packages" '' 'text/plain' "$retrieved_at" '' '' ''
    {{ context }} input "$context" "$input/Sources" '' 'text/plain' "$retrieved_at" '' '' ''
    {{ context }} input "$context" "$input/Contents" '' 'text/plain' "$retrieved_at" '' '' ''
    {{ size }} 'Pulled into {{ input }}' '{{ input }}'

import-debian: import-debian-stable import-debian-sid

import-debian-stable: (_import-debian-suite debian_suite debian_input debian_output)

import-debian-sid: (_import-debian-suite debian_sid_suite debian_sid_input debian_sid_output)

_import-debian-suite suite input output:
    mkdir -p 'var/acquired'
    {{ bnck }} import-debian \
        --packages '{{ input }}/Packages' \
        --sources '{{ input }}/Sources' \
        --contents '{{ input }}/Contents' \
        --output '{{ output }}' --context '{{ input }}/acquisition-context.tsv' \
        --scope 'suite={{ suite }}' \
        --scope 'component={{ debian_component }}' \
        --scope 'architecture={{ debian_arch }}'
    @{{ size }} 'Wrote {{ output }}' '{{ output }}'
