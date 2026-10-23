#!/usr/bin/env bash
# Create the GitHub release that publishes the signed dataset artifacts.
set -euo pipefail

if gh release view "$TAG" >/dev/null 2>&1; then
	printf 'Release %s already exists\n' "$TAG" >&2
	exit 1
fi

gh release create "$TAG" \
	"var/releases/bnck-$RELEASE.tar.zst" \
	"var/releases/bnck-$RELEASE/manifest.json" \
	"var/releases/bnck-$RELEASE/bnck.sqlite.zst" \
	"var/releases/bnck-$RELEASE/SHA256SUMS" \
	"var/releases/bnck-$RELEASE/SHA256SUMS.minisig" \
	--title "bnck dataset $RELEASE" \
	--notes "Dataset release $RELEASE."
