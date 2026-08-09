//! Purpose:
//! Decides whether a program can reach the builtin date/time classes at all.
//!
//! Called from:
//! - `crate::types::checker::driver`, which passes the answer to `inject_builtin_datetime`,
//!   `inject_builtin_date_period` and `inject_builtin_date_exceptions`.
//!
//! Key details:
//! - The predicate is a conservative over-approximation; under-detecting is a compile error.

use crate::names::php_symbol_key;
use crate::parser::ast::Stmt;

/// Every class name the date/time injection registers, and the names this gate looks for.
///
/// Kept in injection order so the two can be read side by side. `DateTimeInterface` is an
/// INTERFACE and lives in the interface id space, not this one, but naming it still has to
/// register the family: `function f(DateTimeInterface $d)` is a program that reaches DateTime
/// without ever writing the class name.
pub(crate) const DATETIME_CLASS_NAMES: &[&str] = &[
    "DateTimeInterface",
    "DateInterval",
    "DateTimeZone",
    "DateTimeImmutable",
    "DateTime",
    "DatePeriod",
    "DateError",
    "DateObjectError",
    "DateRangeError",
    "DateException",
    "DateInvalidTimeZoneException",
    "DateInvalidOperationException",
    "DateMalformedStringException",
    "DateMalformedIntervalStringException",
    "DateMalformedPeriodStringException",
    "DateUnknownException",
];

/// Returns whether `program` can reach any builtin date/time class.
///
/// WHY THIS GATE EXISTS, measured rather than assumed. The `_class_*` metadata tables are dense
/// arrays `max_class_id + 1` entries wide, and a class the checker registers but codegen never
/// emits still claims its slot — 184 bytes across 22 id-indexed tables, counted from the
/// emitted assembly. For `<?php echo 1;` 35 of 44 slots were sentinels, and this family held
/// the largest single share: gating it takes that program to 29 slots and its type-check phase
/// from 13.75 ms to 3.30 ms. It is the same shape as the SPL and Reflection gates next door, and
/// the same argument applies to the checker work these classes cost: DateTime alone carries
/// around thirty synthetic methods to flatten, patch and validate.
///
/// THE FAILURE MODE IS LOUD, which is what makes the gate acceptable. Under-detect and the
/// checker reports `Unknown type: DateTime` at the reference site — a compile error the user
/// reads, not a miscompile.
///
/// IT SEES THE PROGRAM AFTER NAME RESOLUTION, and that is load-bearing here in a way it is not
/// for Reflection. PHP's procedural date API never spells a class name, but
/// `name_resolver::expressions::rewrite_date_procedural_alias` has already rewritten
/// `date_create($s)` into `new DateTime($s)` and `date_diff($a, $b)` into `$a->diff($b)` by the
/// time this runs. So the constructors are visible as class references, and the alias list does
/// not have to be mirrored here — the rewrite is the single source of truth. Every alias that
/// produces an object goes through a `new`; the ones that lower to a method call operate on a
/// receiver some `new` in the same program produced.
///
/// UNSERIALIZE NAMES A CLASS IN ITS DATA, not in the source. `unserialize('O:8:"DateTime":…')`
/// reaches the class through a string the compiler never sees, so its presence registers the
/// family unconditionally — the same reasoning that makes `eval` register everything.
///
/// A DYNAMIC CALL TO AN ALIAS IS NOT A HOLE, though it looks like one. `$f = 'date_create';
/// $f('now');` never reaches the rewrite, which matches a literal callee only — but there is no
/// `date_create` body for the callable dispatch to find either, because the alias exists solely
/// as that rewrite. Such a program does not work today and this gate does not change that.
/// `usage.dynamic_function_call` is therefore deliberately not consulted; consulting it would
/// open the gate for every program that calls anything through a variable.
pub(crate) fn program_may_reference_datetime(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    if usage.introspects {
        return true;
    }
    if DATETIME_PRODUCING_BUILTINS
        .iter()
        .any(|name| usage.references(name))
    {
        return true;
    }
    DATETIME_CLASS_NAMES.iter().any(|name| {
        let key = php_symbol_key(name);
        usage.classes.contains(&key) || usage.literals.contains(&key)
    })
}

