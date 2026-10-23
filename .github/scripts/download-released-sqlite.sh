#!/usr/bin/env bash
# Download the released SQLite database used to build the site.
set -euo pipefail

mkdir -p var
gh release download "$TAG" --pattern bnck.sqlite.zst --dir var
