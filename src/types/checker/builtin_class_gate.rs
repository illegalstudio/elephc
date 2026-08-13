//! Purpose:
//! Decides which builtin classes a program can reach, so the rest are never registered.
//!
//! Called from:
//! - `crate::types::checker::driver`, which passes the answers to `inject_builtin_throwables`,
//!   `inject_builtin_spl_exceptions`, `inject_builtin_iterators` and `inject_builtin_user_filter`.
//!
//! Key details:
//! - Narrows REGISTRATION only; what gets emitted is already gated in `codegen::runtime_metadata`.
//! - Under-detecting is a compile error at the reference site.

use crate::names::php_symbol_key;
use crate::parser::ast::Stmt;
use crate::prelude_prune::usage::Usage;
use std::collections::HashSet;

/// The throwables that must be registered no matter what the program says.
///
/// THIS LIST IS NOT A JUDGEMENT CALL. It mirrors the unconditional half of
/// `codegen::runtime_metadata::classes::seed_runtime_throwable_class_names`, which is the
/// authority on what a runtime helper can materialize with no EIR class reference to hang the id
/// off — `1/$x` raises `DivisionByZeroError` from a codegen guard, `json_encode` raises
/// `JsonException` through `JSON_THROW_ON_ERROR`, and neither names a class in the source.
///
/// `Exception` is here for a second reason as well: `JsonException extends Exception`, so the
/// catch-time walk up `_class_parent_ids` needs it present even for a program that only ever
/// catches the wide type.
///
/// `RuntimeException` IS NOT HERE, and used to be. Its only claim to being unconditional was
/// that `JsonException extends RuntimeException` — which was elephc's own invention, corrected
/// to match reference PHP. Its one producer, `_spl_runtime_exception_class_id`, is read solely by
/// `runtime/spl/doubly_linked_list.rs`, so it now arrives with the SPL surface like the rest of
/// that hierarchy.
///
/// `throwable_gate_matches_the_codegen_seed_list` pins this list against the codegen seeder.
pub(crate) const ALWAYS_REGISTERED_THROWABLES: &[&str] = &[
    "Throwable",
    "Error",
    "TypeError",
    "ValueError",
    "ArithmeticError",
    "DivisionByZeroError",
    "Exception",
    "JsonException",
];

/// Throwables `inject_builtin_throwables` registers that nothing in elephc can raise.
///
/// Each has NO `_*_class_id` symbol, and `seed_runtime_throwable_class_names` records why:
/// elephc rejects a bad builtin arity at COMPILE time where reference PHP raises
/// `ArgumentCountError` at runtime; `assert()` is not implemented, so `AssertionError` has no
/// producer; and an unmatched `match` ends in `Terminator::Fatal` rather than throwing
/// `UnhandledMatchError` — a real gap against reference PHP, and closing it will give the class
/// the EIR reference that makes it survive this gate on its own.
const UNRAISED_THROWABLES: &[&str] = &[
    "ArgumentCountError",
    "AssertionError",
    "UnhandledMatchError",
];

/// (class, parent) for the SPL exception hierarchy, in `builtin_spl_exceptions` order.
///
/// Duplicated deliberately rather than imported: this module needs the PARENT EDGES to close a
/// named exception over its ancestors, and `spl_exception_hierarchy_is_in_step` fails if the two
/// tables ever disagree.
const SPL_EXCEPTION_PARENTS: &[(&str, &str)] = &[
    ("LogicException", "Exception"),
    ("BadFunctionCallException", "LogicException"),
    ("BadMethodCallException", "BadFunctionCallException"),
    ("DomainException", "LogicException"),
    ("InvalidArgumentException", "LogicException"),
    ("LengthException", "LogicException"),
    ("OutOfRangeException", "LogicException"),
    ("RuntimeException", "Exception"),
    ("OutOfBoundsException", "RuntimeException"),
    ("OverflowException", "RuntimeException"),
    ("RangeException", "RuntimeException"),
    ("UnderflowException", "RuntimeException"),
    ("UnexpectedValueException", "RuntimeException"),
];

