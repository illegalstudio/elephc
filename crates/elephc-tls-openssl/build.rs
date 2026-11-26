//! Purpose:
//! Opts the crate's own tests into the managed OpenSSL 3.6.4 libssl/libcrypto
//! archives when `ELEPHC_OPENSSL_LIB_DIR` points at their `lib/` directory.
//!
//! Called from:
//! - Cargo's build-script phase for `elephc-tls-openssl`.
//!
//! Key details:
//! - The production staticlib never links OpenSSL here: the compiler's managed
//!   native-package resolver supplies `libssl.a`/`libcrypto.a` to the final link.
//! - Setting the variable only enables `cargo test -p elephc-tls-openssl`; it
//!   never changes the emitted archive's contents.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(elephc_tls_openssl_native)");
    println!("cargo:rerun-if-env-changed=ELEPHC_OPENSSL_LIB_DIR");
    let Some(dir) = std::env::var_os("ELEPHC_OPENSSL_LIB_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    if !dir.join("libssl.a").is_file() || !dir.join("libcrypto.a").is_file() {
        panic!(
            "ELEPHC_OPENSSL_LIB_DIR must contain libssl.a and libcrypto.a: {}",
            dir.display()
        );
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");
    println!("cargo:rustc-link-lib=z");
    println!("cargo:rustc-cfg=elephc_tls_openssl_native");
}
