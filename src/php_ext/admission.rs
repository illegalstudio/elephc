//! Purpose:
//! Decides whether a PHP extension can be hosted at all, from the symbols its
//! compiled objects require. Two of the failure modes here are invisible at
//! runtime, so the decision has to be made when the extension is declared.
//!
//! Called from:
//! - `crate::php_ext` when an extension is added, before any build work.
//!
//! Key details:
//! - Engine-hook extensions (`zend_execute_ex`, `zend_compile_file`, …) replace
//!   part of the VM. An AOT binary has no execute loop and no opcode array, so
//!   such an extension **links cleanly, installs its hook, and is never called**.
//!   That is precisely the class static linking cannot catch, hence the refusal.
//! - Symbol spelling differs by object format: Mach-O prefixes an underscore, ELF
//!   does not. Since several genuine PHP symbols *begin* with an underscore
//!   (`_emalloc`, `_zend_bailout`), stripping one unconditionally is correct on
//!   macOS and wrong on Linux. [`CSymbol`] therefore matches a name with an
//!   optional mangling prefix instead of rewriting the input.

use std::collections::BTreeSet;

/// Symbols that mean "this extension replaces part of the engine".
const ENGINE_HOOKS: &[&str] = &[
    "zend_execute_ex",
    "zend_execute_internal",
    "zend_compile_file",
    "zend_compile_string",
    "zend_ast_process",
];

/// Surface belonging to *other* extensions: hosting this one drags them in.
const DEPENDENCY_MARKERS: &[(&str, &[&str])] = &[
    ("session", &["php_session_", "ps_globals"]),
    ("SPL", &["spl_ce_", "spl_iterator"]),
    ("pcre", &["php_pcre_", "pcre_get_compiled"]),
    ("json", &["php_json_"]),
];

/// A required symbol as the linker sees it, independent of object format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CSymbol(pub String);

impl CSymbol {
    /// True when this symbol denotes the C name `name`, tolerating one leading
    /// underscore added by Mach-O mangling. Deliberately not a strip: `_emalloc`
    /// and `emalloc` are different C names, and only the mangling prefix is
    /// optional.
    pub fn is(&self, name: &str) -> bool {
        self.0 == name || self.0.strip_prefix('_').is_some_and(|s| s == name)
    }

    /// True when the symbol contains `fragment`, used for family markers such as
    /// `spl_ce_` where the full name varies.
    pub fn contains(&self, fragment: &str) -> bool {
        self.0.contains(fragment)
    }

