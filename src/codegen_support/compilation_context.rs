//! Purpose:
//! Stores per-thread compile profile, autoload metadata, and linked extension state.
//!
//! Called from:
//! - "crate::pipeline" before lowering and code generation.
//!
//! Key details:
//! - Thread-local storage isolates parallel compiler tests while keeping deep lowering APIs narrow.

use std::cell::{Cell, RefCell};

thread_local! {
    /// Number of `spl_autoload_register` closure rules extracted before code generation.
    static AUTOLOAD_RULE_COUNT: Cell<usize> = const { Cell::new(0) };
    /// Canonical PHP extension names supplied by linked bridge static libraries.
    static LINKED_EXTENSIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Selected PHP language profile and whether the current compilation uses web SAPI.
    static COMPILE_PROFILE: Cell<(crate::web_prelude::PhpVersion, bool)> =
        const { Cell::new((crate::web_prelude::PhpVersion::Php85, false)) };
    /// Selected output target, used by compile-time metadata such as `phpinfo()`.
    static COMPILE_TARGET: Cell<Option<crate::codegen_support::platform::Target>> =
        const { Cell::new(None) };
    /// The compile-time `--ini KEY=VALUE` directive overrides of this compilation.
    static INI_OVERRIDES: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    /// Whether `opcache.restrict_api` denies this binary's calls into the OPcache API.
    static OPCACHE_API_RESTRICTED: Cell<bool> = const { Cell::new(false) };
    /// How many scripts the compile-time OPcache manifest holds.
    static OPCACHE_MANIFEST_LEN: Cell<usize> = const { Cell::new(0) };
}

/// Records how many scripts this binary's compile-time OPcache manifest holds.
///
/// They occupy hash slots in php-src like any cached script — the entry script and every static
/// `require` are compiled into the cache before a dynamic include runs — so the runtime tier's
/// capacity is the prime MINUS these. Read via [`opcache_manifest_len`].
pub fn set_opcache_manifest_len(len: usize) {
    OPCACHE_MANIFEST_LEN.with(|cell| cell.set(len));
}

/// How many scripts this compilation's OPcache manifest holds.
pub(crate) fn opcache_manifest_len() -> usize {
    OPCACHE_MANIFEST_LEN.with(Cell::get)
}

/// Records whether `opcache.restrict_api` denies this binary's OPcache API calls.
///
/// Decided ONCE, in the pipeline, by `opcache_prelude::restrict_api_denies` — the same verdict
/// the injected native bodies bake. Carried here because the eval bridge needs it too: an
/// OPcache call the compiler cannot see (a runtime-provided `eval()` source) never gets a
/// native body, and the interpreter's own handler must refuse it just the same. Read via
/// [`opcache_api_restricted`].
pub fn set_opcache_api_restricted(restricted: bool) {
    OPCACHE_API_RESTRICTED.with(|cell| cell.set(restricted));
}

/// Whether `opcache.restrict_api` denies this compilation's OPcache API calls.
pub(crate) fn opcache_api_restricted() -> bool {
    OPCACHE_API_RESTRICTED.with(Cell::get)
}

/// Records the PHP language profile and SAPI mode of the current compilation.
///
/// Called once from `pipeline::compile` before any prelude runs, so every later
/// phase (prescan constants, EIR lowering, per-instruction lowering) observes the
/// same pair. Read via [`compile_php_version`] / [`compile_is_web_sapi`].
pub fn set_compile_profile(php_version: crate::web_prelude::PhpVersion, web: bool) {
    COMPILE_PROFILE.with(|profile| profile.set((php_version, web)));
}

/// Returns the PHP language profile this compilation targets (default: the newest
/// maintained profile, matching `--php-version`'s own default).
pub(crate) fn compile_php_version() -> crate::web_prelude::PhpVersion {
    COMPILE_PROFILE.with(|profile| profile.get().0)
}

/// Returns whether this compilation is a `--web` build (default: `false`, i.e. CLI).
pub(crate) fn compile_is_web_sapi() -> bool {
    COMPILE_PROFILE.with(|profile| profile.get().1)
}

/// Records the output target of the current compilation.
pub fn set_compile_target(target: crate::codegen_support::platform::Target) {
    COMPILE_TARGET.with(|slot| slot.set(Some(target)));
}

