.PHONY: run fmt lint bench

run:
	cargo run --release --example hello_world

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets -- -D warnings

bench:
	cargo bench -p rustle-bench