    /// C++ mangling (Itanium ABI) or runtime support, meaning the extension has
    /// C++ frames that a `longjmp` would skip without running destructors.
    pub fn is_cxx_marker(&self) -> bool {
        self.0.starts_with("_Z")
            || self.0.starts_with("__Z")
            || self.0.contains("__cxa_")
            || self.0.contains("_ZTV")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Structurally unhostable: it would link and then silently do nothing.
    Refuse { engine_hooks: Vec<String> },
    /// Hostable, but with a hazard needing an explicit decision.
    Review { reason: String },
    /// Hostable. `missing` is the porting cost in symbols the shim lacks.
    Admit {
        missing: usize,
        dependencies: Vec<String>,
    },
}

/// Judge an extension from the symbols it needs and the symbols the shim already
/// provides. Both are given as the linker spells them; no rewriting occurs.
pub fn judge(required: &[CSymbol], provided_by_shim: &BTreeSet<String>) -> Verdict {
    let hooks: Vec<String> = ENGINE_HOOKS
        .iter()
        .filter(|hook| required.iter().any(|sym| sym.is(hook)))
        .map(|h| (*h).to_string())
        .collect();
    if !hooks.is_empty() {
        return Verdict::Refuse {
            engine_hooks: hooks,
        };
    }

    let is_cxx = required.iter().any(CSymbol::is_cxx_marker);
    let can_bailout = required.iter().any(|s| s.contains("zend_bailout"));
    if is_cxx && can_bailout {
        return Verdict::Review {
            reason: "C++ extension that can zend_bailout: destructors are skipped by the longjmp"
                .to_string(),
        };
    }

    let missing = required
        .iter()
        .filter(|sym| {
            !provided_by_shim.contains(&sym.0)
                && !sym
                    .0
                    .strip_prefix('_')
                    .is_some_and(|s| provided_by_shim.contains(s))
        })
        .count();

    let dependencies = DEPENDENCY_MARKERS
        .iter()
        .filter(|(_, markers)| {
            markers
                .iter()
                .any(|m| required.iter().any(|sym| sym.contains(m)))
        })
        .map(|(name, _)| (*name).to_string())
        .collect();

    Verdict::Admit {
        missing,
        dependencies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syms(names: &[&str]) -> Vec<CSymbol> {
        names.iter().map(|n| CSymbol((*n).to_string())).collect()
    }

    fn empty_shim() -> BTreeSet<String> {
        BTreeSet::new()
    }

    /// pcov and xdebug hook the executor. On an AOT target there is no execute
    /// loop to hook, so they would link and silently never run.
    #[test]
    fn refuses_engine_hook_extensions() {
        let pcov = syms(&["_zend_compile_file", "_zend_execute_ex", "_emalloc"]);
        match judge(&pcov, &empty_shim()) {
            Verdict::Refuse { engine_hooks } => {
                assert!(engine_hooks.contains(&"zend_execute_ex".to_string()));
                assert!(engine_hooks.contains(&"zend_compile_file".to_string()));
            }
            other => panic!("engine hooks must be refused, got {other:?}"),
        }
    }

    /// Refusal must not depend on object format.
    #[test]
    fn refuses_engine_hooks_in_elf_spelling_too() {
        let elf = syms(&["zend_execute_ex", "emalloc"]);
        assert!(matches!(judge(&elf, &empty_shim()), Verdict::Refuse { .. }));
    }

    /// simdjson: C++ but with no path to a fatal, so no destructor hazard.
    #[test]
    fn admits_cxx_extension_that_cannot_bail_out() {
        let simdjson = syms(&["__ZTVSt12length_error", "_emalloc", "_zend_hash_update"]);
        assert!(matches!(
            judge(&simdjson, &empty_shim()),
            Verdict::Admit { .. }
        ));
    }

    /// apcu: can bail out, but is C — also no destructor hazard.
    #[test]
    fn admits_c_extension_that_can_bail_out() {
        let apcu = syms(&["__zend_bailout", "_emalloc", "_php_var_serialize"]);
        assert!(matches!(judge(&apcu, &empty_shim()), Verdict::Admit { .. }));
    }

    /// Only the intersection is hazardous. This combination is not present in any
    /// real extension measured so far, so the rule is exercised here rather than
    /// left untested.
    #[test]
    fn flags_cxx_extension_that_can_bail_out() {
        let hazard = syms(&["__ZTVSt12length_error", "__zend_bailout", "_emalloc"]);
        match judge(&hazard, &empty_shim()) {
            Verdict::Review { reason } => assert!(reason.contains("destructors")),
            other => panic!("C++ and bailout must be flagged, got {other:?}"),
        }
    }

    #[test]
    fn counts_only_symbols_the_shim_lacks() {
        let required = syms(&["_emalloc", "_efree", "_zend_hash_update"]);
        let mut shim = BTreeSet::new();
        shim.insert("_emalloc".to_string());
        shim.insert("_efree".to_string());
        match judge(&required, &shim) {
            Verdict::Admit { missing, .. } => assert_eq!(missing, 1),
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    /// The shim's own symbol list is recorded without the Mach-O prefix, while a
    /// required symbol carries it; they must still match.
    #[test]
    fn matches_shim_symbols_across_mangling_prefix() {
        let required = syms(&["_emalloc"]);
        let mut shim = BTreeSet::new();
        shim.insert("emalloc".to_string());
        match judge(&required, &shim) {
            Verdict::Admit { missing, .. } => assert_eq!(missing, 0),
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    #[test]
    fn reports_other_extensions_it_would_drag_in() {
        let msgpack = syms(&["_php_session_register_serializer", "_ps_globals", "_emalloc"]);
        match judge(&msgpack, &empty_shim()) {
            Verdict::Admit { dependencies, .. } => {
                assert!(dependencies.contains(&"session".to_string()));
            }
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    /// `_emalloc` is a real C name beginning with an underscore. Treating the
    /// leading underscore as always-strippable would make it match `emalloc`,
    /// which is a different symbol.
    #[test]
    fn does_not_confuse_underscore_prefixed_c_names() {
        let sym = CSymbol("_emalloc".to_string());
        assert!(sym.is("emalloc"), "Mach-O spelling of emalloc must match");
        assert!(sym.is("_emalloc"), "ELF spelling of _emalloc must match");
        let plain = CSymbol("emalloc".to_string());
        assert!(!plain.is("_emalloc"), "emalloc is not _emalloc");
    }
}