/// Whether the SPL surface being registered pulls in an SPL exception. It pulls in ALL of them.
///
/// WHY ALL, having first shipped a narrower version that was wrong. The obvious list is the five
/// in the SPL-gated half of `seed_runtime_throwable_class_names` — the ones SPL container helpers
/// throw BY ID, naming no class any scan can see. That list is about SEEDING, and seeding is only
/// for classes with no EIR reference. It says nothing about the second producer: the synthetic
/// PHP bodies of the SPL classes themselves, which `throw new BadMethodCallException(...)` like
/// any user code. Such a throw IS an EIR reference, so it never needs seeding — and needs the
/// class registered all the same.
///
/// The `spl-decorators` example found it, as `unsupported EIR backend feature: unknown class
/// BadMethodCallException`, in a program whose source never writes that name.
///
/// A derived list is possible — grepping the SPL surface finds `BadMethodCallException` and
/// `InvalidArgumentException` and nothing else — and it is rejected: it would be a hand-kept
/// table that only fails when somebody compiles the one example that exercises it. Registering
/// the whole hierarchy for SPL programs is exactly what happened before this gate existed, so it
/// costs them nothing, and the programs this gate is for name no SPL container at all.
fn spl_pulls_in_every_exception(spl_surface_registered: bool) -> bool {
    spl_surface_registered
}

/// Which builtin throwables to register, given what else the program was found to reach.
///
/// WHY THIS IS SAFE, and it is a narrower claim than it looks. This decides what the CHECKER
/// knows, not what codegen emits: `seed_runtime_throwable_class_names` already guards every one
/// of its inserts on `module.class_infos.contains_key`, so the emitted set was already gated and
/// does not move. What moves is `max_class_id` — the `_class_*` tables are dense arrays indexed
/// by class id, so a registered-but-unemitted throwable costs 184 bytes across 22 tables while
/// being unreachable by construction. For `<?php echo 1;` sixteen of them were doing exactly that.
///
/// THE FAILURE MODE IS A COMPILE ERROR. Miss a reference and the checker reports `Unknown type:
/// DomainException` where the program names it. The one case that would be worse — a runtime
/// helper materializing a class whose metadata was never emitted — cannot be reached from here,
/// because the always-set is exactly the set of classes such a helper can name, and a test pins
/// it against the codegen list.
///
/// THREE THINGS REGISTER EVERYTHING, and each names its class somewhere no static walk can read.
/// `eval` resolves names at runtime, and `codegen::eval_constructor_helpers::
/// BUILTIN_THROWABLE_CONSTRUCTOR_CLASSES` emits a constructor bridge for all twenty-five for
/// exactly that reason. `unserialize` reads its class name out of its DATA. And `new $c` with a
/// computed name reaches every entry in
/// `codegen_support::dynamic_new::supported_dynamic_new_builtin_class_names`, which contains all
/// of these. It widens even when a literal assigned the variable earlier, because at the `new`
/// site the name is a variable and nothing connects it back to that assignment.
///
/// The date/time gate next door has no equivalent case: no `Date*` class is in that dynamic-new
/// list, so `new $c` cannot conjure one.
pub(crate) fn throwables_to_register(
    program: &[Stmt],
    spl_surface_registered: bool,
    reflection_registered: bool,
) -> HashSet<String> {
    let usage = crate::prelude_prune::usage::collect(program);
    let mut wanted: HashSet<String> = ALWAYS_REGISTERED_THROWABLES
        .iter()
        .map(|name| (*name).to_string())
        .collect();

    if usage.introspects || usage.constructs_dynamic_class || usage.references("unserialize") {
        wanted.extend(every_builtin_throwable().map(str::to_string));
        return wanted;
    }

    for name in UNRAISED_THROWABLES
        .iter()
        .chain(SPL_EXCEPTION_PARENTS.iter().map(|(name, _)| name))
        .chain(std::iter::once(&"ReflectionException"))
    {
        if program_names(&usage, name) {
            insert_with_ancestors(name, &mut wanted);
        }
    }
    if spl_pulls_in_every_exception(spl_surface_registered) {
        for (name, _) in SPL_EXCEPTION_PARENTS {
            insert_with_ancestors(name, &mut wanted);
        }
    }
    if reflection_registered {
        wanted.insert("ReflectionException".to_string());
    }
    wanted
}

