set shell := ['bash', '-eu', '-o', 'pipefail', '-c']

bnck := 'cargo run --quiet --bin bnck --'
bnck_bulk := 'cargo run --release --quiet --bin bnck --'

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
    bun test ./web/*.test.js

format:
    cargo fmt
    for file in justfile justfiles/*.just; do just --fmt --justfile "$file"; done

generate acquisitions='var/acquired' output='var/providers.jsonl':
    mkdir -p "$(dirname '{{ output }}')"
    {{ bnck }} generate \
    	--acquisitions '{{ acquisitions }}' \
    	--curations curations \
    	--output '{{ output }}'

generate-sqlite dataset_release input='var/providers.jsonl' output='var/bnck.sqlite.zst':
    generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; \
    	revision="$(git rev-parse HEAD)"; \
    	{{ bnck }} generate-sqlite \
    		--input '{{ input }}' \
    		--output '{{ output }}' \
    		--dataset-release '{{ dataset_release }}' \
    		--generated-at "$generated_at" \
    		--generator-revision "$revision"

release dataset_release input='var/providers.jsonl' acquisitions='var/acquired' output='var/releases':
    generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; \
    	revision="$(git rev-parse HEAD)"; \
    	{{ bnck }} assemble-release \
    		--input '{{ input }}' \
    		--acquisitions '{{ acquisitions }}' \
    		--output '{{ output }}/bnck-{{ dataset_release }}' \
    		--dataset-release '{{ dataset_release }}' \
    		--generated-at "$generated_at" \
    		--generator-revision "$revision"

sign-release release_directory secret_key:
    test -f '{{ release_directory }}/SHA256SUMS'
    minisign -S \
    	-s '{{ secret_key }}' \
    	-m '{{ release_directory }}/SHA256SUMS' \
    	-x '{{ release_directory }}/SHA256SUMS.minisig'

generate-site database='var/bnck.sqlite.zst' output='var/site':
    {{ bnck }} generate-site --database '{{ database }}' --output '{{ output }}'

preview directory='var/site' port='3000':
    bun web/preview.js '{{ directory }}' '{{ port }}'

serve database='var/bnck.sqlite.zst' address='127.0.0.1:3000':
    {{ bnck }} serve --database '{{ database }}' --address '{{ address }}'

fetch-distros: fetch-debian fetch-fedora fetch-arch fetch-homebrew

import-distros: import-debian import-fedora import-arch import-homebrew

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

stats:
    for file in var/acquired/*.jsonl; do printf '%-32s %s\n' "$file" "$(wc -l < "$file")"; done
    if test -f var/providers.jsonl; then printf '%-32s %s\n' var/providers.jsonl "$(wc -l < var/providers.jsonl)"; fi

demo: (fetch-crate 'ripgrep') (fetch-npm 'eslint') (fetch-pypi 'black') import-crates-api import-npm import-pypi generate
    @printf 'Generated %s provider records in var/providers.jsonl\n' "$(wc -l < var/providers.jsonl)"
