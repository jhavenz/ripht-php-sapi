# Contributing to Ripht PHP SAPI

Thank you for your interest in contributing! This document covers development setup and guidelines.

## Development Setup

### PHP Requirements

This crate requires PHP built with `--enable-embed=static` (and typically `--disable-zts` for NTS builds).

#### Option A: Static PHP CLI (Optional convenience for development)

[Static PHP CLI](https://github.com/crazywhalecc/static-php-cli) can simplify building PHP with the embed SAPI (optional):

```bash
# Install spc (macOS example)
curl -fsSL https://dl.static-php.dev/spc-bin/nightly/spc-macos-x86_64 -o spc
chmod +x spc

# Build PHP with embed SAPI (adjust flags as needed)
./spc doctor --auto-fix
./spc download --with-php=8.3 --for-extensions=... # install desired extensions
./spc build php-src --build-embed

# Set the prefix (adjust path based on your spc output)
# e.g., export RIPHT_PHP_SAPI_PREFIX=$HOME/.spc/buildroot
```

#### Option B: Manual PHP Build

```bash
git clone https://github.com/php/php-src.git
cd php-src
git checkout php-8.3.14  # or your desired version

./buildconf
./configure \
    --enable-embed=static \
    --disable-zts \
    --disable-phpdbg \
    --disable-cgi \
    --enable-bcmath \
    --enable-opcache \
    --with-openssl \
    --with-zlib \
    --prefix=$HOME/.ripht/php

make -j$(nproc)
make install

export RIPHT_PHP_SAPI_PREFIX=$HOME/.ripht/php
```

### Building the Crate

```bash
# Ensure RIPHT_PHP_SAPI_PREFIX is set, or install PHP to a fallback location
cargo build
```

### Running Tests

Tests must run serially because PHP NTS is not thread-safe:

```bash
cargo test
```

The `.cargo/config.toml` already sets `RUST_TEST_THREADS=1`.

### Running Examples

```bash
cargo run --example basic_execution
cargo run --example http_server
```

### Running Benchmarks

```bash
cargo bench --bench sapi_comparison
```

To compare against external PHP servers:

```bash
BENCH_COMPARE=1 \
    BENCH_FPM_BIN=/path/to/php-fpm \
    BENCH_FRANKENPHP_BIN=/path/to/frankenphp \
    cargo bench --bench sapi_comparison
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Follow existing patterns in the codebase

## Pull Requests

- Keep changes focused and small
- Add tests for new functionality
- Update documentation as needed
- Ensure all tests pass before submitting

## Questions?

Open an issue if you have questions about the codebase or need help with setup.
