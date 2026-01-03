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
    cargo run -p kurtbuilds-ser -- {{args}}

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
    cargo run -p kurtbuilds-ser -- list

# Check formatting without applying changes
fmt-check: (fmt "true")

# Run all checks (format, clippy, test)
ci: fmt-check clippy test

# Update dependencies
update:
    cargo update

# Bump version (major, minor, or patch)
bump level:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "$(git status --porcelain)" ]; then
        echo "Error: Working directory is not clean. Commit or stash changes first."
        exit 1
    fi
    current=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    case "{{level}}" in
        major) major=$((major + 1)); minor=0; patch=0 ;;
        minor) minor=$((minor + 1)); patch=0 ;;
        patch) patch=$((patch + 1)) ;;
        *) echo "Usage: just bump <major|minor|patch>"; exit 1 ;;
    esac
    new="$major.$minor.$patch"
    sed -i '' "s/^version = \"$current\"/version = \"$new\"/" Cargo.toml
    # Also update the workspace dependency version for kurtbuilds-serlib
    sed -i '' "s/kurtbuilds-serlib = { version = \"$current\"/kurtbuilds-serlib = { version = \"$new\"/" Cargo.toml
    git add .
    git commit -m "v$new"
    echo "Bumped version: $current -> $new"

# Publish to crates.io and create git tag
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    version=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    cargo publish -p kurtbuilds-serlib --allow-dirty
    cargo publish -p kurtbuilds-ser --allow-dirty
    git tag "v$version"
    git push origin "v$version"
    echo "Published and tagged v$version"
