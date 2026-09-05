npm_input := 'var/input/npm'
npm_output := 'var/acquired/npm.jsonl'

fetch-npm package:
    mkdir -p "$(dirname '{{ npm_input }}/{{ package }}.json')"
    curl --fail --location --retry 3 \
    	--output '{{ npm_input }}/{{ package }}.json' \
    	'https://registry.npmjs.org/{{ package }}'

import-npm:
    mkdir -p 'var/acquired'
    {{ bnck }} import-npm \
    	--input '{{ npm_input }}' \
    	--output '{{ npm_output }}' \
    	--scope 'registry=https://registry.npmjs.org' \
    	--scope 'release-policy=latest-dist-tag'
