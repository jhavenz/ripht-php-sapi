# Contributing to Ripht PHP SAPI

Thank you for your interest in contributing! This document covers development setup and guidelines.

## Development Setup

### PHP Requirements

This crate requires PHP built with `--enable-embed=static` (and typically `--disable-zts` for NTS builds).

#### Option A: Shared Static PHP CLI/Bia Prefix

[Static PHP CLI](https://github.com/crazywhalecc/static-php-cli) can simplify building PHP with the embed SAPI. For local development, prefer installing the generated prefix at `~/.ripht/php` so multiple Rust/PHP tools can share one static PHP build.

Ripht consumes a PHP installation prefix, not an SPC or Bia source checkout. A compatible prefix contains `lib/libphp.a`; full shim and bindgen validation also use `include/php/`. Keep SPC work/cache directories outside this repo.

Ripht automatically checks `~/.ripht/php` when `RIPHT_PHP_SAPI_PREFIX` is not set. Bia/static-php-cli prefixes should include `lib/bia-link-flags.txt`; Ripht consumes that linker manifest when present. Without the manifest, Ripht falls back to best-effort static dependency scanning from the prefix `lib/` directory.

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
cargo build
```

### Running Tests

Tests must run serially because PHP NTS is not thread-safe:

```bash
RUST_TEST_THREADS=1 cargo test
```

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
