set next # https://github.com/kurtbuilds/just-next

# Default recipe to display available commands
default:
    @just --list

# Run the project with arguments
run *ARGS:
    cargo run -p kurtbuilds-ser -- $ARGS

# Run tests with optional filter
test FILTER="":
    cargo test $FILTER

# Check the project for errors
check:
    cargo check

# Install the binary locally
install:
    cargo install --path cli

# Bump version (major, minor, or patch)
bump LEVEL:
    cargo bump $LEVEL
    VERSION=$(cargo bump get)
    git commit -am "v$VERSION"

# Publish to crates.io and create git tag
publish:
    VERSION=$(cargo bump get)
    git tag "v$VERSION"
    git push origin "v$VERSION"
    # "Published and tagged v$VERSION. Github actions will build release."
