use std::path::{Path, PathBuf};

pub const PHP_PREFIX_ENV: &str = "RIPHT_PHP_SAPI_PREFIX";
pub const LINK_MANIFEST: &str = "bia-link-flags.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkDirective {
    SearchPath(String),
    Lib(String),
    Framework(String),
    Arg(String),
}

pub fn docs_rs_enabled(value: Option<&str>) -> bool {
    value.is_some()
}

pub fn fallback_prefixes(home: &str) -> [PathBuf; 3] {
    [
        PathBuf::from(home)
            .join(".ripht")
            .join("php"),
        PathBuf::from(home)
            .join(".local")
            .join("php"),
        PathBuf::from("/usr/local"),
    ]
}

pub fn validate_php_prefix(prefix: &Path) -> bool {
    prefix
        .join("lib")
        .join("libphp.a")
        .exists()
}

pub fn missing_prefix_message() -> String {
    format!(
        "Could not locate a PHP build.\n\
         \n\
         Set {PHP_PREFIX_ENV} to your PHP installation root containing:\n\
         - lib/libphp.a (PHP embed SAPI)\n\
         - include/php/ (PHP headers)\n\
         \n\
         Build PHP with: ./configure --enable-embed=static --disable-zts"
    )
}

pub fn invalid_prefix_message(prefix: &Path) -> String {
    format!(
        "{PHP_PREFIX_ENV} points to an invalid PHP prefix: {}\n\
         ripht-php-sapi requires lib/libphp.a at the selected prefix.",
        prefix.display()
    )
}

pub fn parse_manifest_tokens(manifest: &str) -> Vec<String> {
    let mut tokens = Vec::new();

    for line in manifest.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((_, value)) = line.split_once('=') else {
            continue;
        };

        tokens.extend(split_shell_words(value));
    }

    tokens
}

pub fn link_directives(
    tokens: &[String],
    target_os: &str,
) -> Vec<LinkDirective> {
    if target_os == "linux" {
        return linux_link_directives(tokens);
    }

    cargo_link_directives(tokens, target_os)
}

pub fn split_shell_words(value: &str) -> Vec<String> {
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

pub fn static_dependency_libraries(lib_dir: &Path) -> Vec<&'static str> {
    PHP_DEPENDENCY_LIBS
        .iter()
        .copied()
        .filter(|lib| {
            lib_dir
                .join(format!("lib{lib}.a"))
                .exists()
        })
        .collect()
}

fn linux_link_directives(tokens: &[String]) -> Vec<LinkDirective> {
    let mut directives = Vec::new();

    for token in tokens {
        if token.starts_with("-L") && token.len() > 2 {
            directives.push(LinkDirective::SearchPath(token[2..].to_string()));
        }
    }

    directives.push(LinkDirective::Arg("-Wl,--start-group".to_string()));

    for token in tokens {
        if token.starts_with("-l") && token.len() > 2 {
            directives.push(LinkDirective::Arg(token.clone()));
        }
    }

    directives.push(LinkDirective::Arg("-Wl,--end-group".to_string()));

    for token in tokens {
        match token.as_str() {
            "-pthread" => {
                directives.push(LinkDirective::Arg("-pthread".to_string()));
            }
            token if token.starts_with("-Wl,") => {
                directives.push(LinkDirective::Arg(token.to_string()));
            }
            _ => {}
        }
    }

    directives
}

fn cargo_link_directives(
    tokens: &[String],
    target_os: &str,
) -> Vec<LinkDirective> {
    let mut directives = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i].as_str() {
            "-framework" => {
                if let Some(name) = tokens.get(i + 1) {
                    directives.push(LinkDirective::Framework(name.clone()));
                    i += 2;
                    continue;
                }
            }
            "-pthread" => {
                directives.push(LinkDirective::Arg("-pthread".to_string()));
            }
            token if token.starts_with("-L") && token.len() > 2 => {
                directives
                    .push(LinkDirective::SearchPath(token[2..].to_string()));
            }
            token if token.starts_with("-l") && token.len() > 2 => {
                let lib = &token[2..];

                if target_os == "macos" && lib == "stdc++" {
                    i += 1;
                    continue;
                }

                directives.push(LinkDirective::Lib(lib.to_string()));
            }
            token => {
                directives.push(LinkDirective::Arg(token.to_string()));
            }
        }

        i += 1;
    }

    directives
}

const PHP_DEPENDENCY_LIBS: &[&str] = &[
    "charset",
    "iconv",
    "z",
    "crypto",
    "ssl",
    "curl",
    "xml2",
    "bz2",
    "zip",
    "sqlite3",
    "pgcommon",
    "pgport",
    "pq",
    "png16",
    "png",
    "onig",
    "gmp",
    "ncurses",
    "edit",
    "icudata",
    "icuuc",
    "icuio",
    "icutu",
    "icui18n",
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
