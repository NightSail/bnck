debian_base_url := env_var_or_default('DEBIAN_BASE_URL', 'https://deb.debian.org/debian')
debian_suite := env_var_or_default('DEBIAN_SUITE', 'stable')
debian_sid_suite := env_var_or_default('DEBIAN_SID_SUITE', 'unstable')
debian_component := env_var_or_default('DEBIAN_COMPONENT', 'main')
debian_arch := env_var_or_default('DEBIAN_ARCH', 'amd64')
debian_input := 'var/input/debian'
debian_sid_input := 'var/input/debian-sid'
debian_output := 'var/acquired/debian.jsonl'
debian_sid_output := 'var/acquired/debian-sid.jsonl'

fetch-debian: fetch-debian-stable fetch-debian-sid

fetch-debian-stable: (_fetch-debian-suite debian_suite debian_input)

fetch-debian-sid: (_fetch-debian-suite debian_sid_suite debian_sid_input)

_fetch-debian-suite suite input:
    mkdir -p '{{ input }}'
    curl --fail --location --retry 3 \
        --output '{{ input }}/InRelease' \
        '{{ debian_base_url }}/dists/{{ suite }}/InRelease'
    curl --fail --location --retry 3 \
        --output '{{ input }}/Packages.xz' \
        '{{ debian_base_url }}/dists/{{ suite }}/{{ debian_component }}/binary-{{ debian_arch }}/Packages.xz'
    curl --fail --location --retry 3 \
        --output '{{ input }}/Sources.xz' \
        '{{ debian_base_url }}/dists/{{ suite }}/{{ debian_component }}/source/Sources.xz'
    curl --fail --location --retry 3 \
        --output '{{ input }}/Contents.gz' \
        '{{ debian_base_url }}/dists/{{ suite }}/{{ debian_component }}/Contents-{{ debian_arch }}.gz'
    curl --fail --location --retry 3 \
        --output '{{ input }}/Contents-all.gz' \
        '{{ debian_base_url }}/dists/{{ suite }}/{{ debian_component }}/Contents-all.gz'
    xz -dc '{{ input }}/Packages.xz' > '{{ input }}/Packages'
    xz -dc '{{ input }}/Sources.xz' > '{{ input }}/Sources'
    gzip -dc '{{ input }}/Contents.gz' '{{ input }}/Contents-all.gz' > '{{ input }}/Contents'

import-debian: import-debian-stable import-debian-sid

import-debian-stable: (_import-debian-suite debian_suite debian_input debian_output)

import-debian-sid: (_import-debian-suite debian_sid_suite debian_sid_input debian_sid_output)

_import-debian-suite suite input output:
    mkdir -p 'var/acquired'
    {{ bnck }} import-debian \
        --packages '{{ input }}/Packages' \
        --sources '{{ input }}/Sources' \
        --contents '{{ input }}/Contents' \
        --output '{{ output }}' \
        --scope 'suite={{ suite }}' \
        --scope 'component={{ debian_component }}' \
        --scope 'architecture={{ debian_arch }}'
