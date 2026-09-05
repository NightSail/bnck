arch_mirror := env_var_or_default('ARCH_MIRROR', 'https://geo.mirror.pkgbuild.com')
arch_arch := env_var_or_default('ARCH_ARCH', 'x86_64')
arch_input := 'var/input/arch'
arch_output := 'var/acquired/arch.jsonl'

fetch-arch:
    mkdir -p '{{ arch_input }}/archives'
    for repository in core extra; do \
    	for database in db files; do \
    		curl --fail --location --retry 3 \
    			--output "{{ arch_input }}/archives/$repository.$database" \
    			"{{ arch_mirror }}/$repository/os/{{ arch_arch }}/$repository.$database"; \
    	done; \
    done
    rm -rf '{{ arch_input }}/extracted'
    for repository in core extra; do \
    	mkdir -p "{{ arch_input }}/extracted/$repository"; \
    	tar -xf "{{ arch_input }}/archives/$repository.files" \
    		-C "{{ arch_input }}/extracted/$repository"; \
    done

import-arch: build
    mkdir -p 'var/acquired'
    ./target/debug/bnck import-arch \
    	--input '{{ arch_input }}/extracted' \
    	--output '{{ arch_output }}' \
    	--scope 'repositories=core,extra' \
    	--scope 'architecture={{ arch_arch }}'
