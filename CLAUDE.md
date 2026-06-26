# Claude Development Notes

This file contains development notes and commands for Claude Code to help with project maintenance.

## Project Overview

`ser` is a cross-platform CLI tool for managing background services, supporting both macOS (launchd) and Linux (systemd).

This is a Cargo workspace with two crates:
- `serlib` (in `lib/`) - Core library containing platform-specific service management
- `ser` CLI (in `cli/`) - Command-line interface that uses serlib

## Key Commands

### Build and Test
- `just build` - Build the project
- `just build-release` - Build in release mode
- `just test` - Run tests
- `just check` - Check for errors
- `just ci` - Run all CI checks (format, clippy, test)

### Code Quality
- `just fmt` - Format code
- `just fmt-check` - Check formatting without applying changes
- `just clippy` - Run clippy linter

### Development
- `just run <args>` - Run with arguments (uses the cli package)
- `just list` - Quick test of list command
- `just install` - Install binary locally (installs from cli)
- `just clean` - Clean build artifacts
- `just update` - Update dependencies

## Project Structure

### cli crate (`ser`)
- `cli/src/main.rs` - Main entry point with CLI definition
- `cli/src/command/` - Command implementations

### lib crate (`serlib`)
- `lib/src/lib.rs` - Library entry point
- `lib/src/platform/` - Platform-specific service management
  - `macos.rs` - macOS/launchd implementation
  - `linux.rs` - Linux/systemd implementation

## Testing

Run tests across the workspace with `just test` or `cargo test`.

## Dependencies

Core dependencies:
- `clap` (4.4) - CLI framework with derive features
- `serde` (1.0) - Serialization with derive features
- `plist` (1.6) - macOS property list support
- `anyhow` (1.0) - Error handling
- `tabled` (0.15) - Table formatting
- `atty` (0.2) - Terminal detection
- `dialoguer` (0.11) - Interactive prompts

## Development Notes

- The project uses platform-specific conditional compilation for macOS and Linux
- Service management abstractions are in the `platform` module
- Interactive service creation is handled through the `New` command
- All commands implement a `run()` method that returns `anyhow::Result<()>`