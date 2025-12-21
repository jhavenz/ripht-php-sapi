# Ripht PHP SAPI

Safe, pragmatic Rust bindings to PHP's Server API (SAPI) for embedding PHP into Rust applications.

The goal: provide a convenience layer to encourage development of additional Rust tooling for PHP.

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

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ripht-php-sapi = "0.1.0-rc.1"
```

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
        // Do something with the PHP output here...

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

See `examples/` and `tests/php_scripts/` for sample usage and test scripts.

## Support

I have plans to do much more, this crate serves as the foundation of a larger toolchain I have in mind. I'd love to do it full time. 

Your support on [Patreon](https://patreon.com/jhavenz) would be greatly appreciated.

Provide feedback on additional PHP SAPI learning material [here](https://www.patreon.com/posts/gauging-php-sapi-146489023)

## Learning Material

Building this SAPI required diving deep into PHP internals, Rust FFI, and the patterns that connect them. I'm working on educational material to share what I've discovered along the way.

See the summary I've put together [here](docs/leanpub/summary.md). This is the intro for a book I'm planning to put together on Leanpub. I'll be writing the initial chapters and a couple pages of each chapter to publish to get a feel for public interest on this.

Feel free to get involved in the discussions portion of this repo with regards to this.

## Contributing

If you'd like to help, open an issue or a PR — small, focused changes are appreciated.

## License

MIT