/// Returns the selected output target, defaulting to the host for direct codegen tests.
pub(crate) fn compile_target() -> crate::codegen_support::platform::Target {
    COMPILE_TARGET
        .with(Cell::get)
        .unwrap_or_else(crate::codegen_support::platform::Target::detect_host)
}

/// Sets the number of autoload rules registered.
pub fn set_autoload_rule_count(n: usize) {
    AUTOLOAD_RULE_COUNT.with(|c| c.set(n));
}

/// Returns the number of autoload rules registered.
pub fn autoload_rule_count() -> usize {
    AUTOLOAD_RULE_COUNT.with(|c| c.get())
}

/// Extensions every elephc binary reports as loaded, matching `extension_loaded()`.
pub(crate) const CORE_LOADED_EXTENSIONS: &[&str] = &[
    "Core",
    "standard",
    "SPL",
    "json",
    "pcre",
    "date",
    "ctype",
    "mbstring",
    "random",
    "Reflection",
    "Zend OPcache",
];

/// Returns the effective extension set: core modules plus the bridges linked into this build.
pub(crate) fn loaded_extension_names() -> Vec<String> {
    let mut names: Vec<String> = CORE_LOADED_EXTENSIONS.iter().map(|name| (*name).to_string()).collect();
    for extension in linked_extensions() {
        if !names.iter().any(|existing| existing.eq_ignore_ascii_case(&extension)) {
            names.push(extension);
        }
    }
    names
}

/// Records the canonical PHP extension names of the bridge staticlibs linked
/// into the current compilation, so `extension_loaded()` / `get_loaded_extensions()`
/// report them in addition to the always-present core set. Set from the pipeline
/// before codegen; read via [`linked_extensions`]. Passing an empty vec (the
/// default) reports only the core extensions, matching a bridge-free program.
pub fn set_linked_extensions(extensions: Vec<String>) {
    LINKED_EXTENSIONS.with(|names| *names.borrow_mut() = extensions);
}

/// Returns the canonical PHP extension names of the bridges linked into the
/// current compilation (empty unless [`set_linked_extensions`] ran for it).
pub(crate) fn linked_extensions() -> Vec<String> {
    LINKED_EXTENSIONS.with(|names| names.borrow().clone())
}

/// Records this compilation's `--ini` directive overrides.
///
/// Set from the pipeline alongside the compile profile, because the values are
/// consumed far below the parameter list that carries them — the OPcache runtime
/// cache configuration is baked in per-instruction lowering. Read via
/// [`ini_overrides`].
pub fn set_ini_overrides(overrides: Vec<(String, String)>) {
    INI_OVERRIDES.with(|entries| *entries.borrow_mut() = overrides);
}

/// Returns this compilation's `--ini` directive overrides (empty unless
/// [`set_ini_overrides`] ran for it, which is the no-override default).
pub(crate) fn ini_overrides() -> Vec<(String, String)> {
    INI_OVERRIDES.with(|entries| entries.borrow().clone())
}

/// Reports whether the compiled binary renders default diagnostics as HTML.
pub(crate) fn html_errors_enabled() -> bool {
    ini_overrides().iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("html_errors") && ini_truthy(value)
    })
}

/// Returns the compiled binary's nonempty `open_basedir` roots, unparsed.
///
/// PHP applies the last `open_basedir` directive, and the message it prints repeats the value
/// exactly as configured, so the prelude embeds this spelling rather than a normalized list.
pub(crate) fn open_basedir_roots() -> Option<String> {
    ini_overrides().iter().rev()
        .find(|(name, value)| name.eq_ignore_ascii_case("open_basedir") && !value.is_empty())
        .map(|(_, value)| unquote_ini_value(value).to_string())
        .filter(|value| !value.is_empty())
}

/// Removes PHP INI's matching single or double quotes around a directive value.
fn unquote_ini_value(value: &str) -> &str {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        return &trimmed[1..trimmed.len() - 1];
    }
    trimmed
}

/// Parses PHP's INI boolean spellings used by the overrides this compiler honours.
fn ini_truthy(value: &str) -> bool {
    !matches!(value.trim().to_ascii_lowercase().as_str(), "" | "0" | "off" | "false" | "no" | "none")
}
