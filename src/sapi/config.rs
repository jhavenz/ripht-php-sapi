//! SAPI identity and PHP startup configuration.
//!
//! [`SapiConfig`] is a builder for the values PHP reads once at module
//! startup: the SAPI name (`php_sapi_name()`), pretty name, `SERVER_SOFTWARE`,
//! module-level INI entries, and `php.ini` discovery behavior. Apply it via
//! [`super::RiphtSapi::configure`] before the first
//! [`super::RiphtSapi::instance`] call. Defaults reproduce the values that
//! were hardcoded in prior releases, so omitting configuration is a no-op.

use std::ffi::{CStr, CString};

use super::ffi::zend_function_entry;
use super::native::Function as NativeFunction;
use super::native_functions;
use super::SapiError;

const DEFAULT_SAPI_NAME: &str = "ripht";
const DEFAULT_PRETTY_NAME: &str = "Ripht PHP SAPI";
const DEFAULT_SERVER_SOFTWARE: &str =
    concat!("Ripht/", env!("CARGO_PKG_VERSION"));

const DEFAULT_INI_ENTRIES: &[(&str, &str)] = &[
    ("variables_order", "EGPCS"),
    ("request_order", "GP"),
    ("output_buffering", "4096"),
    ("implicit_flush", "0"),
    ("html_errors", "0"),
    ("display_errors", "1"),
    ("log_errors", "1"),
];

/// SAPI identity and PHP startup configuration.
///
/// Apply before first [`super::RiphtSapi::instance()`] call via
/// [`super::RiphtSapi::configure()`]. Defaults match the hardcoded
/// values from prior releases.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SapiConfig {
    pub sapi_name: String,
    pub pretty_name: String,
    pub server_software: String,
    pub ini_entries: Vec<(String, String)>,
    pub ignore_php_ini: bool,
    pub ignore_cwd_ini: bool,
    pub ini_path: Option<String>,
    native_functions: Vec<zend_function_entry>,
}

impl Default for SapiConfig {
    fn default() -> Self {
        Self {
            sapi_name: DEFAULT_SAPI_NAME.to_owned(),
            pretty_name: DEFAULT_PRETTY_NAME.to_owned(),
            server_software: DEFAULT_SERVER_SOFTWARE.to_owned(),
            ini_entries: DEFAULT_INI_ENTRIES
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            ignore_php_ini: false,
            ignore_cwd_ini: true,
            ini_path: None,
            native_functions: Vec::new(),
        }
    }
}

impl SapiConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn sapi_name(mut self, name: impl Into<String>) -> Self {
        self.sapi_name = name.into();
        self
    }

    #[must_use]
    pub fn pretty_name(mut self, name: impl Into<String>) -> Self {
        self.pretty_name = name.into();
        self
    }

    #[must_use]
    pub fn server_software(mut self, software: impl Into<String>) -> Self {
        self.server_software = software.into();
        self
    }

    #[must_use]
    pub fn ini_entries(mut self, entries: Vec<(String, String)>) -> Self {
        self.ini_entries = entries;
        self
    }

    #[must_use]
    pub fn ini_entry(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.ini_entries
            .push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn ignore_php_ini(mut self, ignore: bool) -> Self {
        self.ignore_php_ini = ignore;
        self
    }

    #[must_use]
    pub fn ignore_cwd_ini(mut self, ignore: bool) -> Self {
        self.ignore_cwd_ini = ignore;
        self
    }

    #[must_use]
    pub fn ini_path(mut self, path: impl Into<String>) -> Self {
        self.ini_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn native_functions(
        mut self,
        entries: &'static [NativeFunction],
    ) -> Self {
        self.native_functions = entries
            .iter()
            .copied()
            .map(NativeFunction::entry)
            .take_while(|entry| !entry.fname.is_null())
            .collect();
        self
    }

    /// Validate the configuration and leak its strings to `'static`.
    ///
    /// PHP holds the SAPI module's `name`, `pretty_name`, `ini_entries`, and
    /// `php_ini_path_override` pointers for the lifetime of the process, so the
    /// resolved buffers are intentionally leaked (`Box::leak`) rather than
    /// owned. Call this exactly once, from the one-time engine init in
    /// [`super::RiphtSapi`]; repeated calls leak additional memory.
    pub(crate) fn resolve(self) -> Result<ResolvedConfig, SapiError> {
        fn leak_null_terminated(s: String) -> &'static [u8] {
            let mut bytes = s.into_bytes();
            bytes.push(0);
            Box::leak(bytes.into_boxed_slice())
        }

        if self.sapi_name.contains('\0') {
            return Err(SapiError::InvalidSapiName(self.sapi_name));
        }
        let sapi_name = leak_null_terminated(self.sapi_name);

        if self
            .pretty_name
            .contains('\0')
        {
            return Err(SapiError::InvalidPrettyName(self.pretty_name));
        }
        let pretty_name = leak_null_terminated(self.pretty_name);

        if self
            .server_software
            .contains('\0')
        {
            return Err(SapiError::InvalidServerSoftware(self.server_software));
        }
        let server_software: &'static str = Box::leak(
            self.server_software
                .into_boxed_str(),
        );

        let mut ini_blob = String::new();
        for (key, value) in &self.ini_entries {
            if key.contains('\0') {
                return Err(SapiError::InvalidIniKey);
            }
            if value.contains('\0') {
                return Err(SapiError::InvalidIniValue);
            }
            ini_blob.push_str(key);
            ini_blob.push('=');
            ini_blob.push_str(value);
            ini_blob.push('\n');
        }
        let ini_entries = leak_null_terminated(ini_blob);

        let ini_path = match self.ini_path {
            Some(path) => {
                let cstring = CString::new(path)
                    .map_err(|_| SapiError::InvalidIniPath)?;
                Some(Box::leak(cstring.into_boxed_c_str()) as &'static CStr)
            }
            None => None,
        };

        let mut native_entries = if self
            .native_functions
            .is_empty()
        {
            native_functions::entries()
                .iter()
                .copied()
                .take_while(|entry| !entry.fname.is_null())
                .collect()
        } else {
            self.native_functions
        };
        native_entries.push(zend_function_entry::end());
        let native_functions = Box::leak(native_entries.into_boxed_slice());

        Ok(ResolvedConfig {
            sapi_name,
            pretty_name,
            server_software,
            ini_entries,
            ignore_php_ini: self.ignore_php_ini,
            ignore_cwd_ini: self.ignore_cwd_ini,
            ini_path,
            native_functions,
        })
    }
}

/// Process-lifetime C-compatible pointers for the SAPI module struct.
/// Created via [`SapiConfig::resolve`] and stored in a `OnceLock`.
pub(crate) struct ResolvedConfig {
    pub sapi_name: &'static [u8],
    pub pretty_name: &'static [u8],
    pub server_software: &'static str,
    pub ini_entries: &'static [u8],
    pub ignore_php_ini: bool,
    pub ignore_cwd_ini: bool,
    pub ini_path: Option<&'static CStr>,
    pub native_functions: &'static [zend_function_entry],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};

    use super::super::native::ReturnValue;

    unsafe extern "C" fn fake_handler(
        _execute_data: *mut c_void,
        _return_value: *mut ReturnValue,
    ) {
    }

    #[test]
    fn default_sapi_name() {
        assert_eq!(SapiConfig::default().sapi_name, "ripht");
    }

    #[test]
    fn default_pretty_name() {
        assert_eq!(SapiConfig::default().pretty_name, "Ripht PHP SAPI");
    }

    #[test]
    fn default_server_software_prefix() {
        assert!(SapiConfig::default()
            .server_software
            .starts_with("Ripht/"));
    }

    #[test]
    fn default_ini_entries_count() {
        assert_eq!(
            SapiConfig::default()
                .ini_entries
                .len(),
            7
        );
    }

    #[test]
    fn default_ini_contains_variables_order() {
        let config = SapiConfig::default();
        assert!(config
            .ini_entries
            .contains(&("variables_order".to_owned(), "EGPCS".to_owned())));
    }

    #[test]
    fn default_ignore_flags() {
        let config = SapiConfig::default();
        assert!(!config.ignore_php_ini);
        assert!(config.ignore_cwd_ini);
    }

    #[test]
    fn builder_chaining() {
        let config = SapiConfig::new()
            .sapi_name("cli")
            .pretty_name("Test SAPI")
            .server_software("Test/1.0")
            .ignore_php_ini(true)
            .ignore_cwd_ini(false)
            .ini_path("/etc/php.ini");

        assert_eq!(config.sapi_name, "cli");
        assert_eq!(config.pretty_name, "Test SAPI");
        assert_eq!(config.server_software, "Test/1.0");
        assert!(config.ignore_php_ini);
        assert!(!config.ignore_cwd_ini);
        assert_eq!(config.ini_path.as_deref(), Some("/etc/php.ini"));
    }

    #[test]
    fn ini_entry_appends() {
        let config = SapiConfig::new().ini_entry("custom_key", "custom_value");
        assert_eq!(config.ini_entries.len(), 8);
        assert_eq!(
            config
                .ini_entries
                .last()
                .unwrap(),
            &("custom_key".to_owned(), "custom_value".to_owned())
        );
    }

    #[test]
    fn ini_entries_replaces_all() {
        let config = SapiConfig::new()
            .ini_entries(vec![("only".to_owned(), "one".to_owned())]);
        assert_eq!(config.ini_entries.len(), 1);
    }

    #[test]
    fn resolve_null_terminates_sapi_name() {
        let resolved = SapiConfig::new()
            .sapi_name("test")
            .resolve()
            .unwrap();
        assert_eq!(resolved.sapi_name, b"test\0");
    }

    #[test]
    fn resolve_null_terminates_pretty_name() {
        let resolved = SapiConfig::new()
            .pretty_name("My SAPI")
            .resolve()
            .unwrap();
        assert_eq!(resolved.pretty_name, b"My SAPI\0");
    }

    #[test]
    fn resolve_ini_blob_format() {
        let resolved = SapiConfig::new()
            .ini_entries(vec![
                ("key1".to_owned(), "val1".to_owned()),
                ("key2".to_owned(), "val2".to_owned()),
            ])
            .resolve()
            .unwrap();
        assert_eq!(resolved.ini_entries, b"key1=val1\nkey2=val2\n\0");
    }

    #[test]
    fn resolve_empty_ini_blob() {
        let resolved = SapiConfig::new()
            .ini_entries(vec![])
            .resolve()
            .unwrap();
        assert_eq!(resolved.ini_entries, b"\0");
    }

    #[test]
    fn resolve_rejects_null_in_sapi_name() {
        let result = SapiConfig::new()
            .sapi_name("te\0st")
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidSapiName(_))));
    }

    #[test]
    fn resolve_rejects_null_in_pretty_name() {
        let result = SapiConfig::new()
            .pretty_name("My\0SAPI")
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidPrettyName(_))));
    }

    #[test]
    fn resolve_rejects_null_in_server_software() {
        let result = SapiConfig::new()
            .server_software("Bad\0/1.0")
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidServerSoftware(_))));
    }

    #[test]
    fn resolve_rejects_null_in_ini_key() {
        let result = SapiConfig::new()
            .ini_entries(vec![("ke\0y".to_owned(), "val".to_owned())])
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidIniKey)));
    }

    #[test]
    fn resolve_rejects_null_in_ini_value() {
        let result = SapiConfig::new()
            .ini_entries(vec![("key".to_owned(), "va\0l".to_owned())])
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidIniValue)));
    }

    #[test]
    fn resolve_rejects_null_in_ini_path() {
        let result = SapiConfig::new()
            .ini_path("/etc/ph\0p.ini")
            .resolve();
        assert!(matches!(result, Err(SapiError::InvalidIniPath)));
    }

    #[test]
    fn resolve_ini_path_produces_valid_cstr() {
        let resolved = SapiConfig::new()
            .ini_path("/etc/php.ini")
            .resolve()
            .unwrap();
        let cstr = resolved.ini_path.unwrap();
        assert_eq!(cstr.to_str().unwrap(), "/etc/php.ini");
    }

    #[test]
    fn resolve_defaults_match_prior_constants() {
        let resolved = SapiConfig::default()
            .resolve()
            .unwrap();
        assert_eq!(resolved.sapi_name, b"ripht\0");
        assert_eq!(resolved.pretty_name, b"Ripht PHP SAPI\0");
        assert!(resolved
            .server_software
            .starts_with("Ripht/"));
        assert!(!resolved.ignore_php_ini);
        assert!(resolved.ignore_cwd_ini);
        assert!(resolved.ini_path.is_none());

        let ini = std::str::from_utf8(
            &resolved.ini_entries[..resolved.ini_entries.len() - 1],
        )
        .unwrap();
        assert!(ini.contains("variables_order=EGPCS"));
        assert!(ini.contains("log_errors=1"));
    }

    #[test]
    fn resolve_uses_custom_native_functions_before_terminator() {
        static CUSTOM: [NativeFunction; 1] = [
            // SAFETY: the function name is static, this test does not execute the
            // handler, and the null arginfo pointer is only used to verify table wiring.
            unsafe {
                NativeFunction::new_unchecked(
                    b"ripht_test_native\0".as_ptr() as *const c_char,
                    fake_handler,
                    std::ptr::null(),
                    0,
                )
            },
        ];

        let resolved = SapiConfig::new()
            .native_functions(&CUSTOM)
            .resolve()
            .unwrap();

        let names = resolved
            .native_functions
            .iter()
            .take_while(|entry| !entry.fname.is_null())
            .map(|entry| {
                unsafe { CStr::from_ptr(entry.fname) }
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["ripht_test_native"]);
        assert!(resolved
            .native_functions
            .last()
            .unwrap()
            .fname
            .is_null());
    }
}