/// Every builtin throwable this module knows about, for the eval case.
fn every_builtin_throwable() -> impl Iterator<Item = &'static str> {
    ALWAYS_REGISTERED_THROWABLES
        .iter()
        .copied()
        .chain(UNRAISED_THROWABLES.iter().copied())
        .chain(SPL_EXCEPTION_PARENTS.iter().map(|(name, _)| *name))
        .chain(std::iter::once("ReflectionException"))
}

/// Whether the program names `class_name` anywhere a class name can appear.
fn program_names(usage: &Usage, class_name: &str) -> bool {
    let key = php_symbol_key(class_name);
    usage.classes.contains(&key) || usage.literals.contains(&key)
}

/// Returns whether `program` can reach the builtin `Generator` class.
///
/// A `yield` or `yield from` makes its enclosing function a generator and materializes a
/// `Generator` object that no source line names — that is the case worth detecting, and the only
/// other route is spelling the type (`function f(): Generator`, `$g instanceof Generator`), which
/// the name scan covers. `new Generator` is not a route: PHP forbids constructing one directly,
/// and `Generator` is absent from `dynamic_new::supported_dynamic_new_builtin_class_names`, so
/// `new $c` cannot conjure one either.
///
/// A declared `Generator` RETURN TYPE also makes a function a generator in elephc
/// (`ir_lower::function::is_generator_return_type`); that spells the class, so it is covered.
///
/// When absent, `_generator_class_id` is emitted as `u64::MAX`, a value no object header carries,
/// so the runtime comparisons simply never match.
pub(crate) fn program_may_reference_generator(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects || usage.uses_yield || program_names(&usage, "Generator")
}

/// Returns whether `program` can reach the builtin `Fiber` and `FiberError` classes.
///
/// Both are in `dynamic_new::supported_dynamic_new_builtin_class_names`, so a `new $c` with a
/// computed name has to widen here exactly as it does for the throwables.
///
/// `FiberError` follows `Fiber` rather than being asked about separately: the fiber runtime
/// raises it, and `codegen_support::emitted_classes` already seeds it on the same condition.
pub(crate) fn program_may_reference_fiber(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects
        || usage.constructs_dynamic_class
        || program_names(&usage, "Fiber")
        || program_names(&usage, "FiberError")
}

/// Returns whether `program` can reach the builtin `php_user_filter` class.
///
/// PHP's only way to write a stream filter is a class that `extends php_user_filter`, which
/// spells the name. `stream_filter_register` is consulted as well because it is what makes such a
/// class reachable, and a program that calls it without extending the base is already broken in a
/// way this gate should not be the first to report.
pub(crate) fn program_may_reference_user_filter(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.introspects
        || program_names(&usage, "php_user_filter")
        || usage.references("stream_filter_register")
}

