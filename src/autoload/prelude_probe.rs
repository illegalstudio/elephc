//! Purpose:
//! Decides which detected standard-library preludes the AUTOLOAD CLOSURE of a program needs,
//! before those preludes are injected.
//!
//! Called from:
//! - `crate::pipeline::compile()`, right before the prelude injections.
//!
//! Key details:
//! - Each prelude (PDO, mysqli, tz, image, hash, tokenizer, curl, xml, fileinfo, var_export,
//!   web environment, web request, random, bcmath, gmp, listIdentifiers, preg_grep, OPcache/INI,
//!   version, and core's WeakMap/WeakReference) is injected only when the program uses its
//!   surface, and that decision has to be taken before name resolution so a
//!   namespaced caller resolves to the injected declarations. The Composer/SPL autoload pass runs
//!   AFTER name resolution, so a class file it loads that uses `PDO` or `CurlHandle` was never
//!   scanned: the prelude was skipped and the program failed with `Unknown type: PDO`.
//! - The probe runs name resolution and the autoload pass on a COPY of the program, scans the
//!   result with each prelude's own detector, and hands back the groups found. The real pass then
//!   injects them exactly as if the entry file had used them, so the outcome is the one
//!   `--with-<crate>` gives, without forcing the whole surface past reachability pruning.
//! - The copy's work is discarded, restoring thread-local source IDs and anonymous-class
//!   numbers so the real parse keeps those identities. Process-global synthetic spans and
//!   include temporaries remain monotonic, preserving identities other threads allocated.
//! - A probe failure is not reported here: the real pass hits the same error and reports it.
//! - The web environment group retains process entries when an autoloaded file observes them.
//! - The OPcache and version preludes decide per FUNCTION (`ini_get`, `ini_set`, `opcache_reset`,
//!   `ini_restore`, ...), so [`AutoloadedFunctions`] reports, apart from the groups, which of
//!   their names the closure references, which it declares itself (a user declaration wins, as
//!   in the entry program), and whether it carries an interpreter-backed `eval`.
//!   The closure is name-resolved, so a call matches on its resolved name: a vendor's own
//!   `Vendor\Util\ini_get()` is not the global `ini_get`.

use std::collections::HashSet;
use std::path::Path;

use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;

use super::Registry;

#[cfg(test)]
#[path = "prelude_probe_tests.rs"]
mod tests;

/// Every prelude group the probe can report, with the detector that decides it.
const PROBED_GROUPS: &[(&str, fn(&[Stmt]) -> bool)] = &[
    ("pdo", crate::pdo_prelude::program_uses_pdo),
    ("mysqli", crate::mysqli_prelude::program_uses_mysqli),
    ("tz", crate::tz_prelude::program_uses_surface),
    ("image", crate::image_prelude::program_uses_surface),
    ("hash", crate::hash_prelude::program_uses_surface),
    ("tokenizer", crate::tokenizer_prelude::program_uses_surface),
    ("curl", crate::curl_prelude::program_uses_surface),
    ("xml", crate::xml_prelude::program_uses_surface),
    ("tidy", crate::tidy_prelude::program_uses_surface),
    ("zip", crate::zip_prelude::program_uses_surface),
    ("fileinfo", crate::fileinfo_prelude::program_uses_surface),
    ("var_export", crate::var_export_prelude::program_uses_surface),
    ("web_environment", crate::web_prelude::program_observes_environment),
    ("web_request", crate::web_prelude::program_observes_request),
    ("random", crate::random_prelude::program_uses_random),
    ("bcmath", crate::bcmath_prelude::program_uses_bcmath),
    ("gmp", crate::gmp_prelude::program_uses_gmp),
    ("list_id", crate::list_id_prelude::program_uses_list_identifiers),
    ("weak_map", crate::core_prelude::program_uses_weak_map),
    ("weak_reference", crate::core_prelude::program_uses_weak_reference),
    ("pcre", crate::pcre_prelude::program_uses_surface),
    ("uri", crate::uri_prelude::program_uses_uri),
];

/// Every prelude function decided per name (the OPcache/INI and version preludes' own lists).
const PROBED_FUNCTIONS: &[&[&str]] = &[
    crate::opcache_prelude::PROBED_FUNCTIONS,
    crate::version_prelude::PROBED_FUNCTIONS,
];

/// What the probe found in the autoload closure.
#[derive(Debug, Default)]
pub struct AutoloadedPreludes {
    /// The `PROBED_GROUPS` names whose surface the closure uses.
    pub groups: HashSet<&'static str>,
    /// The per-function facts the OPcache/INI and version preludes decide on.
    pub functions: AutoloadedFunctions,
}

