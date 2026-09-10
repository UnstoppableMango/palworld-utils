CARGO ?= cargo

RUST_SRC ?= $(shell find . -name '*.rs')

build:
	nix build .#

test:
	$(CARGO) test

update:
	nix flake update

check lint:
	nix flake check

format fmt:
	nix fmt

gen-palworld-types:
	nix build .#palworld-types-codegen --out-link result-palworld-types
	cp result-palworld-types src/palworld_type_hints.rs
	chmod +w src/palworld_type_hints.rs
	rm result-palworld-types
	nix fmt

tidy: Cargo.lock

Cargo.lock: Cargo.toml ${RUST_SRC}
	$(CARGO) generate-lockfile
