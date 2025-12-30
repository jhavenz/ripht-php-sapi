pub mod cli;
pub mod web;

use std::path::Path;

pub use cli::{CliRequest, CliRequestError};
pub use web::{Method, WebRequest, WebRequestError};

#[cfg(feature = "http")]
pub use web::{from_http_parts, from_http_request};

use crate::ExecutionContext;

/// Common error type for all SAPI adapters.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AdapterError {
    /// Script file was not found.
    ScriptNotFound(std::path::PathBuf),
    /// Required configuration is missing.
    MissingConfiguration(String),
    /// Invalid configuration value.
    InvalidConfiguration {
        field: String,
        value: String,
        reason: String,
    },
    /// Web-specific errors.
    Web(WebRequestError),
    /// CLI-specific errors.
    Cli(CliRequestError),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScriptNotFound(path) => {
                write!(f, "Script not found: {}", path.display())
            }
            Self::MissingConfiguration(field) => {
                write!(f, "Missing required configuration: {}", field)
            }
            Self::InvalidConfiguration {
                field,
                value,
                reason,
            } => {
                write!(
                    f,
                    "Invalid configuration for '{}' = '{}': {}",
                    field, value, reason
                )
            }
            Self::Web(err) => write!(f, "Web adapter error: {}", err),
            Self::Cli(err) => write!(f, "CLI adapter error: {}", err),
        }
    }
}

impl std::error::Error for AdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Web(err) => Some(err),
            Self::Cli(err) => Some(err),
            _ => None,
        }
    }
}

impl From<WebRequestError> for AdapterError {
    fn from(err: WebRequestError) -> Self {
        Self::Web(err)
    }
}

impl From<CliRequestError> for AdapterError {
    fn from(err: CliRequestError) -> Self {
        Self::Cli(err)
    }
}

/// Trait for types that can be converted into PHP execution contexts.
///
/// This trait provides a unified interface for building `ExecutionContext` instances
/// from different request types (Web, CLI, custom adapters). Implementors configure
/// their internal state through builder methods, then call `build()` to create
/// the final execution context.
///
/// # Design Goals
///
/// - **Unified Interface**: All adapters share the same `build()` signature
/// - **Type Safety**: Strong typing prevents configuration errors at compile time
/// - **Flexibility**: Adapters can have adapter-specific configuration while sharing common patterns
/// - **Error Handling**: Consistent error types across all adapters
///
/// # Examples
///
/// ```no_run
/// use ripht_php_sapi::{WebRequest, CliRequest, adapters::PhpSapiAdapter};
/// use std::path::Path;
///
/// let script = Path::new("/var/www/script.php");
///
/// // Web adapter
/// let web_ctx = WebRequest::get()
///     .with_uri("/api/test")
///     .build(script)?;
///
/// // CLI adapter  
/// let cli_ctx = CliRequest::new()
///     .with_arg("--verbose")
///     .build(script)?;
///
/// // Both return ExecutionContext through the same interface
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Implementation Requirements
///
/// Implementors must:
/// 1. Validate configuration before building
/// 2. Handle script path existence checking
/// 3. Return appropriate error types
/// 4. Ensure the resulting `ExecutionContext` is valid for execution
///
/// # Validation Patterns
///
/// The trait provides default validation methods that implementors can use:
///
/// ```ignore
/// fn build(self, script_path: impl AsRef<Path>) -> Result<ExecutionContext, AdapterError> {
///     let path = Self::validate_script_path(script_path)?;
///     // ... adapter-specific building logic
/// }
/// ```
pub trait PhpSapiAdapter {
    /// Build an execution context from the configured adapter.
    ///
    /// This method consumes the adapter and produces an `ExecutionContext` ready
    /// for execution. The script path is validated for existence, and all adapter
    /// configuration is converted into the appropriate server variables, environment
    /// variables, and execution parameters.
    ///
    /// # Arguments
    ///
    /// * `script_path` - Path to the PHP script to execute. Must exist and be readable.
    ///
    /// # Returns
    ///
    /// - `Ok(ExecutionContext)` - Ready to execute
    /// - `Err(AdapterError)` - Configuration or validation error
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// - The script file does not exist
    /// - Required configuration is missing
    /// - Configuration values are invalid
    /// - Adapter-specific validation fails
    fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, AdapterError>;

