#!/usr/bin/env bash
# Materialize the unencrypted minisign secret key from CI secrets.
set -euo pipefail

test -n "$MINISIGN_SECRET_KEY"
printf '%s\n' "$MINISIGN_SECRET_KEY" > minisign.key
chmod 600 minisign.key
test -f minisign.pub

# Accept either the complete key file or just its base64 key blob.
key_blob="$(
	sed 's/\r$//; s/^[[:space:]]*//; s/[[:space:]]*$//' minisign.key |
		sed -n '/^[A-Za-z0-9+/][A-Za-z0-9+/=]*$/p' |
		head -n 1
)"
if [ -z "$key_blob" ]; then
	printf 'MINISIGN_SECRET_KEY does not contain a minisign key blob.\n' >&2
	printf 'Set it from the complete minisign.key file.\n' >&2
	exit 1
fi

# Normalize the secret key file so minisign receives its expected two lines.
printf 'untrusted comment: minisign secret key\n%s\n' "$key_blob" > minisign.key
chmod 600 minisign.key

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

if ! printf '%s' "$key_blob" | base64 -d > "$work_dir/key.bin" 2>/dev/null; then
	printf 'MINISIGN_SECRET_KEY contains an invalid minisign key blob.\n' >&2
	exit 1
fi
if [ "$(wc -c < "$work_dir/key.bin")" -lt 4 ]; then
	printf 'MINISIGN_SECRET_KEY contains an invalid minisign key blob.\n' >&2
	exit 1
fi

# Minisign encrypts keys even with an empty passphrase; CI cannot answer its prompt.
# The KDF algorithm field is zero only for unencrypted keys.
kdf_alg="$(od -An -tx1 -j2 -N2 "$work_dir/key.bin" | tr -d ' \n')"
if [ "$kdf_alg" != '0000' ]; then
	printf 'MINISIGN_SECRET_KEY is passphrase-protected; CI requires an unencrypted key.\n' >&2
	printf 'Remove the passphrase with: minisign -C -W -s minisign.key\n' >&2
	exit 1
fi

minisign -R -s minisign.key -p "$work_dir/minisign.pub" >/dev/null
if [ "$(sed -n '2p' "$work_dir/minisign.pub")" != "$(sed -n '2p' minisign.pub)" ]; then
	printf 'minisign.pub does not match MINISIGN_SECRET_KEY\n' >&2
	exit 1
fi
