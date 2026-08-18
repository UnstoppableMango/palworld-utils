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

tidy: Cargo.lock

Cargo.lock: Cargo.toml ${RUST_SRC}
	$(CARGO) generate-lockfile
