.PHONY: compile _compile
compile: _compile contract.wasm.gz
_compile:
	for path in $$(ls contracts); do \
		make -C "contracts/$$path" compile; \
		mv -f "contracts/$$path/contract.wasm.gz" "$$(basename $${path}).wasm.gz"; \
	done

.PHONY: compile-optimized _compile-optimized
compile-optimized: _compile-optimized
_compile-optimized:
	for path in $$(ls contracts); do \
		make -C "contracts/$$path" compile-optimized; \
		mv -f "contracts/$$path/contract.wasm" "$$(basename $${path}).wasm.gz"; \
	done

.PHONY: compile-w-debug-print _compile-w-debug-print
compile-w-debug-print: _compile-w-debug-print
_compile-w-debug-print:
	for path in $$(ls contracts); do \
		make -C "contracts/$$path" compile-w-debug-print; \
		mv -f "contracts/$$path/contract.wasm.gz" "$$(basename $${path}).wasm.gz"; \
	done

.PHONY: compile-optimized-reproducible
compile-optimized-reproducible:
	for path in $$(ls contracts); do \
		make -C "contracts/$$path" compile-optimized-reproducible; \
		mv -f "contracts/$$path/contract.wasm.gz" "$$(basename $${path}).wasm.gz"; \
    done

.PHONY: start-server
start-server: # CTRL+C to stop
	docker run -it --rm \
		-p 26657:26657 -p 26656:26656 -p 1337:1337 \
		-v $$(pwd):/root/code \
		--name secretdev enigmampc/secret-network-sw-dev:latest

clean:
	cargo clean
	rm -f ./build/*
	rm -rf contracts/*/target
