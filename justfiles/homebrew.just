homebrew_api := env_var_or_default('HOMEBREW_API', 'https://formulae.brew.sh/api')
homebrew_input := 'var/input/homebrew'
homebrew_output := 'var/acquired/homebrew.jsonl'

fetch-homebrew:
    mkdir -p '{{ homebrew_input }}'
    curl --fail --location --retry 3 \
    	--output '{{ homebrew_input }}/formula.json' \
    	'{{ homebrew_api }}/formula.json'
    curl --fail --location --retry 3 \
    	--output '{{ homebrew_input }}/cask.json' \
    	'{{ homebrew_api }}/cask.json'
    @{{ size }} 'Pulled into {{ homebrew_input }}' '{{ homebrew_input }}'

import-homebrew:
    mkdir -p 'var/acquired'
    {{ bnck }} import-homebrew \
    	--formulae '{{ homebrew_input }}/formula.json' \
    	--casks '{{ homebrew_input }}/cask.json' \
    	--output '{{ homebrew_output }}' \
    	--scope 'tap=homebrew/core,homebrew/cask' \
    	--scope 'package-kinds=formula,cask'
    @{{ size }} 'Wrote {{ homebrew_output }}' '{{ homebrew_output }}'
