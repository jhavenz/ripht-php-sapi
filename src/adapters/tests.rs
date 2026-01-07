use super::*;
use std::error::Error;
use std::path::PathBuf;

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}

fn nonexistent_script_path() -> PathBuf {
    PathBuf::from("/definitely/does/not/exist/script.php")
}

#[derive(Debug, Clone)]
struct TestAdapter {
    required_field: Option<String>,
    optional_port: u16,
    allow_empty_required: bool,
}

impl TestAdapter {
    fn new() -> Self {
        Self {
            required_field: None,
            optional_port: 8080,
            allow_empty_required: false,
        }
    }

    fn with_required_field(mut self, value: impl Into<String>) -> Self {
        self.required_field = Some(value.into());
        self
    }

    fn with_port(mut self, port: u16) -> Self {
        self.optional_port = port;
        self
    }

    fn allow_empty_required(mut self) -> Self {
        self.allow_empty_required = true;
        self
    }
}
impl PhpSapiAdapter for TestAdapter {
    fn build(
        self,
        script_path: impl AsRef<Path>,
    ) -> Result<ExecutionContext, AdapterError> {
        let validated_path = Self::validate_script_path(script_path)?;

        let required = self
            .required_field
            .ok_or_else(|| {
                AdapterError::MissingConfiguration("required_field".to_string())
            })?;

        if !self.allow_empty_required {
            Self::validate_non_empty("required_field", &required)?;
        }

        Self::validate_field(
            "port",
            &self.optional_port,
            |&port| port > 0,
            "must be greater than 0",
        )?;

        Ok(ExecutionContext::script(validated_path)
            .var("TEST_REQUIRED", required)
            .var("TEST_PORT", self.optional_port.to_string()))
    }
}

