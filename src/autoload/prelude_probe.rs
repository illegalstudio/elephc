//! Purpose:
//! Decides which detected standard-library preludes the AUTOLOAD CLOSURE of a program needs,
//! before those preludes are injected.
//!
//! Called from:
//! - `crate::pipeline::compile()`, right before the prelude injections.
//!
//! Key details:
//! - Each prelude (PDO, mysqli, tz, image, hash, curl, xml, var_export) is injected only when
//!   the program uses its surface, and that decision has to be taken before name resolution so a
//!   namespaced caller resolves to the injected declarations. The Composer/SPL autoload pass runs
//!   AFTER name resolution, so a class file it loads that uses `PDO` or `CurlHandle` was never
//!   scanned: the prelude was skipped and the program failed with `Unknown type: PDO`.
//! - The probe runs name resolution and the autoload pass on a COPY of the program, scans the
//!   result with each prelude's own detector, and hands back the groups found. The real pass then
//!   injects them exactly as if the entry file had used them, so the outcome is the one
//!   `--with-<crate>` gives, without forcing the whole surface past reachability pruning.
//! - The copy's work is discarded, including every counter it advanced (source identities,
//!   synthetic spans, anonymous-class numbers, the resolver's hidden temporaries), so the real
//!   pass numbers its files, classes and temporaries exactly as it would without the probe.
//! - A probe failure is not reported here: the real pass hits the same error and reports it.

use std::collections::HashSet;
use std::path::Path;

use crate::parser::ast::{Program, Stmt};
use crate::span::Span;

use super::Registry;

/// Every prelude group the probe can report, with the detector that decides it.
const PROBED_GROUPS: &[(&str, fn(&[Stmt]) -> bool)] = &[
    ("pdo", crate::pdo_prelude::program_uses_pdo),
    ("mysqli", crate::mysqli_prelude::program_uses_mysqli),
    ("tz", crate::tz_prelude::program_uses_surface),
    ("image", crate::image_prelude::program_uses_surface),
    ("hash", crate::hash_prelude::program_uses_surface),
    ("curl", crate::curl_prelude::program_uses_surface),
    ("xml", crate::xml_prelude::program_uses_surface),
    ("var_export", crate::var_export_prelude::program_uses_surface),
];

/// Returns the prelude groups some file of the autoload closure uses.
///
/// `program` is the include-resolved entry program, before any prelude is injected. Returns an
/// empty set when nothing is autoloadable or the probe cannot finish.
pub fn autoloaded_prelude_groups(
    program: &Program,
    base_dir: &Path,
    registry: &Registry,
    defines: &HashSet<String>,
) -> HashSet<&'static str> {
    if registry.is_empty() || crate::resolver::dynamic_include_report_mode() {
        return HashSet::new();
    }
    let span_counters = Span::counters_checkpoint();
    let resolver_counters = crate::resolver::counters_checkpoint();
    let anonymous_classes = crate::parser::anonymous_class_counter();
    let closure = crate::name_resolver::resolve(program.clone())
        .ok()
        .and_then(|resolved| {
            super::run_collecting_included_with_defines(resolved, base_dir, registry, defines).ok()
        });
    Span::restore_counters(span_counters);
    crate::resolver::restore_counters(resolver_counters);
    crate::parser::restore_anonymous_class_counter(anonymous_classes);
    let Some((closure, _)) = closure else {
        return HashSet::new();
    };
    PROBED_GROUPS
        .iter()
        .filter(|(_, detect)| detect(&closure))
        .map(|(group, _)| *group)
        .collect()
}
