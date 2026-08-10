# ser

A cross-platform CLI tool for managing background services on macOS and Linux systems.

## Features

- **List services**: View all background services with their status
- **Show service details**: Get detailed information about a specific service
- **Start/Stop/Restart services**: Control service execution; `start`/`stop`
  persist across reboots, and `run` fires a job once without changing that
- **Create new services**: Interactive service creation with guided prompts
- **Cross-platform support**: Works on both macOS (launchd) and Linux (systemd)

## Installation

```bash
cargo binstall kurtbuilds-ser
```

## Usage

```bash
# List all services
ser list

# Show details for a specific service
ser show <service-name>

# Start a service or timer, and keep it started across reboots
# (for a timer, this arms its schedule)
ser start <service-name>

# Stop a service or timer, and keep it stopped across reboots
# (for a timer, this disarms its schedule)
ser stop <service-name>

# Restart a service
ser restart <service-name>

# Run a job once, right now, without changing whether it is started
ser run <service-name>

# Create a new service interactively
ser new
```

## Development

This is a Cargo workspace with two crates:
- `ser_lib` - Core library with service management functionality
- `ser_cli` - Command-line interface

This project uses [just](https://github.com/casey/just) as a command runner. Available commands:

```bash
# Build the project
just build

# Run with arguments
just run list

# Run tests
just test

# Format code
just fmt

# Run linter
just clippy

# Run all CI checks
just ci

# Install locally
just install
```

## Dependencies

- `clap` - Command line argument parsing
- `serde` - Serialization/deserialization
- `plist` - Property list support for macOS
- `anyhow` - Error handling
- `tabled` - Table formatting for output
- `atty` - Terminal detection
- `dialoguer` - Interactive prompts

## Platform Support

- **macOS**: Uses launchd for service management
- **Linux**: Uses systemd for service management

## License

[Add your license here]