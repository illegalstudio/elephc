//! Purpose:
//! Owns the public builtin class-name registry used while injecting checker metadata.
//! Performs redeclaration checks before synthetic classes are added.
//!
//! Called from:
//! - `super::inject_builtin_spl_classes()`.
//!
//! Key details:
//! - The lists mirror public builtin classes, not private compiler helper classes.
//! - Name comparison uses PHP's case-insensitive symbol keying.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

use super::super::builtin_types::InterfaceDeclInfo;

pub(super) const SPL_CLASS_NAMES: &[&str] = &[
    "SplDoublyLinkedList",
    "SplStack",
    "SplQueue",
    "SplFixedArray",
    "EmptyIterator",
    "InternalIterator",
    "ArrayIterator",
    "RecursiveArrayIterator",
    "ArrayObject",
    "IteratorIterator",
    "LimitIterator",
    "NoRewindIterator",
    "InfiniteIterator",
    "FilterIterator",
    "CallbackFilterIterator",
    "CachingIterator",
    "RecursiveFilterIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveIteratorIterator",
    "ParentIterator",
    "RegexIterator",
    "RecursiveRegexIterator",
    "SplFileInfo",
    "SplFileObject",
    "SplTempFileObject",
    "DirectoryIterator",
    "FilesystemIterator",
    "GlobIterator",
    "RecursiveDirectoryIterator",
    "RecursiveCachingIterator",
    "AppendIterator",
    "MultipleIterator",
    "SplHeap",
    "SplMaxHeap",
    "SplMinHeap",
    "SplPriorityQueue",
    "SplObjectStorage",
];

const PHAR_CLASS_NAMES: &[&str] = &["Phar", "PharData", "PharFileInfo"];

/// The ZIP classes the registration inserts.
///
/// Its own list rather than an entry in `SPL_CLASS_NAMES`, because it is not SPL — but it has to
/// be in the SCAN, or the registration that inserts `ZipArchive` never runs and the class exists
/// without being nameable.
pub(super) const ZIP_CLASS_NAMES: &[&str] = &["ZipArchive"];

