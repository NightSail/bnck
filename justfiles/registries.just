# Resume an existing plan without fetching completed responses again.
fetch-registry-plan channel downloads:
    #!/usr/bin/env bash
    set -euo pipefail
    case '{{ channel }}' in
        npm) header='Accept: application/json' ;;
        pypi) header='Accept: application/vnd.pypi.simple.v1+json' ;;
        *) printf 'Unknown registry channel\n' >&2; exit 1 ;;
    esac
    while IFS=$'\t' read -r url output; do
        if test -f "$output" && test -f "${output%.json}.status"; then
            case "$(< "${output%.json}.status")" in
                200|404|410) continue ;;
                *) printf 'Invalid retained metadata status: %s\n' "$output" >&2; exit 1 ;;
            esac
        fi
        if ! status="$(curl --fail-with-body --location --compressed --retry 3 --connect-timeout 20 --max-time 180 \
            --proto '=https' --proto-redir '=https' --header "$header" \
            --user-agent '{{ bnck_user_agent }}' \
            --write-out '%{http_code}' --output "$output.tmp" "$url")"; then
            case "$status" in
                404|410) printf 'Metadata unavailable: %s (HTTP %s)\n' "$url" "$status" >&2 ;;
                *) printf 'Metadata download failed: %s (HTTP %s)\n' "$url" "$status" >&2; exit 1 ;;
            esac
        fi
        case "$status" in
            200|404|410) ;;
            *) printf 'Unexpected metadata response: %s (HTTP %s)\n' "$url" "$status" >&2; exit 1 ;;
        esac
        printf '%s\n' "$status" > "${output%.json}.status"
        mv "$output.tmp" "$output"
        sleep 0.1
    done < '{{ downloads }}'
