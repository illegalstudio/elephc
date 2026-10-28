//! Purpose:
//! Elephc's own implementation of the PHP extension ABI: what a PECL/PIE
//! extension's C code calls and reads (values, hash tables, argument parsing,
//! classes, exceptions, INI, serialize...), written in Rust, so a compiled
//! program can host an unmodified extension without the PHP engine.
//!
//! Called from:
//! - Extension code, through the C header this crate generates
//!   (`ABI_HEADER` + `MACROS_HEADER`) and the `#[no_mangle]` functions below.
//! - Elephc's generated wrappers, through the `elephc_php_ext_*` contract in
//!   `host`.
//! - Elephc itself (feature `engine` off), which only reads the header text.
//!
//! Key details:
//! - One process, one thread, one long request: the engine starts on first
//!   use and never shuts down, as a php-cli script that runs once.
//! - Memory is the C heap: `emalloc` is `malloc`, so a value either side
//!   allocates, the other may free.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref
)]

pub mod abi;

/// The ABI's types, constants and function prototypes, generated from `abi`
/// and the exported functions by `build.rs`.
pub const ABI_HEADER: &str = include_str!(concat!(env!("OUT_DIR"), "/elephc_zend_abi.h"));

/// The macros extensions expand (`RETURN_STR`, `ZEND_PARSE_PARAMETERS_START`,
/// `ZEND_HASH_FOREACH_*`...): the one part of the ABI that must be C text.
pub const MACROS_HEADER: &str = include_str!("../include/zend_macros.h");

/// File names extensions include (`php.h`, `zend_API.h`...). Each becomes a
/// one-line header including `elephc_zend.h`.
pub const HEADER_NAMES: &str = include_str!("../include/headers.list");

/// Every header an extension build needs, as (relative path, contents).
pub fn include_tree() -> Vec<(String, String)> {
    let mut files = vec![
        ("elephc_zend_abi.h".to_string(), ABI_HEADER.to_string()),
        ("zend_macros.h".to_string(), MACROS_HEADER.to_string()),
        (
            "elephc_zend.h".to_string(),
            "/* elephc_zend.h — the PHP extension ABI as Elephc provides it. */\n\
             #ifndef ELEPHC_ZEND_H\n#define ELEPHC_ZEND_H\n\
             #include \"elephc_zend_abi.h\"\n#include \"zend_macros.h\"\n#endif\n"
                .to_string(),
        ),
    ];
    for name in HEADER_NAMES.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        files.push((name.to_string(), "#include \"elephc_zend.h\"\n".to_string()));
    }
    files
}

#[cfg(feature = "engine")]
mod global;

#[cfg(feature = "engine")]
pub mod alloc;
#[cfg(feature = "engine")]
pub mod string;
#[cfg(feature = "engine")]
pub mod hash;
#[cfg(feature = "engine")]
pub mod zval;
#[cfg(feature = "engine")]
pub mod convert;
#[cfg(feature = "engine")]
pub mod smart_str;
#[cfg(feature = "engine")]
pub mod error;
#[cfg(feature = "engine")]
pub mod zpp;
#[cfg(feature = "engine")]
pub mod module;
#[cfg(feature = "engine")]
pub mod class;
#[cfg(feature = "engine")]
pub mod object;
#[cfg(feature = "engine")]
pub mod property;
#[cfg(feature = "engine")]
pub mod exception;
#[cfg(feature = "engine")]
pub mod api;
#[cfg(feature = "engine")]
pub mod ini;
#[cfg(feature = "engine")]
pub mod constants;
#[cfg(feature = "engine")]
pub mod startup;
#[cfg(feature = "engine")]
pub mod host;
#[cfg(feature = "engine")]
pub mod describe;
#[cfg(feature = "engine")]
pub mod stdlib;
#[cfg(feature = "engine")]
pub mod serialize;
#[cfg(feature = "engine")]
pub mod digest;
#[cfg(feature = "engine")]
pub mod unsupported;

#[cfg(all(test, feature = "engine"))]
mod tests;
