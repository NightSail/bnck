set shell := ['bash', '-eu', '-o', 'pipefail', '-c']

bnck := 'cargo run --quiet --bin bnck --'
bnck_bulk := 'cargo run --release --quiet --bin bnck --'
languages_input := 'var/input/languages'
diagnostics_report := 'var/diagnostics/report.json'
diagnostics_baseline := 'var/diagnostics/baseline.json'
# Report the on-disk size of a job's fetched inputs or written artifacts.
size := justfile_directory() + '/justfiles/size.sh'
context := justfile_directory() + '/justfiles/context.sh'

import 'justfiles/arch.just'
import 'justfiles/crates.just'
import 'justfiles/debian.just'
import 'justfiles/fedora.just'
import 'justfiles/homebrew.just'
import 'justfiles/npm.just'
import 'justfiles/pypi.just'
import 'justfiles/registries.just'

default:
    @just --list

build:
    cargo build

check:
    cargo fmt --check
    for file in justfile justfiles/*.just; do just --fmt --check --justfile "$file"; done
    cargo clippy --all-targets -- -D warnings
    cargo test
    bun run typecheck
    bun test ./web/*.test.ts ./tests/*.test.js

format:
    cargo fmt
    for file in justfile justfiles/*.just; do just --fmt --justfile "$file"; done

generate acquisitions='var/acquired' output='var/providers.jsonl':
    mkdir -p "$(dirname '{{ output }}')"
    candidate_only=''; \
    if [ "${BNCK_CANDIDATE_ONLY:-0}" = '1' ]; then candidate_only='--candidate-only'; fi; \
    {{ bnck }} generate \
        --acquisitions '{{ acquisitions }}' \
        --curations curations \
        --output '{{ output }}' \
        --diagnostics '{{ diagnostics_report }}' \
        --baseline '{{ diagnostics_baseline }}' \
        $candidate_only
    @{{ size }} 'Wrote {{ output }}' '{{ output }}'

# Preview automatic curation from normalized Terminal Trove and tldr hints.
ingest-hints hints='var/hints' acquisitions='var/acquired' report='var/hint-ingestion.json':
    {{ bnck }} ingest-hints \
        --hints '{{ hints }}' \
        --acquisitions '{{ acquisitions }}' \
        --curations curations \
        --report '{{ report }}'

# Apply automatic curation from normalized Terminal Trove and tldr hints.
apply-hints hints='var/hints' acquisitions='var/acquired' report='var/hint-ingestion.json':
    {{ bnck }} ingest-hints \
        --hints '{{ hints }}' \
        --acquisitions '{{ acquisitions }}' \
        --curations curations \
        --report '{{ report }}' \
        --apply

generate-sqlite dataset_release input='var/providers.jsonl' output='var/bnck.sqlite.zst' names_input='var/names.jsonl':
    generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; \
    	revision="$(git rev-parse HEAD)"; \
    	{{ bnck }} generate-sqlite \
    		--input '{{ input }}' \
        --names-input '{{ names_input }}' \
    		--output '{{ output }}' \
    		--dataset-release '{{ dataset_release }}' \
    		--generated-at "$generated_at" \
    		--generator-revision "$revision"
    @{{ size }} 'Wrote {{ output }}' '{{ output }}'

release dataset_release secret_key='minisign.key' public_key='minisign.pub' input='var/providers.jsonl' acquisitions='var/acquired' output='var/releases' names_input='var/names.jsonl':
    #!/usr/bin/env bash
    set -euo pipefail
    release='{{ output }}/bnck-{{ dataset_release }}'
    archive="${release}.tar.zst"
    test ! -e "$release" || { printf 'Release directory already exists: %s\n' "$release" >&2; exit 1; }
    test ! -e "$archive" || { printf 'Release archive already exists: %s\n' "$archive" >&2; exit 1; }
    test -f '{{ secret_key }}' || { printf 'Signing key not found: %s\n' '{{ secret_key }}' >&2; exit 1; }
    test -f '{{ public_key }}' || { printf 'Public key not found: %s\n' '{{ public_key }}' >&2; exit 1; }
    command -v minisign >/dev/null || { printf 'minisign is required to release\n' >&2; exit 1; }
    command -v zstd >/dev/null || { printf 'zstd is required to package a release\n' >&2; exit 1; }
    generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    revision="$(git rev-parse HEAD)"
    candidate_only=''
    if [ "${BNCK_CANDIDATE_ONLY:-0}" = '1' ]; then candidate_only='--candidate-only'; fi
    {{ bnck }} assemble-release \
        --input '{{ input }}' \
        --names-input '{{ names_input }}' \
        --acquisitions '{{ acquisitions }}' \
        --output "$release" \
        --dataset-release '{{ dataset_release }}' \
        --generated-at "$generated_at" \
        --generator-revision "$revision" \
        --diagnostics '{{ diagnostics_report }}' \
        --baseline '{{ diagnostics_baseline }}' \
        $candidate_only
    minisign -S -s '{{ secret_key }}' -m "$release/SHA256SUMS" -x "$release/SHA256SUMS.minisig"
    minisign -Vm "$release/SHA256SUMS" -p '{{ public_key }}'
    tar --zstd -cf "$archive" -C "$(dirname "$release")" "$(basename "$release")"
    {{ size }} 'Wrote release directory' "$release"
    {{ size }} 'Wrote release archive' "$archive"

# Build the static site, replacing any previous output at the destination.
site-build database='var/bnck.sqlite.zst' output='var/site':
    rm -rf '{{ output }}'
    bun web/generate.ts --database '{{ database }}' --output '{{ output }}'
    @{{ size }} 'Wrote {{ output }}' '{{ output }}'

# Serve a built static site from disk.
site-preview address='127.0.0.1:3000' directory='var/site':
    bun web/preview.ts '{{ directory }}' '{{ address }}'

# Run the site against a SQLite artifact with hot reloading.
site-dev address='127.0.0.1:3000' database='var/bnck.sqlite.zst':
    bun web/dev.ts --database '{{ database }}' --address '{{ address }}'

fetch-distros: fetch-debian fetch-fedora fetch-arch fetch-homebrew

import-distros: import-debian import-fedora import-arch import-homebrew

# Acquire the crates.io dump and refresh explicitly curated npm/PyPI packages.
fetch-languages:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p '{{ languages_input }}'
    inputs="$(cd '{{ languages_input }}' && pwd -P)"
    if test -d "$inputs/current"; then
        printf 'Language input selection must be a file: %s/current\n' "$inputs" >&2
        exit 1
    fi
    snapshot="$(mktemp -d "$inputs/$(date -u '+%Y-%m-%dT%H%M%SZ').XXXXXX")"
    printf 'Fetching language metadata into %s\n' "$snapshot"
    just fetch-crates-dump "$snapshot/crates"
    just fetch-npm "$snapshot/npm"
    just fetch-pypi-metadata "$snapshot/pypi"
    just fetch-pypi-wheels "$snapshot/pypi"
    # Publish the selection only after every fetch succeeds.
    printf '%s\n' "$snapshot" > "$snapshot/current"
    mv -f "$snapshot/current" "$inputs/current"
    {{ size }} "Pulled into $snapshot" "$snapshot"

import-languages:
    #!/usr/bin/env bash
    set -euo pipefail
    selection='{{ languages_input }}/current'
    if test ! -f "$selection"; then
        printf 'No completed language fetch. Run just fetch-languages first.\n' >&2
        exit 1
    fi
    # Pin the selection once so concurrent refreshes cannot mix channel inputs.
    if ! IFS= read -r snapshot < "$selection" || test ! -d "$snapshot"; then
        printf 'Invalid language input selection: %s\n' "$selection" >&2
        exit 1
    fi
    just import-crates-dump "$snapshot/crates"
    mkdir -p var/acquired
    just import-npm "$snapshot/npm" '{{ npm_output }}'
    {{ bnck_bulk }} import-pypi --plan "$snapshot/pypi/wheels.json" --output '{{ pypi_output }}' \
        --context "$snapshot/pypi/acquisition-context.tsv" \
        --scope 'index=https://pypi.org/simple/' --scope 'release-policy=latest-non-yanked-stable' \
        --scope 'wheel-policy=prefer-universal-then-smallest' --scope 'catalog-policy=bounded-non-atomic-scan'
    {{ size }} 'Wrote var/acquired' 'var/acquired'

stats:
    for file in var/acquired/*.jsonl; do printf '%-32s %s\n' "$file" "$(wc -l < "$file")"; done
    if test -f var/providers.jsonl; then printf '%-32s %s\n' var/providers.jsonl "$(wc -l < var/providers.jsonl)"; fi
    if test -f var/names.jsonl; then printf '%-32s %s\n' var/names.jsonl "$(wc -l < var/names.jsonl)"; fi

# Keep demo inputs and acquisitions separate from production snapshots and providers.
demo:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p var/demo
    snapshot="$(mktemp -d 'var/demo/run.XXXXXX')"
    just fetch-npm "$snapshot/npm"
    just import-npm "$snapshot/npm" "$snapshot/acquired/npm.jsonl"
    just --set pypi_input "$snapshot/pypi" --set pypi_output "$snapshot/acquired/pypi.jsonl" fetch-pypi black import-pypi
    just generate "$snapshot/acquired" "$snapshot/providers.jsonl"
    printf 'Generated %s provider records in %s/providers.jsonl\n' "$(wc -l < "$snapshot/providers.jsonl")" "$snapshot"
