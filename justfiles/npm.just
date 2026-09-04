npm_input := 'var/input/npm'
npm_output := 'var/acquired/npm.jsonl'

fetch-npm package:
    mkdir -p "$(dirname '{{ npm_input }}/{{ package }}.json')"
    curl --fail --location --retry 3 \
    	--output '{{ npm_input }}/{{ package }}.json' \
    	'https://registry.npmjs.org/{{ package }}'

import-npm: build
    mkdir -p 'var/acquired'
    ./target/debug/bnck import-npm \
    	--input '{{ npm_input }}' \
    	--output '{{ npm_output }}'
