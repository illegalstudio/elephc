//! Purpose:
//! Builds what a hosted PHP extension compiles and links against, from a pinned
//! php-src tarball: the PHP headers, and `libelephc_zend.a` — the Zend engine's
//! own data-structure code plus the host layer that replaces its VM.
//!
//! Called from:
//! - `super::super::recipe::CuratedRecipes` for the `php-src` package.
//!
//! Key details:
//! - No `configure` run. Of the headers an extension pulls in, only three are
//!   absent from the tarball, and all three are supplied here; the engine units
//!   compile against that same configuration.
//! - The engine is **real Zend code**, not a reimplementation: hash tables,
//!   strings, argument parsing, class registration, objects, exceptions and INI
//!   come from php-src itself ([`ENGINE_UNITS`]). A hand-written stand-in would
//!   drift from PHP one ownership rule at a time; these units cannot.
//! - What Zend cannot supply without its executor — engine globals, error
//!   output, the active-frame queries, the bailout target — is `runtime.c`, and
//!   the ABI compiled Elephc code calls is `host.c`. Both are embedded so an
//!   installed Elephc needs no repository access, and both are covered by
//!   [`ENGINE_FINGERPRINT`] so an edit cannot ship under a stale recipe revision.
//! - Headers are staged in the installed layout (`include/Zend/…`), copying
//!   exactly the catalog's retained list: `materialize` refuses a staging tree
//!   holding a file the catalog did not name.

use std::fs;
use std::path::{Path, PathBuf};

use super::super::error::{NativeError, NativeErrorKind};
use super::super::recipe::RecipeRequest;
use super::super::toolchain::run_checked;

/// The three headers `./configure` would generate, embedded so an installed
/// Elephc needs no repository access.
pub const PHP_CONFIG_H: &str = include_str!("php_src/php_config.h");
pub const ZEND_CONFIG_H: &str = include_str!("php_src/zend_config.h");
pub const BUILD_DEFS_H: &str = include_str!("php_src/build-defs.h");

/// Engine globals, error output and the VM-only remainder of Zend.
pub const RUNTIME_C: &str = include_str!("php_src/runtime.c");
/// The ABI generated wrappers call: startup, call frames, value transfer,
/// introspection.
pub const HOST_C: &str = include_str!("php_src/host.c");
/// The standard-library state, MINIT registrations and refusals extensions
/// link against (globals, stream wrappers, output handlers, serialize).
pub const STDLIB_C: &str = include_str!("php_src/stdlib.c");

/// The static archive every hosted extension resolves its Zend symbols from.
pub const ENGINE_ARCHIVE: &str = "lib/libelephc_zend.a";

/// php-src translation units compiled into the engine archive, relative to the
/// source root. Chosen by link closure: together with `runtime.c` they resolve
/// every engine symbol a hosted extension needs except the VM itself.
pub const ENGINE_UNITS: &[&str] = &[
    "Zend/zend_alloc.c",
    "Zend/zend_API.c",
    "Zend/zend_constants.c",
    "Zend/zend_exceptions.c",
    "Zend/zend_hash.c",
    "Zend/zend_hrtime.c",
    "Zend/zend_inheritance.c",
    "Zend/zend_ini.c",
    "Zend/zend_interfaces.c",
    "Zend/zend_iterators.c",
    "Zend/zend_list.c",
    "Zend/zend_object_handlers.c",
    "Zend/zend_objects.c",
    "Zend/zend_objects_API.c",
    "Zend/zend_operators.c",
    "Zend/zend_smart_str.c",
    "Zend/zend_sort.c",
    "Zend/zend_string.c",
    "Zend/zend_strtod.c",
    "Zend/zend_variables.c",
    "main/explicit_bzero.c",
    "main/php_scandir.c",
    "main/snprintf.c",
    "main/spprintf.c",
    // Standard-library units extensions call directly, each self-contained on
    // top of the engine: base64 (zstd), digests (zstd, apcu), the incomplete
    // class used when unserializing an unknown class (msgpack, igbinary).
    "ext/hash/hash_sha.c",
    "ext/standard/base64.c",
    "ext/standard/incomplete_class.c",
    "ext/standard/md5.c",
    "ext/standard/sha1.c",
    // PHP's own serialize()/unserialize(): APCu's default serializer, and what
    // msgpack, igbinary and ds fall back to for objects.
    "ext/standard/var.c",
    "ext/standard/var_unserializer.c",
];

/// Recipe revision the embedded engine sources were last published under, and
/// their SHA-256. `the_engine_fingerprint_matches_the_recipe_revision` fails on
/// any edit to them until both the catalog revision and this pair move.
#[cfg(test)]
pub const ENGINE_FINGERPRINT: (u32, &str) =
    (1, "fcf31cb63a95c5365719e1c86094269aff8de8c7ef1c679cfe4ff032fb46ae20");