/// Returns whether `program` can reach any of the builtin SPL or Phar classes.
///
/// WHY THIS GATE EXISTS, measured rather than assumed. Registering these 40 classes costs
/// **27 ms of the type-check phase** — 54% of it for `<?php echo 1;`, and about 12% of that
/// program's whole compile. It is not the registration that costs (7 ms builds all 99 builtin
/// classes); it is the checker afterwards walking every one of them to patch signatures,
/// validate contracts and flatten inheritance. A program that names none of them pays for all
/// of them.
///
/// THE FAILURE MODE IS LOUD, which is what makes this gate acceptable. Under-detect, and the
/// checker reports `Unknown type: ArrayObject` at the reference site — a compile error the user
/// can read, not a miscompile. That is the opposite of the prelude-pruning hazard, where a
/// dropped `function_exists` subject silently flips a guard.
///
/// The reference can be a TYPE (`new ArrayObject`, `extends SplStack`, a hint, `instanceof`,
/// `Spl…::`) or a STRING (`class_exists('ArrayObject')`, `new $c` after `$c = 'SplStack'`), so
/// both are consulted. Enumerating the symbol table — `get_declared_classes()`, `eval` — names
/// nothing at all, and registers everything.
///
/// IT SEES THE PROGRAM AFTER PRELUDE INJECTION, which is what makes it safe for a prelude to name
/// one of these classes: the checker runs downstream of every `inject_*` pass, so a surface that
/// hints `ArrayObject` registers it just as user code would.
///
/// IT ALSO SEES UNRESOLVED NAMES. `namespace App; new ArrayObject;` resolves to `App\ArrayObject`
/// later, but this predicate matches on the written name and registers the builtin surface
/// anyway. That over-registers, which costs time and breaks nothing.
///
/// The set is `SPL_CLASS_NAMES` plus `PHAR_CLASS_NAMES` plus `ZIP_CLASS_NAMES`; the first two are
/// not split because `PharFileInfo extends SplFileInfo`, so gating Phar separately would imply the
/// SPL gate anyway. `ZipArchive` is here because the same registration inserts it: a name missing
/// from this scan is a class the compiler HAS and no program can reach, which is what
/// `every_registered_class_is_reachable_by_the_gate` now refuses.
pub(crate) fn program_may_reference_spl(program: &[crate::parser::ast::Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    if usage.introspects {
        return true;
    }
    // `unserialize` names its class inside the DATA, where no static walk can read it, exactly as
    // the date/time gate treats it. Without this a valid serialized `SplFixedArray` came back as
    // `__PHP_Incomplete_Class`, and adding an unrelated `class_exists("SplFixedArray")` to the
    // program "fixed" it — the signature of a gate that closed on a reference it could not see.
    if usage.references("unserialize") {
        return true;
    }
    // `new $c` names its class in a VALUE, so there is no reference site for the walk below to
    // find and no `Unknown type` for the checker to report: the program compiles and dies at
    // runtime with `Class "ArrayObject" not found`, where php constructs the object. Eleven of
    // these classes are in `dynamic_new::supported_dynamic_new_builtin_class_names`, so the
    // dispatch can reach them and the gate has to widen exactly as the throwable one does.
    if usage.constructs_dynamic_class {
        return true;
    }
    SPL_CLASS_NAMES
        .iter()
        .chain(PHAR_CLASS_NAMES.iter())
        .chain(ZIP_CLASS_NAMES.iter())
        .any(|name| {
            let key = php_symbol_key(name);
            usage.classes.contains(&key) || usage.literals.contains(&key)
        })
}

/// Rejects a user declaration that would shadow one of these builtin classes.
///
/// THIS MUST RUN WHETHER OR NOT THE CLASSES ARE REGISTERED. It is a statement about the USER's
/// declarations, not about ours. Gating it alongside the registration — which the first cut of
/// the pay-for-use change did — let `class SplFileInfo {}` through in silence, because declaring
/// a name is not REFERENCING it: the predicate said "no SPL here", nothing was registered, and
/// nothing was there to collide with. `error_tests::spl_builtins` failed thirteen ways and said
/// so.
pub(super) fn ensure_no_redeclarations(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    ensure_no_class_redeclarations(
        interface_map,
        class_map,
        SPL_CLASS_NAMES,
        "Cannot redeclare built-in SPL class",
    )?;
    ensure_no_class_redeclarations(
        interface_map,
        class_map,
        PHAR_CLASS_NAMES,
        "Cannot redeclare built-in class",
    )
}

/// Checks one public builtin-class family for user/interface redeclarations.
fn ensure_no_class_redeclarations(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
    class_names: &[&str],
    message_prefix: &str,
) -> Result<(), CompileError> {
    for class_name in class_names {
        let class_key = php_symbol_key(class_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == class_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == class_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("{}: {}", message_prefix, class_name),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every class the registration inserts can be REACHED by the scan that gates it.
    ///
    /// The two sides are separate lists: one decides what gets registered, the other whether to
    /// register at all. `ZipArchive` was in the first and neither of the others, so the compiler
    /// carried a class no program could name — "Undefined class: ZipArchive" from a build that
    /// defines it. Reading both sides here is what makes the next omission a red test.
    #[test]
    fn every_registered_class_is_reachable_by_the_gate() {
        let mut interfaces = HashMap::new();
        let mut classes = HashMap::new();
        super::super::inject_builtin_spl_classes(&mut interfaces, &mut classes, true)
            .expect("the builtin classes must register");
        let reachable: std::collections::HashSet<String> = SPL_CLASS_NAMES
            .iter()
            .chain(PHAR_CLASS_NAMES.iter())
            .chain(ZIP_CLASS_NAMES.iter())
            .map(|name| php_symbol_key(name))
            .collect();
        let unreachable: Vec<&String> = classes
            .keys()
            // A `__Elephc`-prefixed class is SYNTHETIC: the prefix cannot appear in PHP source, so
            // no program names it and none needs to — it is registered alongside the class that
            // uses it, and reached through that one.
            .filter(|name| !name.starts_with("__Elephc"))
            .filter(|name| !reachable.contains(&php_symbol_key(name)))
            .collect();
        assert!(
            unreachable.is_empty(),
            "these classes are registered but no program can name them, because the gate that \
             decides whether to register does not list them: {unreachable:?}"
        );
    }

    /// Parses user-facing PHP the way the checker driver receives it.
    fn parse(source: &str) -> Vec<crate::parser::ast::Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// A program that names no SPL class pays for none of them. This is the case the gate
    /// exists for: it is the overwhelming majority of programs, and it was costing 27 ms of
    /// type-check each.
    #[test]
    fn a_program_naming_no_spl_class_does_not_register_them() {
        assert!(!program_may_reference_spl(&parse("<?php echo 1;")));
        assert!(!program_may_reference_spl(&parse(
            "<?php function f(array $a): int { return count($a); } echo f([1, 2]);"
        )));
    }

    /// A `new $c` reaches these classes with NOTHING spelled, so the gate widens on the DISPATCH.
    ///
    /// Eleven SPL classes sit in `dynamic_new::supported_dynamic_new_builtin_class_names`, so
    /// `new $c` can construct `ArrayObject`. The gate used to ask only whether some NAME in the
    /// program referenced one of them. A dynamic new spells nothing, so nothing registered, and
    /// the program compiled and then died at run time with `Class "ArrayObject" not found` where
    /// php builds the object — the gate's "the failure mode is LOUD, the checker reports
    /// `Unknown type` at the reference site" reasoning does not hold when there IS no reference
    /// site.
    ///
    /// THE SECOND ASSERTION IS WHAT KEEPS THIS HONEST. The same fragments without a dynamic new
    /// must still register nothing, so a passing first assertion cannot be explained by a literal
    /// the walk happened to see.
    #[test]
    fn a_dynamic_new_registers_the_spl_surface_with_no_name_spelled() {
        assert!(program_may_reference_spl(&parse(
            "<?php $parts = ['Array', 'Object']; $c = $parts[0] . $parts[1]; new $c();"
        )));
        assert!(!program_may_reference_spl(&parse(
            "<?php $parts = ['Array', 'Object']; echo $parts[0] . $parts[1];"
        )));
    }

    /// Every way a program can NAME an SPL class has to count, because each of them makes the
    /// checker need its metadata.
    #[test]
    fn every_form_of_reference_registers() {
        for source in [
            "<?php $s = new SplStack();",
            "<?php class Mine extends ArrayObject {}",
            "<?php function f(SplFileInfo $i) { return $i; }",
            "<?php function f($x): ArrayIterator { return $x; }",
            "<?php $ok = $x instanceof SplObjectStorage;",
            "<?php echo SplFileObject::READ_AHEAD;",
            "<?php try { g(); } catch (SplStack $e) {}",
            "<?php $p = new Phar('x.phar');",
        ] {
            assert!(
                program_may_reference_spl(&parse(source)),
                "must register for: {source}"
            );
        }
    }

    /// A LITERAL name is a reference too. `class_exists('ArrayObject')` has to keep answering
    /// truthfully, and `new $c` after `$c = 'SplQueue'` has to keep working.
    #[test]
    fn a_literal_name_registers() {
        assert!(program_may_reference_spl(&parse(
            "<?php if (class_exists('ArrayObject')) { echo 1; }"
        )));
        assert!(program_may_reference_spl(&parse(
            "<?php $c = 'SplQueue'; $q = new $c();"
        )));
    }

    /// Enumerating the symbol table names nothing, so nothing can be inferred and everything is
    /// registered.
    #[test]
    fn introspection_registers_everything() {
        assert!(program_may_reference_spl(&parse(
            "<?php print_r(get_declared_classes());"
        )));
        assert!(program_may_reference_spl(&parse("<?php eval('$x = 1;');")));
    }

    /// Case-insensitivity, because PHP class names are.
    #[test]
    fn the_match_is_case_insensitive() {
        assert!(program_may_reference_spl(&parse("<?php $s = new splstack();")));
        assert!(program_may_reference_spl(&parse(
            "<?php $s = new ARRAYOBJECT();"
        )));
    }
}
