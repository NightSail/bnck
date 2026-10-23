#!/usr/bin/env bash
# Materialize the minisign secret key from CI secrets.
set -euo pipefail

test -n "$MINISIGN_SECRET_KEY"
printf '%s\n' "$MINISIGN_SECRET_KEY" > minisign.key
chmod 600 minisign.key
test -f minisign.pub