/// Headers Elephc writes itself rather than copying from the tarball.
const OWNED_HEADERS: &[&str] = &[
    "include/main/php_config.h",
    "include/Zend/zend_config.h",
    "include/main/build-defs.h",
];

fn build_error(message: impl Into<String>) -> NativeError {
    NativeError::new(NativeErrorKind::Build, message)
}

/// Stages headers and builds the engine archive into the staging prefix.
pub fn build(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    stage_headers(request)?;
    build_engine(request)
}

/// Copies exactly the retained header list, then writes the owned headers.
fn stage_headers(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    for retained in request.version.retained_headers {
        if OWNED_HEADERS.contains(retained) {
            continue;
        }
        let relative = retained.strip_prefix("include/").ok_or_else(|| {
            build_error(format!("retained header '{retained}' is not under include/"))
        })?;
        let source = request.source.join(relative);
        if !source.is_file() {
            return Err(build_error(format!(
                "retained header '{relative}' is missing from the php-src tree: the catalog list \
                 and the pinned tarball disagree"
            )));
        }
        let target = request.staging_prefix.join(retained);
        create_parent(&target)?;
        fs::copy(&source, &target)
            .map_err(|error| NativeError::io("stage php header", &source, error))?;
    }

    // The three configure would have generated. Written last so they overwrite
    // nothing and their absence upstream is obvious in the diff.
    let include = request.staging_prefix.join("include");
    write_file(&include.join("main").join("php_config.h"), PHP_CONFIG_H)?;
    write_file(&include.join("Zend").join("zend_config.h"), ZEND_CONFIG_H)?;
    write_file(&include.join("main").join("build-defs.h"), BUILD_DEFS_H)?;
    Ok(())
}

/// Include directories in the order an installed PHP's `php-config --includes`
/// lists them, rooted at a staged `include/`.
pub fn include_dirs(include: &Path) -> Vec<PathBuf> {
    ["", "main", "Zend", "TSRM", "ext"]
        .iter()
        .map(|sub| if sub.is_empty() { include.to_path_buf() } else { include.join(sub) })
        .collect()
}

/// Compiles the engine units and the embedded host layer into one archive.
fn build_engine(request: &RecipeRequest<'_>) -> Result<(), NativeError> {
    let build = request.staging_prefix.join("engine-build");
    create_dir(&build, "create Zend engine build directory")?;
    let include = request.staging_prefix.join("include");
    let includes = include_dirs(&include);

    let mut sources: Vec<PathBuf> = Vec::with_capacity(ENGINE_UNITS.len() + 3);
    for unit in ENGINE_UNITS {
        let source = request.source.join(unit);
        if !source.is_file() {
            return Err(build_error(format!(
                "engine unit '{unit}' is missing from the php-src tree: the recipe and the pinned \
                 tarball disagree"
            )));
        }
        sources.push(source);
    }
    for (name, contents) in [
        ("elephc_zend_runtime.c", RUNTIME_C),
        ("elephc_zend_host.c", HOST_C),
        ("elephc_zend_stdlib.c", STDLIB_C),
    ] {
        let source = build.join(name);
        write_file(&source, contents)?;
        sources.push(source);
    }

    let mut objects = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let stem = source.file_stem().and_then(|stem| stem.to_str()).unwrap_or("unit");
        let object = build.join(format!("{index:02}-{stem}.o"));
        let mut compile = request.toolchain.command(&request.toolchain.cc);
        compile.args(engine_compile_flags());
        for dir in &includes {
            compile.arg("-I").arg(dir);
        }
        compile.arg("-c").arg(source).arg("-o").arg(&object);
        run_checked(&mut compile, "compile Zend engine unit")?;
        objects.push(object);
    }

    let archive = request.staging_prefix.join(ENGINE_ARCHIVE);
    create_parent(&archive)?;
    let mut create = request.toolchain.command(&request.toolchain.ar);
    create.arg("crs").arg(&archive).args(&objects);
    run_checked(&mut create, "archive Zend engine")?;
    let mut ranlib = request.toolchain.command(&request.toolchain.ranlib);
    ranlib.arg(&archive);
    run_checked(&mut ranlib, "index Zend engine archive")?;
    let mut inspect = request.toolchain.command(&request.toolchain.ar);
    inspect.arg("t").arg(&archive);
    run_checked(&mut inspect, "validate Zend engine archive")?;

    fs::remove_dir_all(&build)
        .map_err(|error| NativeError::io("remove Zend engine build tree", &build, error))
}

/// Flags for engine units. php-src's own sources are compiled with warnings
/// off: they are upstream code built under a configuration php's build never
/// uses, and their warnings are not Elephc's to fix.
fn engine_compile_flags() -> Vec<&'static str> {
    vec!["-std=gnu17", "-O2", "-fPIC", "-w"]
}

