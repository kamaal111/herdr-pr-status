alias fmt := format
alias fmt-c := format-check

@_default:
    just --list --unsorted

build:
    cargo build --locked

build-release:
    cargo build --locked --release --target-dir target

format:
    cargo fmt --all

format-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --locked --all-targets --all-features -- -D warnings

test:
    cargo test --locked --all-features
    uv run --locked pytest

quality: format-check lint

ready: quality test

plugin-link: build-release
    "${HERDR_BIN_PATH:-herdr}" plugin link "$PWD"

plugin-check: build-release
    ./target/release/herdr-pr-status check

plugin-start: build-release
    "${HERDR_BIN_PATH:-herdr}" plugin action invoke herdr-pr-status.start

plugin-refresh: build-release
    "${HERDR_BIN_PATH:-herdr}" plugin action invoke herdr-pr-status.refresh

plugin-stop:
    "${HERDR_BIN_PATH:-herdr}" plugin action invoke herdr-pr-status.stop

plugin-unlink:
    "${HERDR_BIN_PATH:-herdr}" plugin unlink herdr-pr-status
