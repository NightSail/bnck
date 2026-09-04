set shell := ['bash', '-eu', '-o', 'pipefail', '-c']

import 'justfiles/arch.just'
import 'justfiles/crates.just'
import 'justfiles/debian.just'
import 'justfiles/fedora.just'
import 'justfiles/homebrew.just'
import 'justfiles/npm.just'
import 'justfiles/pypi.just'

default:
    @just --list

build:
    cargo build

check:
    cargo fmt --check
    for file in justfile justfiles/*.just; do just --fmt --check --justfile "$file"; done
    cargo clippy --all-targets -- -D warnings
    cargo test

format:
    cargo fmt
    for file in justfile justfiles/*.just; do just --fmt --justfile "$file"; done

generate acquisitions='var/acquired' output='var/providers.jsonl': build
    mkdir -p "$(dirname '{{ output }}')"
    ./target/debug/bnck generate \
    	--acquisitions '{{ acquisitions }}' \
    	--curations curations \
    	--output '{{ output }}'

generate-sqlite dataset_release input='var/providers.jsonl' output='var/bnck.sqlite.zst': build
    generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"; \
    	revision="$(git rev-parse HEAD)"; \
    	./target/debug/bnck generate-sqlite \
    		--input '{{ input }}' \
    		--output '{{ output }}' \
    		--dataset-release '{{ dataset_release }}' \
    		--generated-at "$generated_at" \
    		--generator-revision "$revision"

fetch-distros: fetch-debian fetch-fedora fetch-arch fetch-homebrew

import-distros: import-debian import-fedora import-arch import-homebrew

stats:
    for file in var/acquired/*.jsonl; do printf '%-32s %s\n' "$file" "$(wc -l < "$file")"; done
    if test -f var/providers.jsonl; then printf '%-32s %s\n' var/providers.jsonl "$(wc -l < var/providers.jsonl)"; fi

demo: (fetch-crate 'ripgrep') (fetch-npm 'eslint') (fetch-pypi 'black') import-crates-api import-npm import-pypi generate
    @printf 'Generated %s provider records in var/providers.jsonl\n' "$(wc -l < var/providers.jsonl)"
