//! Purpose:
//! Decides whether a program can reach the builtin Reflection classes at all.
//!
//! Called from:
//! - `crate::types::checker::driver`, which passes the answer to `inject_builtin_reflection`.
//!
//! Key details:
//! - The predicate is a conservative over-approximation; under-detecting is a compile error.

use crate::names::php_symbol_key;
use crate::parser::ast::Stmt;

/// The classes `inject_builtin_reflection` registers, and the names this gate looks for.
///
/// Kept in the same order as the injection so the two can be read side by side. The list is
/// duplicated in `crate::ir_lower::reflection`, which decides separately which of them get
/// METHOD BODIES emitted; this one only decides whether the CHECKER learns they exist.
pub(crate) const REFLECTION_CLASS_NAMES: &[&str] = &[
    "ReflectionAttribute",
    "ReflectionClass",
    "ReflectionObject",
    "ReflectionEnum",
    "ReflectionFunction",
    "ReflectionMethod",
    "ReflectionProperty",
    "ReflectionParameter",
    "ReflectionNamedType",
    "ReflectionUnionType",
    "ReflectionIntersectionType",
    "ReflectionClassConstant",
    "ReflectionEnumUnitCase",
    "ReflectionEnumBackedCase",
];

/// Returns whether `program` can reach any builtin Reflection class.
///
/// WHY THIS GATE EXISTS, measured rather than assumed. Registering these 14 classes is not what
/// costs — building them is a fraction of a millisecond. What costs is everything the checker
/// then does to them: flattening inheritance, patching signatures onto each method, and
/// validating contracts, all of it for a program that never says `Reflection`. This is the same
/// shape as the SPL gate in `crate::types::checker::builtin_spl_classes`, which took 27 ms out
/// of a 50 ms type-check phase, and it is the second-largest such family.
///
/// THE FAILURE MODE IS LOUD, which is what makes the gate acceptable at all. Under-detect and
/// the checker reports `Unknown type: ReflectionClass` at the reference site — a compile error
/// the user reads, not a miscompile. That is the opposite of the prelude-pruning hazard, where
/// a dropped `function_exists` subject silently flips a guard, and it is why this predicate may
/// be a crude over-approximation without apology.
///
/// THE EMITTED CODE IS ALREADY PAY-FOR-USE and unaffected by this decision.
/// `crate::ir_lower::reflection` runs its own reachability fixpoint over EIR and lowers only the
/// Reflection classes a program actually touches. This gate is upstream of that, about checker
/// metadata alone.
///
/// EVAL KEEPS EVERYTHING, and must. The dynamic bridge resolves class names at runtime, so a
/// program containing `eval` can construct any Reflection class from a string this walk never
/// sees. `usage.introspects` covers it — `eval` parses as an ordinary call, not as its own AST
/// node, so the usage walker's introspection table sees it like any other builtin.
///
/// IT SEES THE PROGRAM AFTER PRELUDE INJECTION, so a prelude that hints a Reflection type
/// registers the family exactly as user code would. None does today.
///
/// AND SOME REFERENCES ARE NOT SPELLED AT ALL — see `REFLECTION_PRODUCING_BUILTINS`.
pub(crate) fn program_may_reference_reflection(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    if usage.introspects {
        return true;
    }
    if REFLECTION_PRODUCING_BUILTINS
        .iter()
        .any(|name| usage.references(name))
    {
        return true;
    }
    REFLECTION_CLASS_NAMES.iter().any(|name| {
        let key = php_symbol_key(name);
        usage.classes.contains(&key) || usage.literals.contains(&key)
    })
}