/// What the autoload closure says about the functions the OPcache/INI and version preludes
/// inject one by one: those preludes are decided before Composer class files are loaded.
///
/// The default value describes a program without an autoload closure, which is what every
/// caller outside the CLI pipeline passes.
#[derive(Debug, Default, Clone)]
pub struct AutoloadedFunctions {
    referenced: HashSet<&'static str>,
    declared: HashSet<&'static str>,
    bridge_eval: bool,
}

impl AutoloadedFunctions {
    /// Whether a file of the closure references the global function `name`.
    pub fn references(&self, name: &str) -> bool {
        self.referenced.contains(name)
    }

    /// Whether a file of the closure (a Composer `autoload.files` entry, say) declares the
    /// global function `name` itself, so the prelude must not inject its own.
    pub fn declares(&self, name: &str) -> bool {
        self.declared.contains(name)
    }

    /// Whether a file of the closure has an `eval` the interpreter may have to run, which can
    /// select any OPcache callable at runtime.
    pub fn has_bridge_eval(&self) -> bool {
        self.bridge_eval
    }

    /// Builds the facts directly, for the preludes' own unit tests.
    #[cfg(test)]
    pub(crate) fn for_test(
        referenced: &[&'static str],
        declared: &[&'static str],
        bridge_eval: bool,
    ) -> Self {
        Self {
            referenced: referenced.iter().copied().collect(),
            declared: declared.iter().copied().collect(),
            bridge_eval,
        }
    }

    /// Collects the facts from a name-resolved closure.
    fn scan(closure: &[Stmt]) -> Self {
        let names: Vec<&'static str> = PROBED_FUNCTIONS.iter().copied().flatten().copied().collect();
        Self {
            referenced: crate::opcache_prelude::detect::resolved_program_references(closure, &names)
                .into_iter()
                .collect(),
            declared: names
                .iter()
                .copied()
                .filter(|name| declares_global_function(closure, false, name))
                .collect(),
            bridge_eval: crate::opcache_prelude::detect::program_has_bridge_eval(closure),
        }
    }
}

/// Returns whether `stmts` declare the GLOBAL function `name`, conditionally or not.
///
/// A conditional declaration counts: `if (!function_exists('opcache_get_status')) { function
/// opcache_get_status() {} }` is the usual shape of a Composer `autoload.files` polyfill, and the
/// prelude's own declaration next to it is a duplicate symbol. A function declared in a
/// namespace is a different function and does not count. `namespaced` says whether a preceding
/// `namespace X;` statement scopes `stmts`.
fn declares_global_function(stmts: &[Stmt], namespaced: bool, name: &str) -> bool {
    let mut namespaced = namespaced;
    stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::NamespaceDecl { name: namespace } => {
            namespaced = namespace.is_some();
            false
        }
        StmtKind::FunctionDecl { name: declared, .. } => {
            !namespaced && declared.trim_start_matches('\\').eq_ignore_ascii_case(name)
        }
        StmtKind::NamespaceBlock { name: namespace, body } => {
            declares_global_function(body, namespace.is_some(), name)
        }
        StmtKind::IncludeOnceGuard { body, .. } | StmtKind::Synthetic(body) => {
            declares_global_function(body, namespaced, name)
        }
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            declares_global_function(then_body, namespaced, name)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| declares_global_function(body, namespaced, name))
                || else_body
                    .as_deref()
                    .is_some_and(|body| declares_global_function(body, namespaced, name))
        }
        _ => false,
    })
}

/// Returns the prelude groups and per-function facts of the autoload closure.
///
/// `program` is the include-resolved entry program, before any prelude is injected. Returns an
/// empty result when nothing is autoloadable or the probe cannot finish.
pub fn autoloaded_preludes(
    program: &Program,
    base_dir: &Path,
    registry: &Registry,
    defines: &HashSet<String>,
) -> AutoloadedPreludes {
    if registry.is_empty() {
        return AutoloadedPreludes::default();
    }
    let span_counters = Span::counters_checkpoint();
    let anonymous_classes = crate::parser::anonymous_class_counter();
    let closure = crate::name_resolver::resolve(program.clone())
        .ok()
        .and_then(|resolved| {
            super::run_collecting_included_with_defines(resolved, base_dir, registry, defines).ok()
        });
    let found = closure.map_or_else(AutoloadedPreludes::default, |(closure, _)| {
        AutoloadedPreludes {
            groups: PROBED_GROUPS
                .iter()
                .filter(|(_, detect)| detect(&closure))
                .map(|(group, _)| *group)
                .collect(),
            functions: AutoloadedFunctions::scan(&closure),
        }
    });
    // Usage detectors may parse literal eval fragments, so their identities are discarded too.
    Span::restore_counters(span_counters);
    crate::parser::restore_anonymous_class_counter(anonymous_classes);
    found
}
