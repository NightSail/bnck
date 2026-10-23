#!/usr/bin/env bash
# Install the tooling required to package and sign a dataset release.
set -euo pipefail

sudo apt-get update
sudo apt-get install -y minisign zstd