fn create_dir(path: &Path, action: &str) -> Result<(), NativeError> {
    fs::create_dir_all(path).map_err(|error| NativeError::io(action, path, error))
}

fn create_parent(path: &Path) -> Result<(), NativeError> {
    match path.parent() {
        Some(parent) => create_dir(parent, "create staged directory"),
        None => Ok(()),
    }
}

fn write_file(path: &Path, contents: &str) -> Result<(), NativeError> {
    create_parent(path)?;
    fs::write(path, contents).map_err(|error| NativeError::io("write Elephc-owned php source", path, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// Every embedded engine source, in a fixed order, so the fingerprint is
    /// stable across builds.
    fn engine_sources_digest() -> String {
        let mut digest = Sha256::new();
        for part in [PHP_CONFIG_H, ZEND_CONFIG_H, BUILD_DEFS_H, RUNTIME_C, HOST_C, STDLIB_C] {
            digest.update(part.as_bytes());
            digest.update([0u8]);
        }
        for unit in ENGINE_UNITS {
            digest.update(unit.as_bytes());
            digest.update([0u8]);
        }
        format!("{:x}", digest.finalize())
    }

    /// An installed artifact is keyed by recipe revision, not by content: an
    /// edit to the engine that kept the revision would leave every existing
    /// cache serving the old engine. This pins the content to the revision.
    #[test]
    fn the_engine_fingerprint_matches_the_recipe_revision() {
        let revision = crate::native_deps::catalog::version("php-src", None)
            .expect("php-src is catalogued")
            .recipe_revision;
        let digest = engine_sources_digest();
        assert_eq!(
            ENGINE_FINGERPRINT,
            (revision, digest.as_str()),
            "the embedded engine sources changed: bump php-src's recipe_revision in the catalog, \
             its match arm in recipe.rs, and set ENGINE_FINGERPRINT to ({}, \"{digest}\")",
            revision + 1
        );
    }

    /// The whole reason this is not a configure run: the configuration is
    /// small enough to own outright.
    #[test]
    fn the_owned_configuration_is_small() {
        let defines = PHP_CONFIG_H
            .lines()
            .filter(|l| l.trim_start().starts_with("#define"))
            .count();
        assert!(
            (10..=40).contains(&defines),
            "php_config.h should stay a small owned file, found {defines} defines"
        );
    }

    /// These are exactly the headers `./configure` generates and the tarball
    /// therefore lacks; if one went missing, extensions would fail to compile
    /// with an error pointing at php rather than at us.
    #[test]
    fn supplies_every_header_the_tarball_lacks() {
        assert!(PHP_CONFIG_H.contains("ZEND_API"), "visibility is defined here, not in zend_portability.h");
        assert!(PHP_CONFIG_H.contains("SIZEOF_SIZE_T"), "zval layout depends on this");
        assert!(PHP_CONFIG_H.contains("ZEND_MM_ALIGNMENT"), "zend_alloc.h refuses without it");
        assert!(ZEND_CONFIG_H.contains("php_config.h"), "zend_config.h is a redirect");
        assert!(BUILD_DEFS_H.contains("PHP_EXTENSION_DIR"), "extensions read install paths");
        for owned in OWNED_HEADERS {
            assert!(
                crate::native_deps::php_src_headers::RETAINED_HEADERS.contains(owned),
                "{owned} is written by the recipe, so the catalog must retain it"
            );
        }
    }

    /// Hosted extensions are release builds; a debug-mode mismatch against a
    /// release engine would be an ABI difference, not a preference.
    #[test]
    fn pins_release_mode() {
        assert!(PHP_CONFIG_H.contains("#define ZEND_DEBUG 0"));
    }

    /// A fatal raised inside an extension must longjmp to a frame the host
    /// installed, and every VM-only path must fail loudly rather than return
    /// something plausible — static linking proves a symbol exists, never that
    /// it behaves.
    #[test]
    fn the_host_layer_keeps_its_safety_contracts() {
        assert!(HOST_C.contains("SETJMP(protected_frame)"), "calls run inside a protected frame");
        assert!(RUNTIME_C.contains("with no protected frame"), "an unprotected bailout is reported, not followed");
        assert!(RUNTIME_C.contains("ELEPHC_UNSUPPORTED(\"zend_call_function"), "callbacks into PHP code fail loudly");
        assert!(
            HOST_C.contains("nTableMask = list ? ELEPHC_PACKED_MASK"),
            "exported tables must use the zval bridge's packed marker, never PHP 8.2 packed arrays"
        );
    }

    #[test]
    fn include_dirs_follow_php_config_order() {
        let dirs = include_dirs(Path::new("/p/include"));
        assert_eq!(dirs[0], PathBuf::from("/p/include"));
        assert_eq!(dirs[1], PathBuf::from("/p/include/main"));
        assert_eq!(dirs[2], PathBuf::from("/p/include/Zend"));
    }
}
