//! Purpose:
//! Opts the crate's own tests into a real libcrypto when `ELEPHC_OPENSSL_LIB_DIR`
//! points at a pinned OpenSSL 3.6.4 `lib/` directory.
//!
//! Called from:
//! - Cargo's build-script phase for `elephc-openssl`.
//!
//! Key details:
//! - The production staticlib never links libcrypto here: the compiler's managed
//!   native-package resolver supplies `libcrypto.a` to the final program link.
//! - Setting the variable only enables `cargo test -p elephc-openssl`; it never
//!   changes the emitted archive's contents.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(elephc_openssl_native)");
    println!("cargo:rerun-if-env-changed=ELEPHC_OPENSSL_LIB_DIR");
    let Some(dir) = std::env::var_os("ELEPHC_OPENSSL_LIB_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    if !dir.join("libcrypto.a").is_file() {
        panic!("ELEPHC_OPENSSL_LIB_DIR must contain libcrypto.a: {}", dir.display());
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=crypto");
    println!("cargo:rustc-link-lib=z");
    println!("cargo:rustc-cfg=elephc_openssl_native");
}
