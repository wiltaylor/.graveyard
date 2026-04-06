# Wscript development commands

default: check

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run all checks (format, clippy, test)
check: fmt-check clippy test

# Run clippy
clippy:
    cargo clippy --workspace --all-features -- -D warnings

# Run tests
test:
    cargo test --workspace --all-features

# Run only library tests
test-lib:
    cargo test -p wscript --all-features

# Run LSP-specific tests
test-lsp:
    cargo test -p wscript --features lsp -- lsp

# Run DAP-specific tests
test-dap:
    cargo test -p wscript --features dap -- dap

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format all code
fmt:
    cargo fmt --all

# Run a .ws file
run FILE:
    cargo run -p wscript-cli -- run {{FILE}}

# Run with debug mode
run-debug FILE:
    cargo run -p wscript-cli -- run --debug {{FILE}}

# Start LSP server (stdio)
lsp:
    cargo run -p wscript-cli -- lsp

# Start DAP server
dap PORT="6009":
    cargo run -p wscript-cli -- dap --port {{PORT}}

# Check a file without running
check-file FILE:
    cargo run -p wscript-cli -- check {{FILE}}

# Run all example scripts
examples:
    #!/usr/bin/env bash
    for f in examples/*.ws; do
        echo "=== $f ==="
        cargo run -p wscript-cli -- run "$f" || true
        echo
    done

# Clean build artifacts
clean:
    cargo clean

# Generate and open API documentation
doc:
    cargo doc --workspace --all-features --no-deps --open

# Build the mdbook documentation
book:
    mdbook build docs

# Serve the mdbook documentation with live reload
book-serve:
    mdbook serve docs --open

# Run the hosted example application
example-hosted:
    cargo run -p example-hosted

# Build only the compiler (no runtime/lsp/dap)
build-compiler:
    cargo build -p wscript --no-default-features

# Build + run example-hosted as a fully-static musl binary using `cross`.
# Verifies wscript can be embedded inside a statically-linked Rust binary.
# Requires: `cargo install cross` and a running Docker daemon.
test-musl:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cross >/dev/null 2>&1; then
        echo "error: 'cross' is not installed. Run: cargo install cross --git https://github.com/cross-rs/cross" >&2
        exit 1
    fi
    if ! docker info >/dev/null 2>&1; then
        echo "error: docker daemon is not running (cross needs docker)" >&2
        exit 1
    fi
    TARGET=x86_64-unknown-linux-musl
    echo "=== building example-hosted for $TARGET via cross ==="
    cross build --release --target "$TARGET" -p example-hosted
    BIN="target/$TARGET/release/example-hosted"
    echo "=== verifying binary is statically linked ==="
    file "$BIN"
    if ldd "$BIN" 2>&1 | grep -qv 'statically linked\|not a dynamic'; then
        echo "error: binary appears to have dynamic dependencies" >&2
        ldd "$BIN" || true
        exit 1
    fi
    echo "=== running static musl binary ==="
    "$BIN"
    echo "=== musl integration test OK ==="

# Build-only musl check (faster; does not execute the binary)
test-musl-build:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cross >/dev/null 2>&1; then
        echo "error: 'cross' is not installed. Run: cargo install cross --git https://github.com/cross-rs/cross" >&2
        exit 1
    fi
    cross build --release --target x86_64-unknown-linux-musl -p wscript --features full
    cross build --release --target x86_64-unknown-linux-musl -p example-hosted

# Run a quick smoke test
smoke: build
    #!/usr/bin/env bash
    echo "Smoke test: checking example files..."
    for f in examples/*.ws; do
        echo "  checking $f..."
        cargo run -p wscript-cli -- check "$f" || true
    done
    echo "Done."