/// Builtins that reach a date/time class without the program naming one.
///
/// `unserialize` builds an object from a class name held in its DATA, where no static walk can
/// read it. It is the only entry, and the list is short on purpose: the ninety-odd procedural
/// date builtins look like they belong here and do not, because
/// `name_resolver::expressions::rewrite_date_procedural_alias` has already turned each of them
/// into an expression that names a class — `date_sun_info(…)` becomes
/// `DateTime::__elephc_date_sun_info(…)`, `timezone_identifiers_list()` becomes
/// `DateTimeZone::listIdentifiers()`. Listing them again would be a second copy of that table,
/// free to drift, justified by a reason that is not true.
///
/// This is still the hazard `builtin_types::reflection::gate` documents as SYNTHESISED ENTRY
/// POINTS — a reference that exists only after another pass has had its say. Here the pass that
/// has its say runs BEFORE the gate, so it hands the reference over instead of hiding it.
const DATETIME_PRODUCING_BUILTINS: &[&str] = &["unserialize"];

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// The case the gate exists for: nothing to register, and nothing to pay for.
    #[test]
    fn a_program_that_never_dates_does_not_register() {
        assert!(!program_may_reference_datetime(&parse("<?php echo 1;")));
        assert!(!program_may_reference_datetime(&parse(
            "<?php function f(int $x): string { return (string) $x; } echo f(2);"
        )));
    }

    /// `date()`, `time()` and `strtotime()` are plain builtins returning scalars. They are the
    /// reason this gate is worth having: the overwhelmingly common way to use dates in PHP
    /// touches no class at all.
    #[test]
    fn the_scalar_date_builtins_do_not_register() {
        assert!(!program_may_reference_datetime(&parse(
            "<?php echo date('Y-m-d', time());"
        )));
        assert!(!program_may_reference_datetime(&parse(
            "<?php echo strtotime('+1 day');"
        )));
    }

    /// Every syntactic position that names a class is a reference, not just `new`.
    #[test]
    fn every_naming_position_counts() {
        for source in [
            "<?php $d = new DateTime('now');",
            "<?php function f(DateTimeImmutable $d) {}",
            "<?php function f($d): DateInterval {}",
            "<?php class Sub extends DateTime {}",
            "<?php if ($x instanceof DateTimeZone) { echo 1; }",
            "<?php echo DateTime::ATOM;",
            "<?php echo DatePeriod::class;",
            "<?php try { f(); } catch (DateMalformedStringException $e) { echo 1; }",
        ] {
            assert!(
                program_may_reference_datetime(&parse(source)),
                "should register: {source}"
            );
        }
    }

    /// The interface is the one name a program can use to touch DateTime while naming no class
    /// this gate registers. It is in the list precisely so that this compiles.
    #[test]
    fn the_interface_registers_the_family() {
        assert!(program_may_reference_datetime(&parse(
            "<?php function f(DateTimeInterface $d): string { return $d->format('c'); }"
        )));
    }

    /// The procedural API never spells a class, and one KIND of alias has already been turned
    /// into one by the time this predicate runs: the arms that rewrite to a `new` or to a static
    /// call on `DateTime`/`DateTimeZone`. Parsing alone does not rewrite, so both halves are
    /// asserted — if such an arm ever stopped naming a class, this is the test that fails.
    ///
    /// THE OTHER KIND IS NOT COVERED HERE AND MUST NOT BE ASSERTED HERE. `timezone_identifiers_list()`
    /// rewrites to the prelude free function `__elephc_list_identifiers()`, which names no class
    /// at all; what registers the family for those programs is the prelude's own body, injected
    /// upstream of the checker and invisible to a unit test that only parses and resolves. The
    /// first version of this test asserted them anyway and failed — correctly, for a property it
    /// was looking at the wrong layer to see. `scripts/` has no home for it, so the end-to-end
    /// check lives in the codegen suite next to the other tz tests.
    #[test]
    fn an_alias_that_rewrites_to_a_class_registers_once_resolved() {
        for source in [
            "<?php $d = date_create('now'); echo $d->format('Y');",
            "<?php print_r(date_sun_info(time(), 31.7667, 35.2333));",
            "<?php echo date_sunrise(time());",
            "<?php $a = date_create('now'); $b = date_create('now'); print_r(date_diff($a, $b));",
        ] {
            assert!(
                !program_may_reference_datetime(&parse(source)),
                "unresolved, the alias names nothing: {source}"
            );
            let resolved = crate::name_resolver::resolve(parse(source)).expect("resolve");
            assert!(
                program_may_reference_datetime(&resolved),
                "resolved, the alias must name a class: {source}"
            );
        }
    }

    /// A name that only ever exists as a string still reaches the class, through `new $c`.
    #[test]
    fn a_string_that_names_the_class_counts() {
        assert!(program_may_reference_datetime(&parse(
            "<?php $c = 'DateTime'; $d = new $c('now');"
        )));
        assert!(program_may_reference_datetime(&parse(
            "<?php var_dump(class_exists('DateInterval'));"
        )));
    }

    /// PHP class names are case-insensitive, and so is the index this predicate consults.
    #[test]
    fn the_match_is_case_insensitive_and_ignores_a_leading_slash() {
        assert!(program_may_reference_datetime(&parse(
            "<?php $d = new datetime('now');"
        )));
        assert!(program_may_reference_datetime(&parse(
            "<?php $d = new \\DateTimeImmutable('now');"
        )));
    }

    /// `unserialize` carries the class name in its payload, where no static walk can read it.
    #[test]
    fn unserialize_registers_everything() {
        assert!(program_may_reference_datetime(&parse(
            "<?php $d = unserialize($payload);"
        )));
    }

    /// `eval` resolves names at runtime, so no static walk can bound what it constructs.
    #[test]
    fn introspection_registers_everything() {
        assert!(program_may_reference_datetime(&parse(
            "<?php eval('$d = new DateTime(\"now\");');"
        )));
        assert!(program_may_reference_datetime(&parse(
            "<?php print_r(get_declared_classes());"
        )));
    }
}