    /// Validate that a script path exists and is accessible.
    ///
    /// This is a utility method that adapters can use for consistent path validation.
    /// It converts the path to an absolute path when possible and checks for existence.
    ///
    /// # Arguments
    ///
    /// * `script_path` - Path to validate
    ///
    /// # Returns
    ///
    /// - `Ok(PathBuf)` - Validated, absolute path
    /// - `Err(AdapterError::ScriptNotFound)` - Path does not exist
    fn validate_script_path(
        script_path: impl AsRef<Path>,
    ) -> Result<std::path::PathBuf, AdapterError>
    where
        Self: Sized,
    {
        let path = script_path
            .as_ref()
            .to_path_buf();

        if !path.exists() {
            return Err(AdapterError::ScriptNotFound(path));
        }

        // Try to canonicalize, but fall back to original path if it fails
        Ok(std::fs::canonicalize(&path).unwrap_or(path))
    }

    /// Validate a configuration field is not empty.
    ///
    /// Utility method for validating string configuration fields.
    ///
    /// # Arguments
    ///
    /// * `field_name` - Name of the field for error reporting
    /// * `value` - Value to validate
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Value is non-empty
    /// - `Err(AdapterError::MissingConfiguration)` - Value is empty
    fn validate_non_empty(
        field_name: &str,
        value: &str,
    ) -> Result<(), AdapterError>
    where
        Self: Sized,
    {
        if value.is_empty() {
            Err(AdapterError::MissingConfiguration(field_name.to_string()))
        } else {
            Ok(())
        }
    }

    /// Validate a configuration field against a predicate.
    ///
    /// Utility method for custom field validation.
    ///
    /// # Arguments
    ///
    /// * `field_name` - Name of the field for error reporting
    /// * `value` - Value to validate
    /// * `predicate` - Validation function
    /// * `error_reason` - Reason for validation failure
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Validation passed
    /// - `Err(AdapterError::InvalidConfiguration)` - Validation failed
    fn validate_field<T, F>(
        field_name: &str,
        value: &T,
        predicate: F,
        error_reason: &str,
    ) -> Result<(), AdapterError>
    where
        Self: Sized,
        T: std::fmt::Display,
        F: FnOnce(&T) -> bool,
    {
        if predicate(value) {
            Ok(())
        } else {
            Err(AdapterError::InvalidConfiguration {
                field: field_name.to_string(),
                value: value.to_string(),
                reason: error_reason.to_string(),
            })
        }
    }
}

// Implementations for existing adapters
impl PhpSapiAdapter for WebRequest {
    fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, AdapterError> {
        self.build(script_path)
            .map_err(AdapterError::from)
    }
}

impl PhpSapiAdapter for CliRequest {
    fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, AdapterError> {
        self.build(script_path)
            .map_err(AdapterError::from)
    }
}

// AdapterError and PhpSapiAdapter are already defined in this module and public

#[cfg(test)]
mod tests;

