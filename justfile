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
