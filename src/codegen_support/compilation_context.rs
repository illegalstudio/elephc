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
    /// The compile-time `--ini KEY=VALUE` directive overrides of this compilation.
    static INI_OVERRIDES: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
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

/// Sets the number of autoload rules registered.
pub fn set_autoload_rule_count(n: usize) {
    AUTOLOAD_RULE_COUNT.with(|c| c.set(n));
}

/// Returns the number of autoload rules registered.
pub fn autoload_rule_count() -> usize {
    AUTOLOAD_RULE_COUNT.with(|c| c.get())
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