/// Example implementations demonstrating the `PhpSapiAdapter` trait pattern.
///
/// This module provides examples of how to create custom adapters that implement
/// the `PhpSapiAdapter` trait. These examples are for documentation purposes
/// and demonstrate best practices for adapter implementation.
#[cfg(doc)]
pub mod examples {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A minimal custom adapter demonstrating basic trait implementation.
    ///
    /// This example shows how to implement `PhpSapiAdapter` for a custom adapter
    /// with simple configuration validation.
    ///
    /// ```
    /// # use ripht_php_sapi::adapters::{PhpSapiAdapter, examples::MinimalAdapter};
    /// # use std::path::Path;
    ///
    /// // Create and configure adapter
    /// let adapter = MinimalAdapter::new()
    ///     .with_name("test-app")
    ///     .with_version("1.0.0");
    ///
    /// // Build execution context (requires a valid PHP script)
    /// # let script_path = Path::new("tests/php_scripts/hello.php");
    /// # if script_path.exists() {
    /// let context = adapter.build(script_path)?;
    /// # }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub struct MinimalAdapter {
        app_name: Option<String>,
        app_version: String,
    }

    impl MinimalAdapter {
        /// Create a new minimal adapter with default settings.
        pub fn new() -> Self {
            Self {
                app_name: None,
                app_version: "1.0.0".to_string(),
            }
        }

        /// Set the application name.
        pub fn with_name(mut self, name: impl Into<String>) -> Self {
            self.app_name = Some(name.into());
            self
        }

        /// Set the application version.
        pub fn with_version(mut self, version: impl Into<String>) -> Self {
            self.app_version = version.into();
            self
        }
    }

    impl PhpSapiAdapter for MinimalAdapter {
        fn build(
            self,
            script_path: impl AsRef<Path>,
        ) -> Result<crate::ExecutionContext, AdapterError> {
            // Validate script path using trait utility
            let validated_path = Self::validate_script_path(script_path)?;

            // Validate required fields
            let app_name = self.app_name.ok_or_else(|| {
                AdapterError::MissingConfiguration("app_name".to_string())
            })?;

            // Validate app name is not empty
            Self::validate_non_empty("app_name", &app_name)?;

            // Build execution context
            Ok(crate::ExecutionContext::script(validated_path)
                .var("APP_NAME", app_name)
                .var("APP_VERSION", self.app_version)
                .env("APPLICATION_ENV", "custom"))
        }
    }

    /// A more complex adapter showing advanced validation and configuration.
    ///
    /// This example demonstrates:
    /// - Complex field validation with predicates
    /// - Multiple configuration sources
    /// - Environment variable handling
    /// - INI override patterns
    ///
    /// ```
    /// # use ripht_php_sapi::adapters::{PhpSapiAdapter, examples::AdvancedAdapter};
    /// # use std::path::Path;
    ///
    /// let adapter = AdvancedAdapter::new()
    ///     .with_database_url("postgres://localhost:5432/mydb")
    ///     .with_max_connections(50)
    ///     .with_debug_mode(true)
    ///     .with_env("CUSTOM_VAR", "value");
    ///
    /// # let script_path = Path::new("tests/php_scripts/hello.php");
    /// # if script_path.exists() {
    /// let context = adapter.build(script_path)?;
    /// # }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub struct AdvancedAdapter {
        database_url: Option<String>,
        max_connections: u32,
        debug_mode: bool,
        env_vars: Vec<(String, String)>,
        working_dir: Option<PathBuf>,
    }

    impl AdvancedAdapter {
        /// Create a new advanced adapter with production defaults.
        pub fn new() -> Self {
            Self {
                database_url: None,
                max_connections: 10,
                debug_mode: false,
                env_vars: Vec::new(),
                working_dir: None,
            }
        }

        /// Set the database connection URL.
        pub fn with_database_url(mut self, url: impl Into<String>) -> Self {
            self.database_url = Some(url.into());
            self
        }

        /// Set the maximum database connections.
        pub fn with_max_connections(mut self, max: u32) -> Self {
            self.max_connections = max;
            self
        }

        /// Enable or disable debug mode.
        pub fn with_debug_mode(mut self, debug: bool) -> Self {
            self.debug_mode = debug;
            self
        }

        /// Add an environment variable.
        pub fn with_env(
            mut self,
            key: impl Into<String>,
            value: impl Into<String>,
        ) -> Self {
            self.env_vars
                .push((key.into(), value.into()));
            self
        }

        /// Set the working directory.
        pub fn with_working_dir(mut self, path: impl Into<PathBuf>) -> Self {
            self.working_dir = Some(path.into());
            self
        }
    }

    impl PhpSapiAdapter for AdvancedAdapter {
        fn build(
            self,
            script_path: impl AsRef<Path>,
        ) -> Result<crate::ExecutionContext, AdapterError> {
            // Validate script path
            let validated_path = Self::validate_script_path(script_path)?;

            // Validate database URL if provided
            if let Some(ref db_url) = self.database_url {
                Self::validate_field(
                    "database_url",
                    db_url,
                    |url| {
                        url.starts_with("postgres://")
                            || url.starts_with("mysql://")
                    },
                    "must start with postgres:// or mysql://",
                )?;
            }

            // Validate max connections
            Self::validate_field(
                "max_connections",
                &self.max_connections,
                |&max| max > 0 && max <= 1000,
                "must be between 1 and 1000",
            )?;

            // Build execution context
            let mut ctx = crate::ExecutionContext::script(validated_path)
                .var(
                    "MAX_CONNECTIONS",
                    self.max_connections
                        .to_string(),
                )
                .var("DEBUG_MODE", if self.debug_mode { "1" } else { "0" })
                .ini("log_errors", if self.debug_mode { "1" } else { "0" })
                .ini("display_errors", if self.debug_mode { "1" } else { "0" });

            // Add database URL if configured
            if let Some(db_url) = self.database_url {
                ctx = ctx.var("DATABASE_URL", db_url);
            }

            // Add custom environment variables
            ctx = ctx.envs(self.env_vars);

            // Set working directory if specified
            if let Some(wd) = self.working_dir {
                ctx = ctx.env("PWD", wd.to_string_lossy());
            }

            Ok(ctx)
        }
    }
}
