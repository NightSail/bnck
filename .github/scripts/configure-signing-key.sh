#!/usr/bin/env bash
# Materialize the unencrypted minisign secret key from CI secrets.
set -euo pipefail

test -n "$MINISIGN_SECRET_KEY"
printf '%s\n' "$MINISIGN_SECRET_KEY" > minisign.key
chmod 600 minisign.key
test -f minisign.pub

# Minisign encrypts keys even with an empty passphrase; CI cannot answer its prompt.
# The KDF algorithm field is zero only for unencrypted keys.
kdf_alg="$(
	sed -n '2p' minisign.key |
		base64 -d |
		od -An -tx1 -j2 -N2 |
		tr -d ' \n'
)"
if [ "$kdf_alg" != '0000' ]; then
	printf 'MINISIGN_SECRET_KEY is passphrase-protected; CI requires an unencrypted key.\n' >&2
	printf 'Remove the passphrase with: minisign -C -W -s minisign.key\n' >&2
	exit 1
fi

derived_pub="$(mktemp)"
trap 'rm -f "$derived_pub"' EXIT
rm -f "$derived_pub"
minisign -R -s minisign.key -p "$derived_pub" >/dev/null
if [ "$(sed -n '2p' "$derived_pub")" != "$(sed -n '2p' minisign.pub)" ]; then
	printf 'minisign.pub does not match MINISIGN_SECRET_KEY\n' >&2
	exit 1
fi
