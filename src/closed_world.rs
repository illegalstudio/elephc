//! Purpose:
//! Owns the `--closed-world` build mode: "link no interpreter".
//!
//! Called from:
//! - `crate::cli` (flag and `ELEPHC_CLOSED_WORLD`), `crate::pipeline::compile()` (sets the mode),
//!   the lowering of `eval()` and runtime `include`/`require`, and the final bridge check.
//!
//! Key details:
//! - The state is thread-local like `crate::strict_php`: the front end and lowering run on the one
//!   compile thread, and codegen reads the copy the lowered module carries (`Module::closed_world`).
//! - In this mode a construct that would need the eval bridge either lowers to a native
//!   equivalent (a runtime include over the compiled-source table) or to a runtime error
//!   (`eval()` of a non-literal string), and anything left over fails the build with the reasons.

use std::cell::Cell;

thread_local! {
    static CLOSED_WORLD: Cell<bool> = const { Cell::new(false) };
}

/// Enables or disables the closed-world mode for the compilation running on this thread.
pub fn set_enabled(enabled: bool) {
    CLOSED_WORLD.with(|cell| cell.set(enabled));
}

/// Whether the compilation running on this thread links no interpreter.
pub fn is_enabled() -> bool {
    CLOSED_WORLD.with(|cell| cell.get())
}

/// Whether `ELEPHC_CLOSED_WORLD` asks for the mode (`1`, `true`, `on`).
pub fn requested_by_environment() -> bool {
    matches!(
        std::env::var("ELEPHC_CLOSED_WORLD").as_deref(),
        Ok("1") | Ok("true") | Ok("on")
    )
}

/// The message a non-literal `eval()` raises in a closed-world program.
pub const EVAL_UNAVAILABLE_MESSAGE: &str = "eval() is not available in a closed-world build";

/// Builds the table a closed-world runtime include consults: one record per catalog source.
///
/// - a source the compiler already carries whole (autoloaded or preloaded declaration files)
///   is [`ClosedIncludeKind::Declarations`]: including it again runs nothing;
/// - a source with a compiled unit function runs that function ([`ClosedIncludeKind::Unit`]);
/// - every other source is part of the world but cannot be replayed from an include
///   ([`ClosedIncludeKind::NotCompiled`]); its `_once` guard still answers `true` after the
///   compiler's own inclusion ran it.
pub fn include_table(module: &crate::ir::Module) -> Vec<crate::ir::ClosedInclude> {
    use crate::ir::{ClosedInclude, ClosedIncludeKind};
    let Some(catalog) = module.source_catalog() else {
        return Vec::new();
    };
    let units: std::collections::HashMap<&std::path::Path, &str> = module
        .include_units
        .iter()
        .filter(|unit| {
            module
                .functions
                .iter()
                .any(|function| function.name.eq_ignore_ascii_case(&unit.function))
        })
        .map(|unit| (unit.canonical_path.as_path(), unit.function.as_str()))
        .collect();
    catalog
        .iter()
        .map(|(_, source)| {
            let kind = if let Some(function) = units.get(source.canonical_path.as_path()) {
                ClosedIncludeKind::Unit((*function).to_string())
            } else if module.preincluded_sources.contains(&source.canonical_path) {
                ClosedIncludeKind::Declarations
            } else {
                ClosedIncludeKind::NotCompiled
            };
            ClosedInclude { canonical_path: source.canonical_path.clone(), kind }
        })
        .collect()
}
