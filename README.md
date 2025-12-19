# Ripht PHP SAPI

Safe, pragmatic Rust bindings to PHP's Server API (SAPI) for embedding PHP into Rust applications.

The goal: provide a convenience layer to encourage development of additional Rust tooling for PHP.

Status: WIP — this crate is not yet released; I'm very close as of 2025-12-16

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

> **Tip**: Tools like [Static PHP CLI](https://github.com/crazywhalecc/static-php-cli) can simplify building PHP with the embed SAPI. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup options.

## Quick start

Add the crate to your project (when released) or use it locally as a path/git dependency while developing.

Example usage:

For convenience, the crate provides a `prelude` module — import it with `use ripht_php_sapi::prelude::*;` to get commonly used types.

```rust
use ripht_php_sapi::{RiphtSapi, WebRequest};

let sapi = RiphtSapi::instance();
let script = std::path::Path::new("tests/php_scripts/hello.php");
let req = WebRequest::get().build(&script).expect("build failed");
let res = sapi.execute(req).expect("execution failed");

assert_eq!(res.status, 200);
println!("{}", String::from_utf8_lossy(&res.body));
```

You only write safe Rust and don't have to worry about the low-level SAPI details. 

Here's a minimal example that uses a single hook callback to stream output as it arrives:
```rust
use ripht_php_sapi::{RiphtSapi, WebRequest, ExecutionHooks, OutputAction};

struct StreamHooks;
impl ExecutionHooks for StreamHooks {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        // PHP just wrote to the ouput buffer, do something use full here...
        
        OutputAction::Handled
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

- See `examples/` and `tests/php_scripts/` for sample usage and test scripts.

## Contributing

This project is experimental. If you'd like to help, open an issue or a PR — small, focused changes are appreciated.

## License

Apache-2.0
