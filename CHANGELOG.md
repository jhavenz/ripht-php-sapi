# Changelog

## [0.1.0-rc.1] - 2025-12-21

Initial release candidate.

### Features

- Safe Rust bindings to PHP's Server API (SAPI)
- Execute PHP scripts from Rust with full request lifecycle management
- Web and CLI request builders for different execution contexts
- Execution hooks for streaming output and custom processing
- Comprehensive error handling and message capture
- Support for INI overrides, environment variables, and custom headers

### API

- `RiphtSapi`: Main SAPI instance for script execution
- `WebRequest` / `CliRequest`: Request builders for different contexts
- `ExecutionContext`: Builder for execution parameters
- `ExecutionResult`: Result containing status, headers, body, and messages
- `ExecutionHooks`: Trait for customizing execution behavior
