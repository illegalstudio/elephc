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

/// The KNOWN-MODULE predicate the `ini_get_all` extension filter uses to tell "known module with
/// no INI directives" (`[]`) from "no such module" (`E_WARNING` + `false`).
///
/// The list is derived from [`CORE_LOADED_EXTENSIONS`] — the same compile-time set that backs
/// `extension_loaded()` / `get_loaded_extensions()` — LOWERCASED here, so the two cannot drift and
/// the comparison is verbatim against lowercase registry keys (reference PHP does NOT case-fold
/// this argument; do not share a comparison helper with `extension_loaded`, which does). `web`
/// adds `'session'`, the extra module a `--web` binary registers.
///
/// Bridge-linked extensions (`PDO`, `hash`, …) are deliberately NOT included: they are a
/// per-compilation link-set decision made in codegen, while this prelude is built before codegen.
pub(crate) fn ini_module_known_declaration(web: bool) -> Stmt {
    let mut names: Vec<String> = crate::codegen::lower_inst::builtins::CORE_LOADED_EXTENSIONS
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    if web {
        names.push("session".to_string());
    }
    build::ini_module_known_decl(&names)
}
