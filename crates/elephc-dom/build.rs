//! Purpose:
//! Builds the pinned libxml2 2.15.3 and PHP-bundled Lexbor 2.7.0 sources offline.
//! Verifies vendored archive identities before compiling the native engine adapter.
//!
//! Called from:
//! - Cargo while building `elephc-dom`.
//!
//! Key details:
//! - libxml2 feature flags mirror the frozen PHP 8.5.8 DOM compliance specification.
//! - Lexbor source membership is read from PHP's own `ext/lexbor/config.m4`.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};

const PHP_NATIVE_SHA256: &str =
    "da88520251cc5833281fe6b484598b661aff5051aae7f732502c151cbe157008";
const LIBXML_SHA256: &str =
    "78262a6e7ac170d6528ebfe2efccdf220191a5af6a6cd61ea4a9a9a5042c7a07";

/// Returns the lowercase SHA-256 digest for one vendored archive.
fn sha256(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read vendored archive {}: {error}", path.display())
    });
    format!("{:x}", Sha256::digest(bytes))
}

/// Rejects a vendored archive whose bytes differ from the frozen source lock.
fn verify_archive(path: &Path, expected: &str) {
    let actual = sha256(path);
    assert_eq!(
        actual,
        expected,
        "vendored native archive checksum mismatch for {}",
        path.display()
    );
}

/// Runs one native build command and reports its complete output on failure.
fn run(command: &mut Command) {
    let description = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to start {description}: {error}"));
    if !output.status.success() {
        panic_command(&description, &output);
    }
}

