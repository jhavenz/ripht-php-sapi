# ripht-php-sapi

[![Crates.io](https://img.shields.io/crates/v/ripht-php-sapi)](https://crates.io/crates/ripht-php-sapi)
[![docs.rs](https://docs.rs/ripht-php-sapi/badge.svg)](https://docs.rs/ripht-php-sapi)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Safe, pragmatic Rust bindings for embedding PHP via the embed SAPI.

## Why ripht-php-sapi?
1) Provide tooling that will allow additional PHP tooling to be built in Rust.

2) I hadn't seen another Rust crate offering comparable features.

3) I'm planning to build more tooling on this (stay tuned...).

## Requirements

This crate requires PHP built with the embed SAPI as a static library:

```bash
./configure --enable-embed=static --disable-zts [other options...]
make && make install
```

Set `RIPHT_PHP_SAPI_PREFIX` to your PHP installation root containing:

- `lib/libphp.a` (PHP embed SAPI)
- `include/php/` (PHP headers)

Or install to one of the default fallback locations: `~/.ripht/php`, `~/.local/php`, or `/usr/local`.
    
> Important Notes:
> This crate is focuses on the non-ZTS build of PHP. 
> There aren't currently plans to support ZTS builds.
>
> Tip: Tools like [Static PHP CLI](https://github.com/crazywhalecc/static-php-cli) can simplify building PHP with the embed SAPI. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup options.

## Quick start

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ripht-php-sapi = "0.1.0-rc.4"
```

Example usage:

For convenience, the crate provides a `prelude` module — import it with `use ripht_php_sapi::prelude::*;` to get commonly used types.

### Web Example

Simulate an HTTP request. This populates `$_GET`, `$_SERVER`, etc.

```rust
use ripht_php_sapi::prelude::*;

let sapi = RiphtSapi::instance();
let script = std::path::Path::new("index.php");

let req = WebRequest::get()
    .with_query_param("id", "123")
    .with_header("User-Agent", "Ripht")
    .build(&script)
    .expect("build failed");

let res = sapi.execute(req).expect("execution failed");

assert_eq!(res.status_code(), 200);
println!("{}", res.body_string());
```

### CLI Example

Run a script as if from the command line. This sets `argc`/`argv` and avoids HTTP superglobals.

```rust
use ripht_php_sapi::prelude::*;

let sapi = RiphtSapi::instance();
let script = std::path::Path::new("script.php");

let req = CliRequest::new()
    .with_arg("my-argument")
    .with_env("MY_ENV_VAR", "value")
    .build(&script)
    .expect("build failed");

let res = sapi.execute(req).expect("execution failed");

println!("{}", res.body_string());
```

You only write safe Rust and don't have to worry about the low-level SAPI details.

Here's a minimal example that uses a single hook callback to stream output as it arrives:

```rust
use ripht_php_sapi::{RiphtSapi, WebRequest, ExecutionHooks, OutputAction};

struct StreamHooks;
impl ExecutionHooks for StreamHooks {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        // Do something with the PHP output here...

        OutputAction::Done
    }
}

sapi.execute_with_hooks(ctx, StreamHooks).expect("execution failed");
```

## Development notes

- The build script expects a PHP build root that contains `lib/libphp.a` (static embed SAPI) and headers. Set `RIPHT_PHP_SAPI_PREFIX` to point at your PHP build prefix if necessary.
- Example debug/run helpers and bench configuration are in `.cargo/config.toml.example`.
- Benchmarks use Criterion and live under `benches/`. Run them with:

```bash
cargo bench --bench sapi_comparison
```

To compare against external servers (php-fpm/FrankenPHP), set environment variables before running the bench, e.g.:

_Note: you'll need to have the frankenphp and php-fpm builds setup before running this. Reach out if you have questions_

```bash
BENCH_COMPARE=1 \
    BENCH_FPM_BIN=/path/to/php-fpm \
    BENCH_FRANKENPHP_BIN=/path/to/frankenphp \
    cargo bench --bench sapi_comparison
```

## Examples

See `examples/` and `tests/php_scripts/` for sample usage and test scripts.

## Support Development
This project is part of a larger educational initiative about PHP internals and Rust FFI.
- [Vote on educational content direction](https://www.patreon.com/posts/gauging-php-sapi-146489023)
- [Support on Patreon](https://www.patreon.com/cw/jhavenz)

## Contributing

If you'd like to help, open an issue or a PR — small, focused changes are appreciated.

## License

MIT
