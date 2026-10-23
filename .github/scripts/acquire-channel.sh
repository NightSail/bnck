#!/usr/bin/env bash
# Fetch and import one acquisition channel into var/acquired.
set -euo pipefail

mkdir -p var/acquired

case "$CHANNEL" in
	debian)
		just fetch-debian
		just import-debian
		;;
	fedora)
		just fetch-fedora
		just import-fedora
		;;
	arch)
		just fetch-arch
		just import-arch
		;;
	homebrew)
		just fetch-homebrew
		just import-homebrew
		;;
	crates-io)
		just fetch-crates-dump
		just import-crates-dump
		;;
	npm)
		snapshot='var/input/npm-ci'
		rm -rf "$snapshot"
		just fetch-npm "$snapshot"
		just import-npm "$snapshot" 'var/acquired/npm.jsonl'
		;;
	pypi)
		snapshot='var/input/pypi-ci'
		rm -rf "$snapshot"
		just fetch-pypi-metadata "$snapshot"
		just fetch-pypi-wheels "$snapshot"
		cargo run --release --quiet --bin bnck -- import-pypi \
			--plan "$snapshot/wheels.json" \
			--output 'var/acquired/pypi.jsonl' \
			--scope 'index=https://pypi.org/simple/' \
			--scope 'release-policy=latest-non-yanked-stable' \
			--scope 'wheel-policy=prefer-universal-then-smallest' \
			--scope 'catalog-policy=bounded-non-atomic-scan'
		;;
	*)
		printf 'Unknown acquisition channel: %s\n' "$CHANNEL" >&2
		exit 1
		;;
esac
