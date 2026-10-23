homebrew_api := env_var_or_default('HOMEBREW_API', 'https://formulae.brew.sh/api')
homebrew_input := 'var/input/homebrew'
homebrew_output := 'var/acquired/homebrew.jsonl'

fetch-homebrew:
    #!/usr/bin/env bash
    set -euo pipefail
    input='{{ homebrew_input }}'
    context="$input/acquisition-context.tsv"
    retrieved_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    mkdir -p "$input"
    {{ context }} reset "$context"
    for kind in formula cask; do
        url='{{ homebrew_api }}/'"$kind.json"
        curl --fail --location --retry 3 --output "$input/$kind.json" "$url"
        {{ context }} input "$context" "$input/$kind.json" "$url" 'application/json' "$retrieved_at" '' '' ''
    done
    {{ size }} 'Pulled into {{ homebrew_input }}' '{{ homebrew_input }}'

import-homebrew:
    mkdir -p 'var/acquired'
    {{ bnck }} import-homebrew \
    	--formulae '{{ homebrew_input }}/formula.json' \
    	--casks '{{ homebrew_input }}/cask.json' \
    	--output '{{ homebrew_output }}' --context '{{ homebrew_input }}/acquisition-context.tsv' \
    	--scope 'tap=homebrew/core,homebrew/cask' \
    	--scope 'package-kinds=formula,cask'
    @{{ size }} 'Wrote {{ homebrew_output }}' '{{ homebrew_output }}'
