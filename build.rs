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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RIPHT_PHP_SAPI_PREFIX");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rustc-check-cfg=cfg(bindgen_available)");

    if env::var("DOCS_RS").is_ok() {
        println!("cargo:warning=Building docs - skipping PHP linking");
        return;
    }

    let prefix = find_php_prefix().unwrap_or_else(|| {
        panic!(
            "Could not locate a PHP build.\n\
             \n\
             Set RIPHT_PHP_SAPI_PREFIX to your PHP installation root containing:\n\
             - lib/libphp.a (PHP embed SAPI)\n\
             - include/php/ (PHP headers)\n\
             \n\
             Build PHP with: ./configure --enable-embed=static --disable-zts"
        )
    });

    println!("Using PHP prefix: {}", prefix.display());

    let lib_dir = prefix.join("lib");
    if lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    }

    let link_flags = lib_dir.join("bia-link-flags.txt");
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

    compile_exit_status_shim(&prefix);
    generate_bindgen_validation(&prefix);
}

fn find_php_prefix() -> Option<PathBuf> {
    if let Ok(prefix) = env::var("RIPHT_PHP_SAPI_PREFIX") {
        let path = PathBuf::from(&prefix);
        if validate_php_prefix(&path) {
            return Some(path);
        }
        println!("RIPHT_PHP_SAPI_PREFIX set but invalid: {}", prefix);
    }

    let home = env::var("HOME").unwrap_or_else(|_| String::from("/root"));
    let candidates = [
        format!("{}/.ripht/php", home),
        format!("{}/.local/php", home),
        "/usr/local".to_string(),
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if validate_php_prefix(&path) {
            return Some(path);
        }
    }

    None
}

fn validate_php_prefix(prefix: &Path) -> bool {
    prefix.exists()
        && prefix
            .join("lib")
            .join("libphp.a")
            .exists()
}

fn emit_link_flags(path: &Path) {
    let manifest =
        fs::read_to_string(path).expect("failed to read PHP linker manifest");

    for line in manifest.lines() {
        let Some((_, value)) = line.split_once('=') else {
            continue;
        };

        emit_tokens(&split_shell_words(value));
    }
}

fn emit_tokens(tokens: &[String]) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "linux" {
        emit_linux_tokens(tokens);

        return;
    }

    emit_cargo_tokens(tokens, &target_os);
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

fn emit_linux_tokens(tokens: &[String]) {
    for token in tokens {
        if token.starts_with("-L") && token.len() > 2 {
            println!("cargo:rustc-link-search=native={}", &token[2..]);
        }
    }

    println!("cargo:rustc-link-arg=-Wl,--start-group");

    for token in tokens {
        if token.starts_with("-l") && token.len() > 2 {
            println!("cargo:rustc-link-arg={token}");
        }
    }

    println!("cargo:rustc-link-arg=-Wl,--end-group");

    for token in tokens {
        match token.as_str() {
            "-pthread" => println!("cargo:rustc-link-arg=-pthread"),
            token if token.starts_with("-Wl,") => {
                println!("cargo:rustc-link-arg={token}")
            }
            _ => {}
        }
    }
}

fn emit_cargo_tokens(tokens: &[String], target_os: &str) {
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i].as_str() {
            "-framework" => {
                if let Some(name) = tokens.get(i + 1) {
                    println!("cargo:rustc-link-lib=framework={name}");
                    i += 2;
                    continue;
                }
            }
            "-pthread" => {
                println!("cargo:rustc-link-arg=-pthread");
            }
            token if token.starts_with("-L") && token.len() > 2 => {
                println!("cargo:rustc-link-search=native={}", &token[2..]);
            }
            token if token.starts_with("-l") && token.len() > 2 => {
                let lib = &token[2..];

                if target_os == "macos" && lib == "stdc++" {
                    i += 1;
                    continue;
                }

                println!("cargo:rustc-link-lib={lib}");
            }
            token => {
                println!("cargo:rustc-link-arg={token}");
            }
        }

        i += 1;
    }
}

fn split_shell_words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in value.chars() {
        match (quote, ch) {
            (Some(active), ch) if ch == active => {
                quote = None;
            }
            (Some(_), ch) => current.push(ch),
            (None, '\'' | '"') => {
                quote = Some(ch);
            }
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn link_php_dependencies(lib_dir: &Path) {
    let xml_libs = ["xml2"];
    let network_libs = ["curl"];
    let text_libs = ["onig", "gmp"];
    let ssl_libs = ["crypto", "ssl"];
    let archive_libs = ["bz2", "zip"];
    let image_libs = ["png16", "png"];
    let terminal_libs = ["ncurses", "edit"];
    let core_libs = ["charset", "iconv", "z"];
    let db_libs = ["sqlite3", "pgcommon", "pgport", "pq"];
    let icu_libs = ["icudata", "icuuc", "icuio", "icutu", "icui18n"];
    let extension_libs = [
        "brotli",
        "brotlicommon",
        "brotlidec",
        "brotlienc",
        "cares",
        "ffi",
        "lzma",
        "nghttp2",
        "sodium",
        "yaml",
        "zstd",
    ];

    for lib in core_libs
        .iter()
        .chain(ssl_libs.iter())
        .chain(network_libs.iter())
        .chain(xml_libs.iter())
        .chain(archive_libs.iter())
        .chain(db_libs.iter())
        .chain(image_libs.iter())
        .chain(text_libs.iter())
        .chain(terminal_libs.iter())
        .chain(icu_libs.iter())
        .chain(extension_libs.iter())
    {
        let lib_file = format!("lib{}.a", lib);
        if lib_dir
            .join(&lib_file)
            .exists()
        {
            println!("cargo:rustc-link-lib=static={}", lib);
        }
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

fn compile_exit_status_shim(php_prefix: &Path) {
    use std::fs;

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let Some(include_dir) = php_include_dir(php_prefix) else {
        println!(
            "cargo:warning=PHP headers not found, skipping exit-status shim"
        );
        return;
    };

    let shim_path = out_dir.join("exit_status_shim.c");
    fs::write(
        &shim_path,
        r#"#include <main/php.h>

int ripht_php_sapi_exit_status(void) {
    return EG(exit_status);
}
"#,
    )
    .expect("Failed to write exit-status shim");

    cc::Build::new()
        .file(&shim_path)
        .include(&include_dir)
        .include(include_dir.join("main"))
        .include(include_dir.join("Zend"))
        .include(include_dir.join("TSRM"))
        .compile("ripht_exit_status_shim");
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
