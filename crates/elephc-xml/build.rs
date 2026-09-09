//! Purpose:
//! Adds native link-search/lib directives for `elephc-xml`'s own test binary, and ONLY
//! for that binary, so `cargo test -p elephc-xml` can link the real libxml2 2.15.3
//! archive plus the Elephc-owned shim archive from an installed `elephc native` package
//! when a developer asks for it.
//!
//! Called from:
//! - Cargo, automatically, before compiling any target of this crate (including
//!   `cargo build -p elephc-xml` and `cargo test -p elephc-xml`).
//!
//! Key details:
//! - `cargo build -p elephc-xml` never needs this: the crate's `staticlib`/`rlib` outputs
//!   never invoke the system linker, so the unresolved `xml*` / `elephc_libxml2_v1_*`
//!   `extern "C"` symbols stay unresolved in the archive regardless of whether these
//!   directives were emitted. The PHP-program linker supplies the managed `libxml2`
//!   package's `lib/` at final-binary link time instead.
//! - `cargo test -p elephc-xml` DOES produce a real executable, so it needs every symbol
//!   resolved. The crate's libxml2-calling unit tests live behind
//!   `#[cfg(elephc_xml_native)]`, which this script only emits when
//!   `ELEPHC_XML_LIBXML2_LIB_DIR` is set. When the cfg is absent nothing in the test
//!   binary references the native symbols (the always-compiled `elephc_xml_*` entry
//!   points are never called by anything in that build, so the linker's dead-stripping
//!   drops them) and the binary links cleanly, running a single test that prints a clear
//!   skip message.
//! - `ELEPHC_XML_LIBXML2_LIB_DIR` must point at the libxml2 artifact's `lib/` directory,
//!   which holds BOTH `libelephc_libxml2_shim.a` and `libxml2.a`; the shim is linked
//!   first because it references libxml2, and libxml2 references nothing but libc (plus
//!   `iconv` on Apple targets, where it is a separate system library).
//! - `ELEPHC_XML_LIB_DIR` is a DIFFERENT variable: `src/linker/bridges.rs` reads it as the
//!   override for a prebuilt `libelephc_xml.a` BRIDGE archive (this crate's own staticlib
//!   output). It is never reused here.

use std::env;
use std::path::Path;

/// Emits the custom `cfg` this build script may set, so rustc's `unexpected_cfgs` lint
/// stays quiet regardless of which branch below runs.
fn declare_check_cfg() {
    println!("cargo:rustc-check-cfg=cfg(elephc_xml_native)");
}

/// Configures the native libxml2 and shim archives when a test artifact is supplied.
fn main() {
    declare_check_cfg();

    println!("cargo:rerun-if-env-changed=ELEPHC_XML_LIBXML2_LIB_DIR");

    let Some(lib_dir) = env::var_os("ELEPHC_XML_LIBXML2_LIB_DIR") else {
        // No native artifact configured. Emit nothing: `cargo build -p elephc-xml` does
        // not need it, and `cargo test -p elephc-xml` links cleanly because the gated
        // real tests (behind `elephc_xml_native`) are simply not compiled in.
        return;
    };

    println!(
        "cargo:rustc-link-search=native={}",
        Path::new(&lib_dir).display()
    );
    // Shim first: it references libxml2's symbols, which the archive after it resolves.
    // `-bundle`: link the test binary against the archives WITHOUT copying their members
    // into `libelephc_xml.a` — the staticlib the codegen harness and shipped programs link
    // must keep reaching libxml2 through the managed catalog artifact, never through a
    // second copy bundled at `cargo test` time.
    println!("cargo:rustc-link-lib=static:-bundle=elephc_libxml2_shim");
    println!("cargo:rustc-link-lib=static:-bundle=xml2");

    let target_vendor = env::var_os("CARGO_CFG_TARGET_VENDOR");
    if target_vendor.as_deref() == Some(std::ffi::OsStr::new("apple")) {
        // libxml2 is built `--with-iconv`; glibc ships iconv inside libc, Apple as a
        // separate system library.
        println!("cargo:rustc-link-lib=dylib=iconv");
    }

    // Compile the crate's real, libxml2-calling unit tests in.
    println!("cargo:rustc-cfg=elephc_xml_native");
}
