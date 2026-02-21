# Default recipe - show available commands
default:
    @just --list

# Run tests
test:
    cargo test

# Run clippy
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Install binary to ~/.local/bin
install:
    cargo install --path . --root ~/.local --force

# Release and install a new version (patch/minor/major)
release level:
    cargo release {{level}} --execute --no-confirm
    cargo install --path . --root ~/.local --force
