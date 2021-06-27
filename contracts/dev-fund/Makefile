
.PHONY: compile-optimized-reproducible
compile-optimized-reproducible:
	docker run --rm -v "$$(pwd)"/../..:/contract \
		--env "CARGO_TARGET_DIR=/contract/contracts/$$(basename "$$(pwd)")/target" \
		--mount type=volume,source="$$(basename "$$(pwd)")_cache",target=/code/target \
		--mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
		--workdir "/contract/contracts/$$(basename "$$(pwd)")" \
		enigmampc/secret-contract-optimizer:1.0.4