/// Builtins that MANUFACTURE a Reflection object the program never names.
///
/// `class_get_attributes('C')` is typed `Array(Object("ReflectionAttribute"))`, so a program can
/// hold, index and read a Reflection object having written nothing but a lowercase builtin name.
/// Scanning for class names alone misses it entirely, and the miss is not theoretical: it is what
/// `error_tests::classes_traits::test_error_reflection_attribute_internal_properties_are_private`
/// caught the first time this gate was wired.
///
/// This is the Reflection-side face of the hazard `crate::prelude_prune` documents as SYNTHESISED
/// ENTRY POINTS: a reference that exists only after some other pass has had its say, and that a
/// grep for the class name will never find. A builtin whose return type names one of
/// `REFLECTION_CLASS_NAMES` belongs on this list.
const REFLECTION_PRODUCING_BUILTINS: &[&str] = &["class_get_attributes"];

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// The case the gate exists for: nothing to register, and nothing to pay for.
    #[test]
    fn a_program_that_never_reflects_does_not_register() {
        assert!(!program_may_reference_reflection(&parse("<?php echo 1;")));
        assert!(!program_may_reference_reflection(&parse(
            "<?php function f(int $x): string { return (string) $x; } echo f(2);"
        )));
    }

    /// Every syntactic position that names a class is a reference, not just `new`.
    #[test]
    fn every_naming_position_counts() {
        for source in [
            "<?php $r = new ReflectionClass('Foo');",
            "<?php function f(ReflectionMethod $m) {}",
            "<?php function f($m): ReflectionProperty {}",
            "<?php class Sub extends ReflectionFunction {}",
            "<?php if ($x instanceof ReflectionEnum) { echo 1; }",
            "<?php echo ReflectionMethod::IS_PUBLIC;",
            "<?php echo ReflectionEnum::class;",
            "<?php try { f(); } catch (\\ReflectionClass $r) { echo 1; }",
        ] {
            assert!(
                program_may_reference_reflection(&parse(source)),
                "should register: {source}"
            );
        }
    }

    /// `ReflectionException` is NOT one of these fourteen classes — it is a throwable, registered
    /// with the rest of the exception hierarchy in `builtin_types::declarations`. So catching it
    /// must NOT drag in the Reflection family, and this test exists because the first version of
    /// the one above put it in the "should register" list, where it passed only because another
    /// Reflection name shared the same source. A test that passes for the wrong reason is worse
    /// than no test: it documents behaviour the code does not have.
    #[test]
    fn the_reflection_exception_is_a_throwable_and_does_not_count() {
        assert!(!program_may_reference_reflection(&parse(
            "<?php try { f(); } catch (ReflectionException $e) { echo 1; }"
        )));
    }

    /// A name that only ever exists as a string still reaches the class, through `new $c` or a
    /// probe. The literal pool is consulted for exactly this.
    #[test]
    fn a_string_that_names_the_class_counts() {
        assert!(program_may_reference_reflection(&parse(
            "<?php $c = 'ReflectionClass'; $r = new $c('Foo');"
        )));
        assert!(program_may_reference_reflection(&parse(
            "<?php var_dump(class_exists('ReflectionEnum'));"
        )));
    }

    /// PHP class names are case-insensitive, and so is the index this predicate consults.
    #[test]
    fn the_match_is_case_insensitive_and_ignores_a_leading_slash() {
        assert!(program_may_reference_reflection(&parse(
            "<?php $r = new reflectionclass('Foo');"
        )));
        assert!(program_may_reference_reflection(&parse(
            "<?php $r = new \\ReflectionClass('Foo');"
        )));
    }

    /// A builtin can hand back a Reflection object the program never names. Scanning for class
    /// names alone misses this completely — the source says nothing but a lowercase function.
    #[test]
    fn a_builtin_that_manufactures_a_reflection_object_counts() {
        assert!(program_may_reference_reflection(&parse(
            "<?php #[A] class C {} $attrs = class_get_attributes('C'); echo $attrs[0]->getName();"
        )));
    }

    /// `eval` resolves names at runtime, so no static walk can bound what it constructs. The
    /// dynamic bridge documents that it keeps the full Reflection surface for this reason.
    #[test]
    fn introspection_registers_everything() {
        assert!(program_may_reference_reflection(&parse(
            "<?php eval('$r = new ReflectionClass(\"Foo\");');"
        )));
        assert!(program_may_reference_reflection(&parse(
            "<?php print_r(get_declared_classes());"
        )));
    }
}
