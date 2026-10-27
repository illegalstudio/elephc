//! Purpose:
//! Links optional bridge unit tests to the managed libmagic archives.
//!
//! Called from:
//! - Cargo before building elephc-fileinfo.
//!
//! Key details:
//! - Ordinary staticlibs keep unresolved native symbols for final PHP-program linking.

/// Enables native tests only when their exact managed archives are supplied.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(elephc_fileinfo_native)");
    println!("cargo:rerun-if-env-changed=ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR");
    if let Some(dir) = std::env::var_os("ELEPHC_FILEINFO_LIBMAGIC_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", std::path::Path::new(&dir).display());
        println!("cargo:rustc-link-lib=static:-bundle=elephc_magic_database");
        println!("cargo:rustc-link-lib=static:-bundle=magic");
        println!("cargo:rustc-cfg=elephc_fileinfo_native");
    }
}