/// Panics with stdout and stderr from one failed native build command.
fn panic_command(description: &str, output: &Output) -> ! {
    panic!(
        "native build command failed: {description}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Extracts a trusted, checksum-verified XZ tar archive into Cargo's output tree.
fn extract_archive(archive: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap_or_else(|error| {
        panic!(
            "failed to create native source directory {}: {error}",
            destination.display()
        )
    });
    run(Command::new("tar")
        .arg("-xJf")
        .arg(archive)
        .arg("-C")
        .arg(destination));
}

/// Configures, compiles, and installs the pinned static libxml2 archive.
fn build_libxml(
    source: &Path,
    output: &Path,
    jobs: &OsStr,
    target: &str,
) -> PathBuf {
    let build = output.join("libxml-build");
    let install = output.join("libxml-install");
    let compiler = cc::Build::new().get_compiler();
    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(source)
        .arg("-B")
        .arg(&build)
        .arg(format!(
            "-DCMAKE_INSTALL_PREFIX={}",
            install.to_string_lossy()
        ))
        .arg(format!(
            "-DCMAKE_C_COMPILER={}",
            compiler.path().to_string_lossy()
        ))
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg("-DBUILD_SHARED_LIBS=OFF")
        .arg("-DLIBXML2_WITH_C14N=ON")
        .arg("-DLIBXML2_WITH_CATALOG=ON")
        .arg("-DLIBXML2_WITH_DEBUG=OFF")
        .arg("-DLIBXML2_WITH_DOCS=OFF")
        .arg("-DLIBXML2_WITH_HTML=ON")
        .arg("-DLIBXML2_WITH_HTTP=OFF")
        .arg("-DLIBXML2_WITH_ICONV=ON")
        .arg("-DLIBXML2_WITH_ICU=OFF")
        .arg("-DLIBXML2_WITH_ISO8859X=ON")
        .arg("-DLIBXML2_WITH_LEGACY=OFF")
        .arg("-DLIBXML2_WITH_LZMA=OFF")
        .arg("-DLIBXML2_WITH_MODULES=OFF")
        .arg("-DLIBXML2_WITH_OUTPUT=ON")
        .arg("-DLIBXML2_WITH_PATTERN=ON")
        .arg("-DLIBXML2_WITH_PROGRAMS=OFF")
        .arg("-DLIBXML2_WITH_PUSH=ON")
        .arg("-DLIBXML2_WITH_PYTHON=OFF")
        .arg("-DLIBXML2_WITH_READER=ON")
        .arg("-DLIBXML2_WITH_REGEXPS=ON")
        .arg("-DLIBXML2_WITH_RELAXNG=ON")
        .arg("-DLIBXML2_WITH_SAX1=ON")
        .arg("-DLIBXML2_WITH_SCHEMAS=ON")
        .arg("-DLIBXML2_WITH_SCHEMATRON=ON")
        .arg("-DLIBXML2_WITH_TESTS=OFF")
        .arg("-DLIBXML2_WITH_THREADS=ON")
        .arg("-DLIBXML2_WITH_TLS=ON")
        .arg("-DLIBXML2_WITH_VALID=ON")
        .arg("-DLIBXML2_WITH_WRITER=ON")
        .arg("-DLIBXML2_WITH_XINCLUDE=ON")
        .arg("-DLIBXML2_WITH_XPATH=ON")
        .arg("-DLIBXML2_WITH_XPTR=ON")
        .arg("-DLIBXML2_WITH_ZLIB=OFF");
    if target.contains("apple") {
        let deployment = env::var("MACOSX_DEPLOYMENT_TARGET")
            .unwrap_or_else(|_| "11.0".to_owned());
        configure.arg(format!("-DCMAKE_OSX_DEPLOYMENT_TARGET={deployment}"));
    }
    run(&mut configure);

    run(Command::new("cmake")
        .arg("--build")
        .arg(&build)
        .arg("--config")
        .arg("Release")
        .arg("--target")
        .arg("install")
        .arg("--parallel")
        .arg(jobs));
    install
}

/// Reads PHP's authoritative Lexbor compilation unit list from `config.m4`.
fn lexbor_sources(extension_root: &Path) -> Vec<PathBuf> {
    let config_path = extension_root.join("config.m4");
    let config = fs::read_to_string(&config_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", config_path.display()));
    let start_marker = "PHP_NEW_EXTENSION([lexbor], m4_normalize([";
    let start = config
        .find(start_marker)
        .unwrap_or_else(|| panic!("Lexbor source-list start marker is absent"))
        + start_marker.len();
    let remainder = &config[start..];
    let end = remainder
        .find("]),")
        .unwrap_or_else(|| panic!("Lexbor source-list end marker is absent"));
    let mut sources = Vec::new();
    for line in remainder[..end].lines() {
        let relative = line.trim();
        if relative.is_empty() || relative == "php_lexbor.c" {
            continue;
        }
        let relative = relative
            .strip_prefix("$LEXBOR_DIR/")
            .unwrap_or_else(|| panic!("unexpected Lexbor source entry: {relative}"));
        let path = extension_root.join("lexbor").join(relative);
        assert!(path.is_file(), "Lexbor source is absent: {}", path.display());
        sources.push(path);
    }
    assert_eq!(
        sources.len(),
        185,
        "PHP 8.5.8 Lexbor source inventory changed unexpectedly"
    );
    sources
}

/// Compiles PHP's pinned Lexbor inventory together with the small C engine adapter.
fn build_lexbor(
    extension_root: &Path,
    libxml_install: &Path,
    manifest: &Path,
    target: &str,
) {
    let mut build = cc::Build::new();
    build
        .include(extension_root)
        .include(extension_root.parent().expect("PHP ext root is required"))
        .include(manifest.join("native/php_compat"))
        .include(libxml_install.join("include/libxml2"))
        .define("LEXBOR_STATIC", None)
        .warnings(false)
        .flag_if_supported("-std=c11")
        .file(manifest.join("native/engine.c"))
        .file(manifest.join("native/document_metadata.c"))
        .file(manifest.join("native/html_parser.c"))
        .file(manifest.join("native/html_serializer.c"))
        .file(manifest.join("native/selectors_adapter.c"))
        .file(manifest.join("native/selector_engine.c"))
        .file(manifest.join("native/simplexml.c"))
        .file(manifest.join("native/simplexml_handlers.c"));
    if target.contains("apple") {
        let deployment = env::var("MACOSX_DEPLOYMENT_TARGET")
            .unwrap_or_else(|_| "11.0".to_owned());
        build.flag(&format!("-mmacosx-version-min={deployment}"));
    }
    for source in lexbor_sources(extension_root) {
        build.file(source);
    }
    build.compile("elephc_dom_native");
}

/// Emits platform system-library requirements used by the pinned static libxml2 build.
fn emit_platform_links(target: &str) {
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=iconv");
    }
    if target.contains("linux") {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
}

/// Verifies inputs, prepares native sources, and builds the two pinned engines.
fn main() {
    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is required"),
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is required"));
    let target = env::var("TARGET").expect("TARGET is required");
    let jobs = env::var_os("NUM_JOBS").unwrap_or_else(|| OsStr::new("1").to_owned());
    let vendor = manifest.join("vendor");
    let php_archive = vendor.join("php-8.5.8-dom-native.tar.xz");
    let libxml_archive = vendor.join("libxml2-2.15.3.tar.xz");

    println!("cargo:rerun-if-changed={}", php_archive.display());
    println!("cargo:rerun-if-changed={}", libxml_archive.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/engine.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/document_metadata.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/html_parser.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/html_serializer.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/selectors_adapter.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/selector_engine.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/simplexml.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/simplexml_handlers.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        manifest.join("native/php_compat").display()
    );
    verify_archive(&php_archive, PHP_NATIVE_SHA256);
    verify_archive(&libxml_archive, LIBXML_SHA256);

    let sources = output.join("native-sources");
    extract_archive(&php_archive, &sources);
    extract_archive(&libxml_archive, &sources);
    let libxml_install = build_libxml(
        &sources.join("libxml2-2.15.3"),
        &output,
        &jobs,
        &target,
    );
    build_lexbor(
        &sources.join("php-8.5.8/ext/lexbor"),
        &libxml_install,
        &manifest,
        &target,
    );

    println!(
        "cargo:rustc-link-search=native={}",
        libxml_install.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=xml2");
    emit_platform_links(&target);
}
