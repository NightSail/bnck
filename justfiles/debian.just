debian_base_url := env_var_or_default('DEBIAN_BASE_URL', 'https://deb.debian.org/debian')
debian_suite := env_var_or_default('DEBIAN_SUITE', 'stable')
debian_component := env_var_or_default('DEBIAN_COMPONENT', 'main')
debian_arch := env_var_or_default('DEBIAN_ARCH', 'amd64')
debian_input := 'var/input/debian'
debian_output := 'var/acquired/debian.jsonl'

fetch-debian:
    mkdir -p '{{ debian_input }}'
    curl --fail --location --retry 3 \
    	--output '{{ debian_input }}/InRelease' \
    	'{{ debian_base_url }}/dists/{{ debian_suite }}/InRelease'
    curl --fail --location --retry 3 \
    	--output '{{ debian_input }}/Packages.xz' \
    	'{{ debian_base_url }}/dists/{{ debian_suite }}/{{ debian_component }}/binary-{{ debian_arch }}/Packages.xz'
    curl --fail --location --retry 3 \
    	--output '{{ debian_input }}/Sources.xz' \
    	'{{ debian_base_url }}/dists/{{ debian_suite }}/{{ debian_component }}/source/Sources.xz'
    curl --fail --location --retry 3 \
    	--output '{{ debian_input }}/Contents.gz' \
    	'{{ debian_base_url }}/dists/{{ debian_suite }}/{{ debian_component }}/Contents-{{ debian_arch }}.gz'
    xz -dc '{{ debian_input }}/Packages.xz' > '{{ debian_input }}/Packages'
    xz -dc '{{ debian_input }}/Sources.xz' > '{{ debian_input }}/Sources'
    gzip -dc '{{ debian_input }}/Contents.gz' > '{{ debian_input }}/Contents'

import-debian: build
    mkdir -p 'var/acquired'
    ./target/debug/bnck import-debian \
    	--packages '{{ debian_input }}/Packages' \
    	--sources '{{ debian_input }}/Sources' \
    	--contents '{{ debian_input }}/Contents' \
    	--output '{{ debian_output }}'
