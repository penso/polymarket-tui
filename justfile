# Default recipe (runs when just is called without arguments)
default:
    @just --list

# Format Rust code
format:
    cargo fmt

# Check if code is formatted
format-check:
    cargo fmt -- --check

# Lint Rust code using clippy
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Build the project
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

