//! Purpose:
//! Compiles the exact timelib sources vendored from the audited php-src PHP-8.5 commit.
//!
//! Called from:
//! - Cargo when building the `elephc-tz` bridge.
//!
//! Key details:
//! - The timelib source list mirrors `ext/date/config0.m4`; the final local C
//!   translation unit asserts the shared ABI layout and emits no runtime code.
//! - Using php-src's generated parsers and timezone database keeps grammar and
//!   tzdb provenance identical on every supported target.

/// Compiles php-src's standalone timelib C sources into the bridge.
fn main() {
    let sources = [
        "vendor/timelib/astro.c",
        "vendor/timelib/dow.c",
        "vendor/timelib/parse_date.c",
        "vendor/timelib/parse_tz.c",
        "vendor/timelib/parse_posix.c",
        "vendor/timelib/timelib.c",
        "vendor/timelib/tm2unixtime.c",
        "vendor/timelib/unixtime2tm.c",
        "vendor/timelib/parse_iso_intervals.c",
        "vendor/timelib/interval.c",
        "src/timelib_layout_asserts.c",
    ];
    let mut build = cc::Build::new();
    build
        .files(sources)
        .include("vendor/timelib")
        .define("HAVE_STDINT_H", None)
        .define("HAVE_GETTIMEOFDAY", None)
        .flag_if_supported("-fwrapv")
        .flag_if_supported("-Wno-implicit-fallthrough")
        .flag_if_supported("-Wno-unused-parameter");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        build.define("HAVE_IO_H", None);
    } else {
        println!("cargo:rustc-link-lib=m");
        build.define("HAVE_UNISTD_H", None);
    }
    build.compile("elephc_php_src_timelib");
}
