//! Purpose:
//! Generates the crate's C header (see `abi_header.rs`) and compiles its one C
//! unit (the variadic entry points and the bailout frame).
//!
//! Called from:
//! - Cargo, before compiling the crate.
//!
//! Key details:
//! - The header lands in `$OUT_DIR/elephc_zend_abi.h`, which `lib.rs` embeds
//!   for the crate's own tests; Elephc generates the same text itself.

use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "abi_header.rs"]
mod abi_header;

/// Build script entry point: regenerates the C ABI header from the Rust sources into OUT_DIR and
/// compiles the C side of the crate.
fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("src");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=include");
    println!("cargo:rerun-if-changed=abi_header.rs");

    let header = abi_header::generate(&src).unwrap_or_else(|message| panic!("elephc-zend ABI header: {message}"));
    fs::write(out.join("elephc_zend_abi.h"), header).expect("write generated header");

    if env::var_os("CARGO_FEATURE_ENGINE").is_some() {
        cc::Build::new()
            .file(src.join("c/variadic.c"))
            .flag_if_supported("-std=gnu17")
            .warnings(true)
            .compile("elephc_zend_variadic");
        // `php_iconv_string` uses the system iconv, its own library on Apple
        // targets. Elephc passes the same flag when it links a program.
        if env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
            println!("cargo:rustc-link-lib=iconv");
        }
    }
}
