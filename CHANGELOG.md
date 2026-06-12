# Changelog

## [0.1.0-rc.9] - 2026-06-12

### Added
- `ExecutionResult::exit_status()` exposes PHP's engine exit status after
  script execution.

### Changed
- `ExecutionResult::new(...)` now requires an `exit_status` argument.

## [0.1.0-rc.8] - 2026-05-27

### Added
- `SapiConfig` for configuring SAPI identity and PHP startup behavior
- `RiphtSapi::configure()` to apply custom settings before initialization
- Configurable: sapi_name, pretty_name, server_software, module-level INI
  entries, php.ini path, ignore_php_ini, ignore_cwd_ini
- `SapiError::AlreadyInitialized` and `SapiError::AlreadyConfigured` variants
- `SapiError::InvalidSapiName`, `InvalidPrettyName`, `InvalidServerSoftware`,
  and `InvalidIniPath` variants for per-field config validation

### Changed
- Default sapi_name is `"ripht"` (was `"cli"` in unreleased rc.7 local build).
  Call `RiphtSapi::configure(SapiConfig::new().sapi_name("cli"))` for
  Swoole/OpenSwoole compatibility.

## [0.1.0-rc.*] - 2025-12-21

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
