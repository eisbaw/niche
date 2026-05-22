set shell := ["nix-shell", "--run"]

# Build post2html
build:
    cargo build

# Run clippy and tests
check:
    cargo clippy -- -D warnings && cargo test

# Format Rust source
fmt:
    cargo fmt

# Remove cargo artifacts
clean:
    cargo clean

# Engine smoke test (flake check on tests/fixtures/site)
e2e:
    bash tests/e2e.sh
