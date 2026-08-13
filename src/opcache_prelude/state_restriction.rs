//! Purpose:
//! Renders mutable OPcache state and compile-time restrict_api gates.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - Warning text and entry-path prefix matching remain reference-verified.

#[allow(unused_imports)]
use super::*;

/// The `opcache.restrict_api` directive name.
pub(super) const RESTRICT_API_DIRECTIVE: &str = "opcache.restrict_api";

/// The VERBATIM `E_WARNING` text php-src's OPcache API guard emits when
/// `opcache.restrict_api` denies a call. In php-src this is
/// `zend_error(E_WARNING, ACCELERATOR_PRODUCT_NAME " API is restricted by \"restrict_api\"
/// configuration directive")` with `ACCELERATOR_PRODUCT_NAME` = `"Zend OPcache"`.
///
/// VERIFIED byte-for-byte against reference PHP 8.5.6 (Homebrew, `Zend OPcache` loaded):
/// `php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.restrict_api=/nonexistent`
/// emits exactly `Warning: Zend OPcache API is restricted by "restrict_api" configuration
/// directive in <file> on line <n>`. Elephc reproduces the message but not the
/// ` in <file> on line <n>` suffix, which it does not synthesize — the same documented
/// shortfall the `opcache_compile_file` notice carries (see `COMPILE_FILE_TEMPLATE`).
pub(super) const RESTRICT_API_WARNING_TEXT: &str =
    "Zend OPcache API is restricted by \"restrict_api\" configuration directive";

/// Decides — AT COMPILE TIME — whether `opcache.restrict_api` denies this binary's calls into
/// the OPcache API, reproducing php-src's `validate_api_restriction()` exactly.
///
/// php-src:
/// ```c
/// if (ZCG(accel_directives).restrict_api && *ZCG(accel_directives).restrict_api) {
///     size_t len = strlen(ZCG(accel_directives).restrict_api);
///     if (!SG(request_info).path_translated ||
///         strlen(SG(request_info).path_translated) < len ||
///         memcmp(SG(request_info).path_translated, ZCG(accel_directives).restrict_api, len) != 0) {
///         zend_error(E_WARNING, ...); return 0;
///     }
/// }
/// ```
///
/// Every rule below is VERIFIED against reference PHP 8.5.6, not merely derived from the source:
/// - EMPTY prefix disables the restriction entirely (`restrict_api=` → allowed).
/// - The comparison target is the ENTRY SCRIPT, not the currently-executing file. PROVEN with an
///   entry in one directory that `require`s a script in another and calls the API from there:
///   `restrict_api=<entry's dir>` ALLOWED the call, `restrict_api=<includee's dir>` DENIED it.
///   This is precisely what makes the compile-time evaluation exact for elephc — the entry script
///   is fixed when the binary is built.
/// - It is a PLAIN BYTE PREFIX, NOT a path-component match: prefix `/private/tmp/ra/foo` ALLOWS
///   entry `/private/tmp/ra/foobar/x.php`. (`str::starts_with` on `&str` is a byte compare, so it
///   reproduces `memcmp` verbatim.)
/// - It is CASE-SENSITIVE even on a case-insensitive filesystem: prefix `…/Foobar` DENIES entry
///   `…/foobar/x.php` (memcmp, not a filesystem lookup).
/// - A prefix LONGER than the entry path denies (the `strlen(...) < len` arm).
/// - A prefix EQUAL to the whole entry path allows.
/// - The path compared is the RESOLVED one: invoking `php /tmp/ra/foobar/x.php` on macOS (where
///   `/tmp` symlinks to `/private/tmp`) DENIES prefix `/tmp/ra` and ALLOWS `/private/tmp/ra`.
///   That is why `entry_path` must be the canonicalized path — the same canonicalization
///   `__FILE__` and `ScriptEntry::path` use.
///
/// `entry_path` of `None` denies, mirroring php-src's `!SG(request_info).path_translated` arm
/// (no entry script to compare against ⇒ the restriction cannot be satisfied).
///
/// COMPILE-TIME vs RUNTIME: reference PHP evaluates this per request against the live script
/// path. An elephc AOT binary has exactly one entry script, fixed when it was compiled, and
/// `--ini` is a compile-time flag — so the predicate has no runtime-varying input and baking its
/// result loses nothing.
pub(super) fn restrict_api_denies(
    entry_path: Option<&str>,
    version_id: u32,
    overrides: &[(String, String)],
) -> bool {
    let prefix = directive_str(version_id, RESTRICT_API_DIRECTIVE, overrides);
    if prefix.is_empty() {
        return false;
    }
    match entry_path {
        // php-src's `!SG(request_info).path_translated` arm: nothing to compare ⇒ deny.
        None => true,
        // `starts_with` on `&str` compares bytes, matching php-src's `memcmp`; a prefix longer
        // than the path can never match, covering the `strlen(path) < len` arm.
        Some(path) => !path.starts_with(&prefix),
    }
}

/// Canonicalizes the compile-time entry script path for the `opcache.restrict_api` comparison.
///
/// Uses `Path::canonicalize`, the SAME normalization `__FILE__` bakes
/// (`crate::magic_constants::file_pass`) and `collect_manifest` applies to `ScriptEntry::path`,
/// because reference PHP compares the RESOLVED script path (verified: on macOS a `/tmp/...`
/// invocation is compared as `/private/tmp/...`). Returns `None` when the path cannot be
/// resolved, which `restrict_api_denies` treats as php-src's null-`path_translated` deny arm.
///
/// This is deliberately NOT read back out of the manifest: `collect_manifest` skips any entry it
/// cannot stat, so a manifest's first element is only *usually* the entry file — and silently
/// comparing an `autoload.files` path instead would flip a security-shaped decision.
pub fn canonical_entry_path(main_file: &str) -> Option<String> {
    Path::new(main_file)
        .canonicalize()
        .ok()
        .map(|path| path.display().to_string())
}

/// The `opcache.restrict_api` diagnostic, or `None` when the API is not denied — the decision
/// the PHP form expressed by deleting a placeholder line.
pub(super) fn restrict_api_warning(restricted: bool) -> Option<Stmt> {
    restricted.then(|| {
        build::restrict_api_warning_stmt(&format!("Warning: {RESTRICT_API_WARNING_TEXT}"))
    })
}
