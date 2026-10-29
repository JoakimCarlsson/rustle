.PHONY: run fmt lint

run:
	cargo run --example hello_world

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings
