//! Build script for ripht-php-sapi crate.
//!
//! This crate statically links PHP's embed SAPI library (`libphp.a`).
//!
//! # PHP Discovery
//!
//! The build expects a PHP installation prefix (build root) that contains:
//! - `lib/libphp.a` (PHP embed SAPI built as static library)
//! - `include/php/` (PHP headers for FFI validation)
//!
//! If the prefix also contains `lib/bia-link-flags.txt`, that linker manifest is
//! used instead of heuristic dependency scanning. This is the preferred path for
//! Static PHP CLI/Bia-generated prefixes.
//!
//! Configure which PHP build to use via environment variables:
//! - `RIPHT_PHP_SAPI_PREFIX` - Path to PHP build root
//!
//! If not set, the build script checks these fallback locations:
//! - `~/.ripht/php` (project-recommended location)
//! - `~/.local/php` (common user install location)
//! - `/usr/local` (system location)
//!
//! # Building PHP
//! ```sh
//! ./configure --enable-embed=static --disable-zts [other options...]
//! make && make install INSTALL_ROOT=/path/to/prefix
//! ```
//!
//! # Documentation Builds
//!
//! When `DOCS_RS` is set (docs.rs builds), this script skips all PHP discovery/linking.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod build_contract;

use build_contract::{
    docs_rs_enabled, fallback_prefixes, invalid_prefix_message,
    link_directives, missing_prefix_message, parse_manifest_tokens,
    static_dependency_libraries, validate_php_prefix, LinkDirective,
    LINK_MANIFEST, PHP_PREFIX_ENV,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_contract.rs");
    println!("cargo:rerun-if-env-changed={PHP_PREFIX_ENV}");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rustc-check-cfg=cfg(bindgen_available)");

    if docs_rs_enabled(
        env::var("DOCS_RS")
            .ok()
            .as_deref(),
    ) {
        println!("cargo:warning=Building docs - skipping PHP linking");
        return;
    }

    let prefix = find_php_prefix();

    println!("Using PHP prefix: {}", prefix.display());

    let lib_dir = prefix.join("lib");
    if lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    }

    let link_flags = lib_dir.join(LINK_MANIFEST);
    println!("cargo:rerun-if-changed={}", link_flags.display());

    let libphp_path = lib_dir.join("libphp.a");
    if !libphp_path.exists() {
        panic!(
            "ripht-php-sapi requires static linking but libphp.a was not found at: {}\n\
             Set RIPHT_PHP_SAPI_PREFIX to a PHP prefix containing lib/libphp.a (embed SAPI built as static).",
            libphp_path.display()
        );
    }

    println!("Linking against: {}", libphp_path.display());

    if link_flags.is_file() {
        println!("Using PHP linker manifest: {}", link_flags.display());
        emit_link_flags(&link_flags);
        emit_macos_compiler_runtime();
    } else {
        println!("cargo:rustc-link-lib=static=php");
        link_php_dependencies(&lib_dir);
        link_platform_libraries();
    }

    compile_sapi_shim(&prefix);
    generate_bindgen_validation(&prefix);
}

fn find_php_prefix() -> PathBuf {
    if let Ok(prefix) = env::var(PHP_PREFIX_ENV) {
        let path = PathBuf::from(&prefix);
        if validate_php_prefix(&path) {
            return path;
        }

        panic!("{}", invalid_prefix_message(&path));
    }

    let home = env::var("HOME").unwrap_or_else(|_| String::from("/root"));
    for path in fallback_prefixes(&home) {
        if validate_php_prefix(&path) {
            return path;
        }
    }

    panic!("{}", missing_prefix_message());
}

fn emit_link_flags(path: &Path) {
    let manifest =
        fs::read_to_string(path).expect("failed to read PHP linker manifest");
    let tokens = parse_manifest_tokens(&manifest);
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    emit_directives(&link_directives(&tokens, &target_os));
}

fn emit_macos_compiler_runtime() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "macos" {
        return;
    }

    let clang = env::var_os("CC").unwrap_or_else(|| "clang".into());
    let Ok(output) = Command::new(clang)
        .arg("-print-file-name=libclang_rt.osx.a")
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let path = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if path.is_empty()
        || path == "libclang_rt.osx.a"
        || !Path::new(&path).is_file()
    {
        return;
    }

    println!("cargo:rustc-link-arg={path}");
}

fn emit_directives(directives: &[LinkDirective]) {
    for directive in directives {
        match directive {
            LinkDirective::SearchPath(path) => {
                println!("cargo:rustc-link-search=native={path}");
            }
            LinkDirective::Lib(lib) => {
                println!("cargo:rustc-link-lib={lib}");
            }
            LinkDirective::Framework(name) => {
                println!("cargo:rustc-link-lib=framework={name}");
            }
            LinkDirective::Arg(arg) => {
                println!("cargo:rustc-link-arg={arg}");
            }
        }
    }
}

fn link_php_dependencies(lib_dir: &Path) {
    for lib in static_dependency_libraries(lib_dir) {
        println!("cargo:rustc-link-lib=static={lib}");
    }
}

fn link_platform_libraries() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=resolv");
        println!("cargo:rustc-link-lib=iconv");
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=SystemConfiguration");
    }
}

fn compile_sapi_shim(php_prefix: &Path) {
    use std::fs;

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let Some(include_dir) = php_include_dir(php_prefix) else {
        println!("cargo:warning=PHP headers not found, skipping SAPI shim");
        return;
    };

    let shim_path = out_dir.join("ripht_sapi_shim.c");
    fs::write(
        &shim_path,
        r#"#include <stdarg.h>
#include <main/php.h>

int ripht_php_sapi_exit_status(void) {
    return EG(exit_status);
}

void ripht_sapi_error_shim(int type, const char *error_msg, ...) {
    (void) type;
    (void) error_msg;
}
"#,
    )
    .expect("Failed to write SAPI shim");

    cc::Build::new()
        .file(&shim_path)
        .include(&include_dir)
        .include(include_dir.join("main"))
        .include(include_dir.join("Zend"))
        .include(include_dir.join("TSRM"))
        .compile("ripht_sapi_shim");
}

fn generate_bindgen_validation(php_prefix: &Path) {
    use std::fs;

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let output_path = out_dir.join("bindgen_validation.rs");

    let Some(include_dir) = php_include_dir(php_prefix) else {
        println!("cargo:warning=PHP SAPI.h not found, writing stub bindgen_validation.rs");
        fs::write(
            &output_path,
            "// Bindgen validation skipped - PHP headers not available\n",
        )
        .expect("Failed to write stub bindgen file");
        return;
    };

    let sapi_header = include_dir
        .join("main")
        .join("SAPI.h");
    let main_include = include_dir.join("main");
    let zend_include = include_dir.join("Zend");
    let tsrm_include = include_dir.join("TSRM");

    let php_header = include_dir
        .join("main")
        .join("php.h");
    let wrapper_content = format!(
        r#"
#include "{}"
#include "{}"
"#,
        php_header.display(),
        sapi_header.display()
    );

    let wrapper_path = out_dir.join("bindgen_wrapper.h");
    fs::write(&wrapper_path, wrapper_content)
        .expect("Failed to write bindgen wrapper");

    let bindings = bindgen::Builder::default()
        .header(wrapper_path.to_string_lossy())
        .clang_arg(format!("-I{}", main_include.display()))
        .clang_arg(format!("-I{}", zend_include.display()))
        .clang_arg(format!("-I{}", tsrm_include.display()))
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_type("_sapi_globals_struct")
        .allowlist_type("_sapi_module_struct")
        .allowlist_type("sapi_request_info")
        .allowlist_type("_sapi_headers_struct")
        .allowlist_type("sapi_header_struct")
        .allowlist_type("_zend_llist")
        .allowlist_type("_zend_llist_element")
        .allowlist_type("_sapi_request_parse_body_context")
        .opaque_type("_zval_struct")
        .opaque_type("_zend_array")
        .opaque_type("_zend_object")
        .opaque_type("_zend_string")
        .opaque_type("_zend_class_entry")
        .opaque_type("_zend_fcall_info_cache")
        .opaque_type("_zend_function")
        .opaque_type("_zend_function_entry")
        .opaque_type("_zend_module_entry")
        .opaque_type("_php_stream")
        .opaque_type("_sapi_post_entry")
        .derive_debug(true)
        .derive_default(false)
        .layout_tests(false)
        .generate_comments(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate();

    match bindings {
        Ok(b) => {
            b.write_to_file(&output_path)
                .expect("Failed to write bindgen validation output");
            println!(
                "Generated bindgen validation at {}",
                output_path.display()
            );
            println!("cargo:rustc-cfg=bindgen_available");
        }
        Err(e) => {
            println!("cargo:warning=Bindgen generation failed: {}", e);
            fs::write(
                &output_path,
                "// Bindgen generation failed - see build warnings\n",
            )
            .expect("Failed to write error stub");
        }
    }
}

fn php_include_dir(php_prefix: &Path) -> Option<PathBuf> {
    let include_candidates = [
        php_prefix
            .join("include")
            .join("php"),
        php_prefix.join("php"),
    ];

    include_candidates
        .into_iter()
        .find(|p| {
            p.join("main")
                .join("SAPI.h")
                .exists()
        })
}