#[test]
fn test_adapter_error_display() {
    let path = PathBuf::from("/test/path.php");

    let errors = vec![
        (
            AdapterError::ScriptNotFound(path.clone()),
            "Script not found: /test/path.php",
        ),
        (
            AdapterError::MissingConfiguration("field_name".to_string()),
            "Missing required configuration: field_name",
        ),
        (
            AdapterError::InvalidConfiguration {
                field: "port".to_string(),
                value: "99999".to_string(),
                reason: "too large".to_string(),
            },
            "Invalid configuration for 'port' = '99999': too large",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn test_adapter_error_from_web_error() {
    let web_error = WebRequestError::MissingMethod;
    let adapter_error: AdapterError = web_error.clone().into();

    match adapter_error {
        AdapterError::Web(err) => {
            assert_eq!(format!("{}", err), format!("{}", web_error));
        }
        _ => panic!("Expected Web variant"),
    }
}

#[test]
fn test_adapter_error_from_cli_error() {
    let path = PathBuf::from("/test.php");
    let cli_error = CliRequestError::ScriptNotFound(path.clone());
    let adapter_error: AdapterError = cli_error.clone().into();

    match adapter_error {
        AdapterError::Cli(err) => {
            assert_eq!(format!("{}", err), format!("{}", cli_error));
        }
        _ => panic!("Expected Cli variant"),
    }
}

#[test]
fn test_adapter_error_source() {
    let web_error = WebRequestError::InvalidMethod("INVALID".to_string());
    let adapter_error = AdapterError::Web(web_error.clone());

    assert!(adapter_error
        .source()
        .is_some());

    let missing_config = AdapterError::MissingConfiguration("test".to_string());
    assert!(missing_config
        .source()
        .is_none());
}

#[test]
fn test_validate_script_path_existing() {
    let script_path = php_script_path("hello.php");

    let result = TestAdapter::validate_script_path(&script_path);
    assert!(result.is_ok());

    let validated_path = result.unwrap();
    assert!(validated_path.exists());
    assert!(validated_path.is_absolute() || validated_path == script_path);
}

#[test]
fn test_validate_script_path_nonexistent() {
    let script_path = nonexistent_script_path();

    let result = TestAdapter::validate_script_path(&script_path);
    assert!(result.is_err());

    match result.unwrap_err() {
        AdapterError::ScriptNotFound(path) => {
            assert_eq!(path, script_path);
        }
        _ => panic!("Expected ScriptNotFound error"),
    }
}

#[test]
fn test_validate_non_empty_valid() {
    let result = TestAdapter::validate_non_empty("test_field", "valid_value");
    assert!(result.is_ok());
}

#[test]
fn test_validate_non_empty_invalid() {
    let result = TestAdapter::validate_non_empty("test_field", "");
    assert!(result.is_err());

    match result.unwrap_err() {
        AdapterError::MissingConfiguration(field) => {
            assert_eq!(field, "test_field");
        }
        _ => panic!("Expected MissingConfiguration error"),
    }
}

#[test]
fn test_validate_field_success() {
    let result = TestAdapter::validate_field(
        "port",
        &8080u16,
        |&port| port > 1000 && port < 9000,
        "must be between 1000 and 9000",
    );
    assert!(result.is_ok());
}

#[test]
fn test_validate_field_failure() {
    let result = TestAdapter::validate_field(
        "port",
        &65535u16,
        |&port| port < 65535,
        "must be less than 65535",
    );
    assert!(result.is_err());

    match result.unwrap_err() {
        AdapterError::InvalidConfiguration {
            field,
            value,
            reason,
        } => {
            assert_eq!(field, "port");
            assert_eq!(value, "65535");
            assert_eq!(reason, "must be less than 65535");
        }
        _ => panic!("Expected InvalidConfiguration error"),
    }
}

#[test]
fn test_custom_adapter_success() {
    let script_path = php_script_path("hello.php");

    let context = TestAdapter::new()
        .with_required_field("test_value")
        .with_port(3000)
        .build(&script_path)
        .expect("should build successfully");

    assert_eq!(context.script_path, script_path);

    let has_required = context
        .server_vars
        .iter()
        .any(|(k, _)| k == "TEST_REQUIRED");
    let has_port = context
        .server_vars
        .iter()
        .any(|(k, _)| k == "TEST_PORT");

    assert!(has_required, "TEST_REQUIRED should be set");
    assert!(has_port, "TEST_PORT should be set");
}

#[test]
fn test_custom_adapter_missing_required() {
    let script_path = php_script_path("hello.php");

    let result = TestAdapter::new()
        .with_port(3000)
        .build(&script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::MissingConfiguration(field) => {
            assert_eq!(field, "required_field");
        }
        _ => panic!("Expected MissingConfiguration error"),
    }
}

#[test]
fn test_custom_adapter_empty_required() {
    let script_path = php_script_path("hello.php");

    let result = TestAdapter::new()
        .with_required_field("")
        .build(&script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::MissingConfiguration(field) => {
            assert_eq!(field, "required_field");
        }
        _ => panic!("Expected MissingConfiguration error"),
    }

    let result = TestAdapter::new()
        .with_required_field("")
        .allow_empty_required()
        .build(&script_path);

    assert!(result.is_ok());
}

#[test]
fn test_custom_adapter_invalid_port() {
    let script_path = php_script_path("hello.php");
    let invalid_port = 0;

    let result = TestAdapter::new()
        .with_required_field("test")
        .with_port(invalid_port)
        .build(&script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::InvalidConfiguration {
            field,
            value,
            reason,
        } => {
            assert_eq!(field, "port");
            assert_eq!(value, "0");
            assert_eq!(reason, "must be greater than 0");
        }
        _ => panic!("Expected InvalidConfiguration error"),
    }
}

#[test]
fn test_custom_adapter_nonexistent_script() {
    let script_path = nonexistent_script_path();

    let result = TestAdapter::new()
        .with_required_field("test")
        .build(&script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::ScriptNotFound(path) => {
            assert_eq!(path, script_path);
        }
        _ => panic!("Expected ScriptNotFound error"),
    }
}

#[test]
fn test_web_adapter_trait_implementation() {
    let script_path = php_script_path("hello.php");

    let context = WebRequest::get()
        .with_uri("/test")
        .build(&script_path)
        .expect("should build successfully");

    assert_eq!(context.script_path, script_path);
    assert!(!context.log_to_stderr); 
}

#[test]
fn test_web_adapter_trait_error_conversion() {
    let script_path = nonexistent_script_path();

    let result: Result<ExecutionContext, AdapterError> =
        PhpSapiAdapter::build(WebRequest::get(), &script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Web(WebRequestError::ScriptNotFound(path)) => {
            assert_eq!(path, script_path);
        }
        _ => panic!("Expected Web(ScriptNotFound) error"),
    }
}

#[test]
fn test_web_adapter_trait_missing_method() {
    let script_path = php_script_path("hello.php");

    let web_request = WebRequest::default();
    let result: Result<ExecutionContext, AdapterError> =
        PhpSapiAdapter::build(web_request, &script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Web(WebRequestError::MissingMethod) => {
            // Expected
        }
        _ => panic!("Expected Web(MissingMethod) error"),
    }
}

#[test]
fn test_cli_adapter_trait_implementation() {
    let script_path = php_script_path("hello.php");

    let context = CliRequest::new()
        .with_arg("--test")
        .build(&script_path)
        .expect("should build successfully");

    assert_eq!(context.script_path, script_path);
    assert!(context.log_to_stderr); 
}

#[test]
fn test_cli_adapter_trait_error_conversion() {
    let script_path = nonexistent_script_path();

    let result: Result<ExecutionContext, AdapterError> =
        PhpSapiAdapter::build(CliRequest::new(), &script_path);

    assert!(result.is_err());
    match result.unwrap_err() {
        AdapterError::Cli(CliRequestError::ScriptNotFound(path)) => {
            assert_eq!(path, script_path);
        }
        _ => panic!("Expected Cli(ScriptNotFound) error"),
    }
}

// Note: trait objects not supported due to Self: Sized constraints on utility methods
fn execute_adapter<A: PhpSapiAdapter>(
    adapter: A,
    script: impl AsRef<Path>,
) -> Result<ExecutionContext, AdapterError> {
    adapter.build(script)
}

#[test]
fn test_generic_adapter_function() {
    let script_path = php_script_path("hello.php");

    let web_result = execute_adapter(WebRequest::get(), &script_path);
    assert!(web_result.is_ok());

    let cli_result = execute_adapter(CliRequest::new(), &script_path);
    assert!(cli_result.is_ok());

    let test_result = execute_adapter(
        TestAdapter::new().with_required_field("test"),
        &script_path,
    );
    assert!(test_result.is_ok());
}

#[test]
fn test_validation_error_performance() {
    let start = std::time::Instant::now();

    for i in 0..1000 {
        let _ = TestAdapter::validate_field(
            "test_field",
            &i,
            |&val| val < 500,
            "must be less than 500",
        );
    }

    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 100, "Validation should be fast");
}

#[test]
fn test_large_configuration_validation() {
    let script_path = php_script_path("hello.php");

    let long_value = "a".repeat(10000);

    let result = TestAdapter::new()
        .with_required_field(&long_value)
        .build(&script_path);

    assert!(result.is_ok());

    let context = result.unwrap();
    let required_value = context
        .server_vars
        .iter()
        .find(|(k, _)| k == "TEST_REQUIRED")
        .map(|(_, v)| v)
        .expect("TEST_REQUIRED should be set");

    assert_eq!(required_value, &long_value);
}
