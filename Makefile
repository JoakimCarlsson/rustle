.PHONY: fmt lint

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings
