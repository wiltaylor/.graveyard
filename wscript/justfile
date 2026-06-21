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

# Build the COM test driver as a Windows binary (via `cross` + Docker) and
# run it under Wine. Mirrors the `test-musl` setup so no host-side
# mingw-w64 install is needed — cross-rs's image ships the toolchain.
# Requires: `cargo install cross --git https://github.com/cross-rs/cross`,
# a running Docker daemon, and `wine` on the host PATH.
test-com-wine:
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
    if ! command -v wine >/dev/null 2>&1; then
        echo "error: 'wine' not found on PATH — install wine (e.g. 'sudo pacman -S wine' on Arch)" >&2
        exit 1
    fi
    TARGET=x86_64-pc-windows-gnu
    # Use a dedicated target dir so host-built build-script binaries (linked
    # against host glibc) don't leak into the cross container.
    export CARGO_TARGET_DIR="target-cross"
    echo "=== building wscript-com-test for $TARGET via cross ==="
    cross build --release --target "$TARGET" -p wscript-com-test
    BIN="target-cross/$TARGET/release/wscript-com-test.exe"
    echo "=== running under wine ==="
    WINEDEBUG=-all wine "$BIN"

# Build-only check for the COM Windows target via `cross` (no Wine needed).
# Uses the same Docker-based toolchain as `test-com-wine`, so you don't
# need a host-side mingw-w64 install.
test-com-build:
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
    export CARGO_TARGET_DIR="target-cross"
    cross build --release --target x86_64-pc-windows-gnu -p wscript-com-test

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

# ── CI recipes ──────────────────────────────────────────────────────────
# These wrap CI logic so it stays out of YAML and is reproducible locally.

# Run the full check suite used by CI (fmt, clippy, tests).
ci-check: fmt-check clippy test

# Compute the next release version from git trailers + conventional commits.
# Echoes KEY=VALUE lines on stdout. The workflow appends these to $GITHUB_OUTPUT.
# Outputs: version, do_release.
# Trailers on HEAD commit:
#   release: true      → stable bump
#   pre-release: true  → bump + "-alpha" suffix
#   neither            → version=0.0.0-alpha, do_release=false
ci-version:
    #!/usr/bin/env bash
    set -euo pipefail
    IS_RELEASE=$(git log -1 --format='%(trailers:key=release,valueonly)')
    IS_PRERELEASE=$(git log -1 --format='%(trailers:key=pre-release,valueonly)')

    if [[ "$IS_RELEASE" != "true" && "$IS_PRERELEASE" != "true" ]]; then
        echo "version=0.0.0-alpha"
        echo "do_release=false"
        exit 0
    fi

    LAST_TAG=$(git describe --tags --match 'v*' --abbrev=0 2>/dev/null || echo "")
    if [[ -z "$LAST_TAG" ]]; then
        PREV_MAJOR=0; PREV_MINOR=1; PREV_PATCH=0
        RANGE="HEAD"
    else
        VER="${LAST_TAG#v}"
        VER="${VER%%-*}"
        IFS='.' read -r PREV_MAJOR PREV_MINOR PREV_PATCH <<< "$VER"
        RANGE="${LAST_TAG}..HEAD"
    fi

    BUMP="none"
    while IFS= read -r HASH; do
        SUBJECT=$(git log -1 --format='%s' "$HASH")
        BODY=$(git log -1 --format='%b' "$HASH")
        if echo "$SUBJECT" | grep -qE '^[a-z]+(\(.+\))?!:' || echo "$BODY" | grep -qiE '^BREAKING[ -]CHANGE:'; then
            BUMP="major"
        elif echo "$SUBJECT" | grep -qE '^feat(\(.+\))?:' && [[ "$BUMP" != "major" ]]; then
            BUMP="minor"
        elif echo "$SUBJECT" | grep -qE '^(fix|perf|refactor|build|ci|docs|style|test|chore|revert)(\(.+\))?:' && [[ "$BUMP" == "none" ]]; then
            BUMP="patch"
        fi
    done < <(git log --format='%H' $RANGE)

    [[ "$BUMP" == "none" ]] && BUMP="patch"

    case "$BUMP" in
        major)
            if [[ "$PREV_MAJOR" -eq 0 ]]; then
                MAJOR=0; MINOR=$((PREV_MINOR + 1)); PATCH=0
            else
                MAJOR=$((PREV_MAJOR + 1)); MINOR=0; PATCH=0
            fi ;;
        minor) MAJOR=$PREV_MAJOR; MINOR=$((PREV_MINOR + 1)); PATCH=0 ;;
        patch) MAJOR=$PREV_MAJOR; MINOR=$PREV_MINOR; PATCH=$((PREV_PATCH + 1)) ;;
    esac

    VERSION="${MAJOR}.${MINOR}.${PATCH}"
    [[ "$IS_PRERELEASE" == "true" ]] && VERSION="${VERSION}-alpha"

    echo "version=$VERSION"
    echo "do_release=true"

# Rewrite the workspace version in the root Cargo.toml. Also patches the
# inline `version = "..."` on the wscript path-dep in wscript-cli so the
# crates.io upload sees consistent versions.
ci-set-version VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    sed -i -E 's/^version = "[^"]+"/version = "{{VERSION}}"/' Cargo.toml
    sed -i -E 's|(wscript = \{ path = "\.\./wscript", version = ")[^"]+|\1{{VERSION}}|' crates/wscript-cli/Cargo.toml
    echo "set workspace version to {{VERSION}}"

# Publish wscript and wscript-cli to crates.io. Library first.
# Requires CARGO_REGISTRY_TOKEN in the environment.
ci-publish:
    cargo publish -p wscript --no-verify --allow-dirty
    cargo publish -p wscript-cli --no-verify --allow-dirty

# Tag and create a GitHub release. IS_PRERELEASE must be "true" or "false".
# Requires gh authenticated and GITHUB_TOKEN with contents:write.
ci-release VERSION IS_PRERELEASE:
    #!/usr/bin/env bash
    set -euo pipefail
    git config user.name "github-actions[bot]"
    git config user.email "github-actions[bot]@users.noreply.github.com"
    TAG="v{{VERSION}}"
    git tag "$TAG"
    git push origin "$TAG"
    PRE_FLAG=""
    if [[ "{{IS_PRERELEASE}}" == "true" ]]; then
        PRE_FLAG="--prerelease"
    fi
    gh release create "$TAG" --title "$TAG" --generate-notes $PRE_FLAG

# Run a quick smoke test
smoke: build
    #!/usr/bin/env bash
    echo "Smoke test: checking example files..."
    for f in examples/*.ws; do
        echo "  checking $f..."
        cargo run -p wscript-cli -- check "$f" || true
    done
    echo "Done."
