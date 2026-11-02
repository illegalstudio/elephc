//! Purpose:
//! Renders the CLI ini_get, ini_set, and ini_get_all compatibility surface.
//!
//! Called from:
//! - The OPcache prelude facade and sibling rendering modules.
//!
//! Key details:
//! - User declarations and module filters preserve pay-for-use injection.

#[allow(unused_imports)]
use super::*;

/// The module names `ini_get_all`'s extension filter RECOGNIZES, used to tell "known module with
/// no INI directives" (`[]`) from "no such module" (`E_WARNING` + `false`).
///
/// LOWERCASED, and compared verbatim against lowercase registry keys: reference PHP does NOT
/// case-fold this argument, so this must not share a comparison helper with `extension_loaded`,
/// which does.
///
/// THIS IS DELIBERATELY NOT THE `extension_loaded()` SET, and it used to be. `extension_loaded()`
/// answers "can every function of this extension be called", which elephc now derives from the
/// shared catalog's php-src coverage — so `mbstring` (2 of php's 65 functions) and `ctype` (4 of
/// 11) dropped out of it. `ini_get_all($module)` asks a different question: reference PHP 8.5
/// returns `[]` for `json`, `spl`, `ctype`, `reflection` and `standard` because it RECOGNIZES the
/// module and it simply declares no INI directives. Recognition survives partial function
/// coverage; the promise `extension_loaded()` makes does not. Keeping them one set would have made
/// `ini_get_all('ctype')` start warning about a module elephc plainly knows.
///
/// Bridge-linked extensions (`PDO`, `hash`, …) are deliberately NOT included: they are a
/// per-compilation link-set decision made in codegen, while this prelude is built before codegen.
pub(crate) const INI_KNOWN_MODULES: &[&str] = &[
    "Core",
    "standard",
    "SPL",
    "json",
    "pcre",
    "date",
    "ctype",
    "mbstring",
    "Reflection",
    // PHP ships ext/session in both the CLI and built-in server SAPIs.
    "session",
    "Zend OPcache",
];

/// Builds the `__elephc_ini_module_known($m)` declaration over [`INI_KNOWN_MODULES`].
///
/// `web` is retained for callers compiled against the older API; `session` is now part of
/// the normal CLI module set as well as the web set.
pub(crate) fn ini_module_known_declaration(web: bool) -> Stmt {
    let mut names: Vec<String> = INI_KNOWN_MODULES
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    if web && !names.iter().any(|name| name == "session") {
        names.push("session".to_string());
    }
    build::ini_module_known_decl(&names)
}

/// The PHP-visible names this module's wrappers declare.
///
/// Spelled once so the two injection sites cannot disagree about what "the CLI INI surface" is.
const CLI_INI_SURFACE: [&str; 3] = ["ini_get", "ini_set", "ini_get_all"];

/// The SECOND injection site for the CLI `ini_get`/`ini_set`/`ini_get_all` wrappers, run from
/// `pipeline::compile`'s `compat-preludes` phase — AFTER `autoload::run`.
///
/// WHY A SECOND SITE, AND WHY THIS PHASE. `injection::inject_if_used` already gates these three
/// on `detect::program_references`, but it runs in the `opcache-prelude` phase, which is before
/// name resolution and therefore before autoload expansion. The program it sees is the entry
/// file plus its statically resolved includes — and the code that actually calls `ini_set()` is
/// usually neither. Symfony's console entry is 21 lines that mention no INI function at all;
/// `ini_set()` is called from a class the autoload pass splices in at `autoload-run`, so the
/// early gate injected nothing and the binary died with `Call to undefined function ini_set()`.
/// MEASURED on `examples/symfony-app/bin/console`. This is the same placement lesson
/// `error_handling_prelude::inject_if_used` records, and the same phase.
///
/// ONE IMPLEMENTATION, TWO SITES: both call the same `build::cli_ini_*_decl` builders and the
/// same helper blocks. The early site is left in place rather than moved, so a program that DOES
/// name an INI function in its entry file compiles exactly as it did before; this pass then adds
/// only what is still missing. Every piece is guarded by `detect::program_declares` against the
/// post-autoload program, which makes "already injected early" and "the user wrote their own"
/// the same no-op — so the two sites can never both emit the same declaration.
///
/// Returns the program untouched under `--web`: there `web_prelude::build::web_declarations`
/// owns all three names (with the session dispatch layered on top of the shared `opcache.*`
/// helpers), and a second copy would be a redeclaration.
pub fn inject_cli_ini_if_used(
    program: Program,
    php_version: PhpVersion,
    overrides: &[(String, String)],
    web: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if web {
        return program;
    }
    let needed: Vec<&str> = CLI_INI_SURFACE
        .iter()
        .copied()
        .filter(|name| {
            detect::program_references(&program, name) && !detect::program_declares(&program, name)
        })
        .collect();
    if needed.is_empty() {
        return program;
    }

    // Built under the internal source mode for the same reason the early site is: the mode is
    // read off a thread-local as each node is created, and this runs inside a pipeline that has
    // `SourceMode::Php` installed for the user's program.
    let mut declarations = crate::synthetic_class::internal_declarations(|| {
        let mut declarations: Program = Vec::new();
        // The runtime `ELEPHC_INI_*` block the raw-string arms call into.
        // `opcache_get_configuration()` needs it too, so the early site may already have it out.
        if !detect::program_declares(&program, "__elephc_opcache_env") {
            declarations.extend(env_override_declarations());
        }
        if !detect::program_declares(&program, "__elephc_opcache_ini_string") {
            declarations.extend(ini_helper_declarations(php_version, overrides));
        }
        if needed.contains(&"ini_get") {
            declarations.push(build::cli_ini_get_decl());
        }
        if needed.contains(&"ini_set") {
            declarations.push(build::cli_ini_set_decl());
        }
        if needed.contains(&"ini_get_all") {
            // The known-module predicate is only reachable from `ini_get_all`'s extension filter,
            // so it travels with it rather than with the shared helpers.
            if !detect::program_declares(&program, "__elephc_ini_module_known") {
                declarations.push(ini_module_known_declaration(false));
            }
            declarations.push(build::cli_ini_get_all_decl());
        }
        declarations
    });

    // The same group id the early site records under: the two contribute to one prelude as far
    // as `prune_unreachable_prelude_functions` is concerned, and `record_program` merges.
    inventory.record_program("opcache", &declarations);
    declarations.extend(program);
    declarations
}