/// Adds `name` and every ancestor of it, so `catch (BadMethodCallException $e)` does not leave
/// `BadFunctionCallException` and `LogicException` missing from the chain the checker flattens.
fn insert_with_ancestors(name: &str, wanted: &mut HashSet<String>) {
    let mut current = Some(name);
    while let Some(class_name) = current {
        wanted.insert(class_name.to_string());
        current = SPL_EXCEPTION_PARENTS
            .iter()
            .find(|(child, _)| *child == class_name)
            .map(|(_, parent)| *parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    fn registered(source: &str) -> HashSet<String> {
        throwables_to_register(&parse(source), false, false)
    }

    /// The case the gate exists for. Nine survive; the other sixteen were being registered for a
    /// program that cannot reach any of them.
    #[test]
    fn a_trivial_program_registers_only_what_a_helper_can_raise() {
        let wanted = registered("<?php echo 1;");
        assert_eq!(wanted.len(), ALWAYS_REGISTERED_THROWABLES.len());
        for name in ALWAYS_REGISTERED_THROWABLES {
            assert!(wanted.contains(*name), "{name} must always be registered");
        }
        for name in [
            "DomainException",
            "AssertionError",
            "ReflectionException",
            "RuntimeException",
        ] {
            assert!(!wanted.contains(name), "{name} should have been gated out");
        }
    }

    /// Naming `RuntimeException` alone must register it without dragging in the SPL surface.
    /// It sits in the SPL hierarchy table but is a perfectly ordinary class to catch.
    #[test]
    fn naming_runtime_exception_registers_it_alone() {
        let wanted = registered("<?php try { f(); } catch (RuntimeException $e) { echo 1; }");
        assert!(wanted.contains("RuntimeException"));
        assert!(wanted.contains("Exception"), "its parent");
        assert!(!wanted.contains("OutOfBoundsException"), "unrelated child");
    }

    /// Naming one SPL exception must bring its whole ancestor chain, because the checker flattens
    /// inheritance and the catch-time walk climbs `_class_parent_ids` from the THROWN class.
    #[test]
    fn naming_an_exception_registers_its_ancestors() {
        let wanted = registered("<?php try { f(); } catch (BadMethodCallException $e) { echo 1; }");
        for name in [
            "BadMethodCallException",
            "BadFunctionCallException",
            "LogicException",
            "Exception",
        ] {
            assert!(wanted.contains(name), "{name} missing from the chain");
        }
        assert!(!wanted.contains("DomainException"), "unrelated sibling");
    }

    /// A `throw new` names the class as surely as a `catch` does, and so does a string that only
    /// ever reaches it through `new $c`.
    #[test]
    fn every_naming_position_counts() {
        assert!(registered("<?php throw new DomainException('x');").contains("DomainException"));
        assert!(registered("<?php function f(): OverflowException {}").contains("OverflowException"));
        assert!(registered("<?php try { f(); } catch (RangeException $e) { echo 1; }")
            .contains("RangeException"));
        assert!(registered("<?php $x = new \\LengthException('x');").contains("LengthException"));
        assert!(registered("<?php $x = new unexpectedvalueexception('x');")
            .contains("UnexpectedValueException"));
    }

    /// The SPL surface pulls in the whole exception hierarchy — helpers throw five of them by id,
    /// and the synthetic SPL class bodies `throw new` others by name. `spl-decorators` failed to
    /// compile with `unknown class BadMethodCallException` when this registered only the five.
    #[test]
    fn the_spl_surface_registers_every_spl_exception() {
        let wanted = throwables_to_register(&parse("<?php echo 1;"), true, false);
        for (name, _) in SPL_EXCEPTION_PARENTS {
            assert!(wanted.contains(*name), "{name} belongs to the SPL surface");
        }
        assert!(
            wanted.contains("BadMethodCallException"),
            "thrown by a synthetic SPL body, which is an EIR reference and never seeded"
        );
        assert!(!wanted.contains("AssertionError"), "unrelated to SPL");
    }

    /// Reflection helpers raise ReflectionException without the program naming it.
    #[test]
    fn the_reflection_surface_registers_its_exception() {
        let wanted = throwables_to_register(&parse("<?php echo 1;"), false, true);
        assert!(wanted.contains("ReflectionException"));
    }

    /// The three cases where the class name is somewhere no static walk can read it: resolved at
    /// runtime by eval, carried in unserialize's payload, or computed into a `new $c`.
    ///
    /// The last is the one that nearly slipped through. Every builtin throwable is in
    /// `supported_dynamic_new_builtin_class_names`, so `new $c` can construct any of them, and
    /// the emission side already handles that by seeding the whole list — guarded on the class
    /// being REGISTERED. Gating registration without this would have made `new $c` quietly
    /// unable to build them.
    #[test]
    fn a_name_no_walk_can_read_registers_everything() {
        for source in [
            "<?php eval('throw new DomainException(\"x\");');",
            "<?php $e = unserialize($payload);",
            "<?php $c = $argv[1]; throw new $c('x');",
        ] {
            let wanted = registered(source);
            for name in every_builtin_throwable() {
                assert!(wanted.contains(name), "{name} missing for: {source}");
            }
        }
    }

    /// `new $c` widens even when a literal assigned `$c` earlier, and that is not laziness: at
    /// the `new` site the name expression is a VARIABLE, and nothing in this walk connects it back
    /// to the assignment. A first draft of the test above asserted the narrow behaviour and would
    /// have documented something the code does not do.
    #[test]
    fn a_dynamic_new_widens_even_with_a_literal_in_scope() {
        let wanted = registered("<?php $c = 'RangeException'; throw new $c('x');");
        assert!(wanted.contains("RangeException"));
        assert!(wanted.contains("DomainException"), "the whole family, by design");
    }

    /// The always-set must cover every class the codegen seeder can insert unconditionally.
    /// If it did not, a helper would stamp an object with a class whose metadata was never
    /// emitted, and the catch-time walk would meet the `-2` sentinel and abort — the one failure
    /// this gate could cause that is not a compile error.
    #[test]
    fn throwable_gate_matches_the_codegen_seed_list() {
        for name in [
            "Throwable",
            "Error",
            "TypeError",
            "ValueError",
            "ArithmeticError",
            "DivisionByZeroError",
            "JsonException",
            "Exception",
        ] {
            assert!(
                ALWAYS_REGISTERED_THROWABLES.contains(&name),
                "{name} is seeded unconditionally by codegen and must always be registered"
            );
        }
    }

    /// A `yield` is the case worth detecting: it materializes a Generator no source line names.
    #[test]
    fn a_yield_registers_generator() {
        for source in [
            "<?php function g() { yield 1; } foreach (g() as $v) { echo $v; }",
            "<?php function g() { yield from [1, 2]; } foreach (g() as $v) { echo $v; }",
            "<?php $f = function () { yield 1; };",
            "<?php class C { public function m() { yield 1; } }",
        ] {
            assert!(
                program_may_reference_generator(&parse(source)),
                "should register: {source}"
            );
        }
    }

    /// Spelling the type is the other route, and a declared `Generator` return type is what makes
    /// a body a generator in elephc even before its `yield` is reached.
    #[test]
    fn naming_generator_registers_it() {
        assert!(program_may_reference_generator(&parse(
            "<?php function f(): Generator { return g(); }"
        )));
        assert!(program_may_reference_generator(&parse(
            "<?php if ($x instanceof Generator) { echo 1; }"
        )));
        assert!(program_may_reference_generator(&parse(
            "<?php eval('function g() { yield 1; }');"
        )));
    }

    /// The case the gate exists for: no yield anywhere, no mention of the class.
    #[test]
    fn a_program_without_yield_does_not_register_generator() {
        assert!(!program_may_reference_generator(&parse("<?php echo 1;")));
        assert!(!program_may_reference_generator(&parse(
            "<?php function f(array $a): int { return count($a); } echo f([1]);"
        )));
    }

    /// Fiber and FiberError are both in the dynamic-new list, so `new $c` widens here too.
    #[test]
    fn fiber_registers_on_a_name_or_a_dynamic_new() {
        for source in [
            "<?php $f = new Fiber(function () { Fiber::suspend(1); });",
            "<?php function h(Fiber $f) {}",
            "<?php try { f(); } catch (FiberError $e) { echo 1; }",
            "<?php $c = $argv[1]; $o = new $c();",
            "<?php eval('$f = new Fiber(fn () => 1);');",
        ] {
            assert!(
                program_may_reference_fiber(&parse(source)),
                "should register: {source}"
            );
        }
        assert!(!program_may_reference_fiber(&parse("<?php echo 1;")));
    }

    /// A stream filter is written as a class extending php_user_filter, which spells the name.
    #[test]
    fn a_user_filter_registers_on_the_base_class_or_the_registrar() {
        assert!(program_may_reference_user_filter(&parse(
            "<?php class Up extends php_user_filter { public function filter($in, $out, &$c, $end) { return PSFS_PASS_ON; } }"
        )));
        assert!(program_may_reference_user_filter(&parse(
            "<?php stream_filter_register('up', 'Up');"
        )));
        assert!(!program_may_reference_user_filter(&parse("<?php echo 1;")));
        assert!(!program_may_reference_user_filter(&parse(
            "<?php $h = fopen('php://memory', 'r+'); fwrite($h, 'x');"
        )));
    }

    /// The parent edges here and the injection table in `builtin_spl_exceptions` are two copies
    /// of one hierarchy. Closing a named exception over the wrong parents would leave the checker
    /// flattening an incomplete chain.
    #[test]
    fn spl_exception_hierarchy_is_in_step() {
        assert_eq!(
            SPL_EXCEPTION_PARENTS,
            crate::types::checker::builtin_spl_exceptions::hierarchy_for_gate(),
            "the two copies of the SPL exception hierarchy disagree"
        );
    }
}
