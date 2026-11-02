//! Purpose:
//! Derives the PHP extensions elephc may honestly report as loaded from the shared builtin
//! catalog itself, so `extension_loaded()` and `get_loaded_extensions()` describe the function
//! surface that actually exists instead of a hand-written name list.
//!
//! Called from:
//! - `elephc::codegen::lower_inst::builtins::scalar_metadata` (the AOT fold and dynamic test).
//! - `elephc::codegen::lower_inst::builtins::types` (`get_loaded_extensions`).
//! - `elephc_magician::interpreter::builtins::network_env` (the eval mirrors).
//!
//! Key details:
//! - A module is reportable only when [`PhpModule::covers_php_function_surface`] holds, or when it
//!   is one of the three engine surfaces php-src always builds
//!   ([`PhpModule::is_engine_surface`]); whether this particular compilation can serve those
//!   functions is the CALLER's question, because only the backend knows its target, language
//!   profile, and injected preludes.
//! - Language constructs, dedicated syntax, and compiler-internal entries are not extension
//!   functions and never take part in the decision.

use crate::{contracts, BuiltinContract, BuiltinKind, PhpModule};

/// Returns the PHP-visible FUNCTION contracts one module owns.
///
/// `internal` entries are compiler plumbing rather than PHP surface, and `LanguageConstruct` /
/// `DedicatedSyntax` entries (`isset`, `empty`, `exit`, `buffer_new`, …) are parser forms php does
/// not attribute to an extension at all — reference PHP's `get_extension_funcs('Core')` does not
/// list them. Both are excluded so the availability question asked about a module is only ever
/// asked about names a program can actually call as functions.
pub fn module_php_functions(module: PhpModule) -> impl Iterator<Item = &'static BuiltinContract> {
    contracts().iter().filter(move |contract| {
        contract.module == module
            && !contract.internal
            && matches!(
                contract.kind,
                BuiltinKind::Function
                    | BuiltinKind::PreludeProvided
                    | BuiltinKind::NameResolverRewrite
            )
    })
}

/// php's module registration order, as reference PHP 8.5's `get_loaded_extensions()` lists it.
///
/// php numbers modules as it registers them: `Core` first, then the static extensions in the
/// order the build configured them, the Zend extension among them. MEASURED (8.5, static build):
/// `Core,date,lexbor,openssl,pcre,sqlite3,zlib,bcmath,…,odbc,Zend OPcache,pcntl,…,zip`. Both
/// backends list what they report in this order, so `get_loaded_extensions()[0]` is `Core` as it
/// is in every php, and an AOT and an eval read of the same program agree on positions.
const PHP_REGISTRATION_ORDER: &[&str] = &[
    "Core", "date", "lexbor", "openssl", "pcre", "sqlite3", "zlib", "bcmath", "bz2", "calendar",
    "ctype", "curl", "dba", "uri", "json", "mbstring", "SPL", "FFI", "fileinfo", "filter", "ftp",
    "gd", "gettext", "gmp", "hash", "iconv", "intl", "session", "ldap", "standard", "libxml",
    "mysqlnd", "mysqli", "odbc", "Zend OPcache", "pcntl", "PDO", "pdo_dblib", "pdo_mysql",
    "PDO_ODBC", "pdo_pgsql", "pdo_sqlite", "pgsql", "Phar", "posix", "random", "readline",
    "Reflection", "exif", "shmop", "SimpleXML", "snmp", "soap", "sockets", "sodium", "sysvmsg",
    "sysvsem", "sysvshm", "tidy", "tokenizer", "dom", "xml", "xmlreader", "xmlwriter", "xsl", "zip",
];

/// Returns where php registers an extension name, case-insensitively; names php does not
/// bundle (PECL modules elephc provides) sort after every bundled one.
pub fn php_registration_rank(name: &str) -> usize {
    PHP_REGISTRATION_ORDER
        .iter()
        .position(|known| known.eq_ignore_ascii_case(name))
        .unwrap_or(PHP_REGISTRATION_ORDER.len())
}

/// Returns every module elephc may report as loaded, in declaration order.
///
/// Two admissions, and they are different claims:
///
/// - [`PhpModule::covers_php_function_surface`] — elephc declares php-src's whole function list
///   for it, so "loaded" is a promise it can keep. The backend still has to check that THIS
///   compilation can serve each of those functions.
/// - [`PhpModule::is_engine_surface`] — `Core`, `standard` and `pcre`, which php-src cannot be
///   built without. These are admitted DESPITE partial coverage, because nothing polyfills them
///   and no PHP process is ever without them; the backend reports them unconditionally.
///
/// A module elephc has no contract for at all is never a candidate, so a completeness flag can
/// never make an absent extension vacuously "complete".
pub fn complete_surface_modules() -> impl Iterator<Item = PhpModule> {
    PhpModule::ALL
        .iter()
        .copied()
        .filter(|module| {
            module.is_php() && (module.covers_php_function_surface() || module.is_engine_surface())
        })
        .filter(|module| module_php_functions(*module).next().is_some() || *module == PhpModule::Reflection)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every reportable module's display name has a registration rank, so none sorts to the end
    /// by a spelling mismatch; `Core` ranks first and the Zend extension sits among the others.
    #[test]
    fn reportable_modules_have_a_php_registration_rank() {
        assert_eq!(php_registration_rank("Core"), 0);
        assert_eq!(php_registration_rank("core"), 0);
        assert!(php_registration_rank("Zend OPcache") > php_registration_rank("standard"));
        assert!(php_registration_rank("Zend OPcache") < PHP_REGISTRATION_ORDER.len());
        for module in complete_surface_modules() {
            assert!(
                php_registration_rank(module.display_name()) < PHP_REGISTRATION_ORDER.len(),
                "{} has no php registration rank",
                module.display_name()
            );
        }
    }

    /// Every complete-surface module must own at least one catalog function, except `Reflection`,
    /// which php exports no procedural functions for at all and which elephc provides as classes.
    ///
    /// Without this, adding a module name to `covers_php_function_surface` for an extension elephc
    /// has nothing for would report it loaded on a vacuous "no function is missing".
    #[test]
    fn complete_modules_own_catalog_functions() {
        for module in complete_surface_modules() {
            if module == PhpModule::Reflection {
                continue;
            }
            assert!(
                module_php_functions(module).next().is_some(),
                "module {} is marked complete but owns no catalog function",
                module.php_name()
            );
        }
        assert!(complete_surface_modules().any(|module| module == PhpModule::Json));
        assert!(!complete_surface_modules().any(|module| module == PhpModule::Mbstring));
        assert!(!complete_surface_modules().any(|module| module == PhpModule::Ctype));
        // The three engine surfaces are admitted despite partial coverage, on purpose.
        assert!(complete_surface_modules().any(|module| module == PhpModule::Core));
        assert!(complete_surface_modules().any(|module| module == PhpModule::Standard));
        assert!(complete_surface_modules().any(|module| module == PhpModule::Pcre));
        assert!(!PhpModule::Core.covers_php_function_surface());
    }

    /// Language constructs and internal helpers are not extension functions.
    #[test]
    fn module_functions_exclude_constructs_and_internals() {
        let core: Vec<&str> = module_php_functions(PhpModule::Core)
            .map(|contract| contract.name)
            .collect();
        assert!(core.contains(&"extension_loaded"));
        assert!(!core.contains(&"isset"), "isset is dedicated syntax, not a Core function");
        assert!(!core.contains(&"exit"), "exit is a language construct, not a Core function");
    }
}
