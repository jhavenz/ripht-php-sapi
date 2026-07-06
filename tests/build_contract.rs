use std::fs;
use std::path::{Path, PathBuf};

#[path = "../build_contract.rs"]
mod build_contract;

use build_contract::{
    docs_rs_enabled, fallback_prefixes, invalid_prefix_message,
    link_directives, missing_prefix_message, parse_manifest_tokens,
    split_shell_words, static_dependency_libraries, validate_php_prefix,
    LinkDirective, LINK_MANIFEST, PHP_PREFIX_ENV,
};

#[test]
fn shell_words_handle_quotes_and_whitespace() {
    assert_eq!(
        split_shell_words(
            r#"-L"/prefix with spaces/lib" -lphp '-Wl,-rpath,/tmp/x'"#
        ),
        vec!["-L/prefix with spaces/lib", "-lphp", "-Wl,-rpath,/tmp/x"]
    );
}

#[test]
fn manifest_parser_ignores_comments_and_malformed_lines() {
    let tokens = parse_manifest_tokens(
        r#"
# generated
ldflags=-L/prefix/lib
malformed
libs=-lphp -framework CoreFoundation -pthread -Wl,-rpath,/prefix/lib
"#,
    );

    assert_eq!(
        tokens,
        vec![
            "-L/prefix/lib",
            "-lphp",
            "-framework",
            "CoreFoundation",
            "-pthread",
            "-Wl,-rpath,/prefix/lib"
        ]
    );
}

#[test]
fn linux_manifest_tokens_group_static_libraries() {
    let tokens = vec![
        "-L/prefix/lib".to_string(),
        "-lphp".to_string(),
        "-lssl".to_string(),
        "-pthread".to_string(),
        "-Wl,--no-as-needed".to_string(),
    ];

    assert_eq!(
        link_directives(&tokens, "linux"),
        vec![
            LinkDirective::SearchPath("/prefix/lib".to_string()),
            LinkDirective::Arg("-Wl,--start-group".to_string()),
            LinkDirective::Arg("-lphp".to_string()),
            LinkDirective::Arg("-lssl".to_string()),
            LinkDirective::Arg("-Wl,--end-group".to_string()),
            LinkDirective::Arg("-pthread".to_string()),
            LinkDirective::Arg("-Wl,--no-as-needed".to_string()),
        ]
    );
}

#[test]
fn macos_manifest_tokens_skip_stdcxx_and_emit_frameworks() {
    let tokens = vec![
        "-L/prefix/lib".to_string(),
        "-lphp".to_string(),
        "-lstdc++".to_string(),
        "-framework".to_string(),
        "CoreFoundation".to_string(),
        "-pthread".to_string(),
    ];

    assert_eq!(
        link_directives(&tokens, "macos"),
        vec![
            LinkDirective::SearchPath("/prefix/lib".to_string()),
            LinkDirective::Lib("php".to_string()),
            LinkDirective::Framework("CoreFoundation".to_string()),
            LinkDirective::Arg("-pthread".to_string()),
        ]
    );
}

#[test]
fn prefix_validation_requires_static_libphp() {
    let dir = temp_dir("prefix-validation");
    fs::create_dir_all(dir.join("lib")).expect("create lib dir");

    assert!(!validate_php_prefix(&dir));

    fs::write(
        dir.join("lib")
            .join("libphp.a"),
        b"",
    )
    .expect("write libphp placeholder");

    assert!(validate_php_prefix(&dir));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn invalid_prefix_message_names_env_and_libphp_contract() {
    let message = invalid_prefix_message(Path::new("/tmp/ripht-missing-php"));

    assert!(message.contains(PHP_PREFIX_ENV));
    assert!(message.contains("lib/libphp.a"));
}

#[test]
fn docs_rs_env_skips_php_discovery() {
    assert!(docs_rs_enabled(Some("1")));
    assert!(!docs_rs_enabled(None));
}

#[test]
fn fallback_prefixes_match_documented_contract() {
    assert_eq!(LINK_MANIFEST, "bia-link-flags.txt");
    assert_eq!(
        fallback_prefixes("/home/ripht"),
        [
            PathBuf::from("/home/ripht/.ripht/php"),
            PathBuf::from("/home/ripht/.local/php"),
            PathBuf::from("/usr/local"),
        ]
    );

    let message = missing_prefix_message();

    assert!(message.contains(PHP_PREFIX_ENV));
    assert!(message.contains("include/php/"));
}

#[test]
fn fallback_dependency_scan_is_best_effort_by_existing_archives() {
    let dir = temp_dir("fallback-dependency-scan");
    fs::create_dir_all(&dir).expect("create temp lib dir");
    fs::write(dir.join("libz.a"), b"").expect("write libz");
    fs::write(dir.join("libcurl.a"), b"").expect("write libcurl");
    fs::write(dir.join("libssl.dylib"), b"").expect("write shared ssl");

    assert_eq!(static_dependency_libraries(&dir), vec!["z", "curl"]);

    let _ = fs::remove_dir_all(dir);
}

fn temp_dir(name: &str) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "ripht-build-contract-{}-{timestamp}-{name}",
        std::process::id(),
    ))
}
