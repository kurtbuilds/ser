set dotenv-load
set export

# Default recipe to display available commands
default:
    @just --list

# Build the project
build profile="debug":
    cargo build {{ if profile == "release" { "--release" } else { "" } }}

# Build the project in release mode
build-release: (build "release")

# Run the project with arguments
run +args="":
    cargo run -p ser_cli -- {{args}}

# Run tests with optional filter
test filter="":
    cargo test {{filter}}

# Check the project for errors
check:
    cargo check

# Format the code
fmt check="false":
    {{ if check == "true" { "cargo fmt --check" } else { "cargo fmt" } }}

# Run clippy linter with optional flags
clippy +flags="":
    cargo clippy {{flags}}

# Clean build artifacts
clean:
    cargo clean

# Install the binary locally
install:
    cargo install --path cli

# Run the list command
list:
    cargo run -p ser_cli -- list

# Check formatting without applying changes
fmt-check: (fmt "true")

# Run all checks (format, clippy, test)
ci: fmt-check clippy test

# Update dependencies
update:
    cargo update