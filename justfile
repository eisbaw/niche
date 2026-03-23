# Run all recipes inside nix-shell
set shell := ["nix-shell", "--run"]

# Build the Rust binary (site.nix build will come later)
build:
    cargo build

# Run clippy and tests
check:
    cargo clippy -- -D warnings && cargo test

# Format Rust source
fmt:
    cargo fmt

# Remove build artifacts
clean:
    cargo clean && rm -f result

# Serve the built site (requires result/ from nix-build)
serve:
    python3 -m http.server -d result/

# Scaffold a new post: just new my-post-slug
new slug:
    mkdir -p content/{{slug}}
    printf '{\n  title = "TODO";\n  date = "%s";\n  tags = [];\n}\n' "$(date +%Y-%m-%d)" > content/{{slug}}/meta.nix
    touch content/{{slug}}/post.md
    @echo "Created content/{{slug}}/"

# End-to-end tests (placeholder)
e2e:
    @echo "E2E tests not yet implemented"
