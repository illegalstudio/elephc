//! Purpose:
//! Regression tests for generators reached ONLY through the runtime eval bridge: a class or
//! function containing `yield`, declared in a file whose path is known only at run time (an
//! `include $variable`), so the compiled entry point's own AST never spells `yield` or
//! `Generator` anywhere a static scan can see.
//!
//! Called from:
//!  - `cargo test` via the integration test harness; aggregated under
//!    `tests::codegen::generators` in `tests/codegen/generators/mod.rs`.
//!
//! Key details:
//!  - Before the fix, calling a generator function or method reached ONLY through a dynamic
//!    include died with `Fatal error: eval() runtime failed: unsupported MethodCall expression`
//!    (or `unsupported Call expression` for a plain function). The label was a red herring: the
//!    interpreter's own generator machinery (step-list lowering, resumption, `yield from`, ...)
//!    is complete and already covered by `crates/elephc-magician`'s own fragment tests. The
//!    actual defect was two static "does this program need the builtin `Generator` class"
//!    gates that only checked the compiled entry file's OWN literal AST for a `yield` or the
//!    spelled name `Generator` (`types::checker::builtin_class_gate::program_may_reference_generator`
//!    and `codegen::runtime_metadata::classes::runtime_referenced_class_names`), so neither ever
//!    fired when the only `yield` lived in a dynamically included file. Without a registered
//!    `Generator` class, `new_object("Generator")` returned null, `eval_generator_new` turned
//!    that into a bare `RuntimeFatal` with no message, and the outermost `MethodCall`/`Call`
//!    expression picked up the generic "unsupported X expression" label -- exactly the kind of
//!    mislabeling this campaign has hit before (a double free, a premature destruct, a missing
//!    scope push, an ownership undercount). This is the Symfony `--web` 404 blocker:
//!    `Kernel::doInitializeBundles()` runs `foreach ($this->registerBundles() as $bundle)` where
//!    `registerBundles()` is a generator method reached only through the same dynamic-include
//!    path every autoloaded vendor file uses.
//!  - Expected values are real `LC_ALL=C php -n` 8.5.6 output.

use crate::support::*;

/// Verifies a generator METHOD reached only through a dynamic include iterates correctly.
///
/// `main.php` contains no `yield` and never spells `Generator`, so both static gates that used
/// to decide whether the builtin `Generator` class exists at all saw nothing. `php -n` 8.5.6
/// prints `yield_in_method:3`.
#[test]
fn test_generator_method_reached_only_through_dynamic_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include $path; } load('piece.php');",
            ),
            (
                "piece.php",
                r#"<?php
class Y { public function items(): iterable { yield 1; yield 2; } }
$y = new Y();
$s = 0;
foreach ($y->items() as $v) { $s += $v; }
echo "yield_in_method:", $s, "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "yield_in_method:3\n");
}

/// Verifies a generator FUNCTION reached only through a dynamic include iterates correctly, and
/// that the returned object satisfies `instanceof Generator`/`Traversable`/`Iterator` even
/// though the compiled entry point never names any of the three.
///
/// `php -n` 8.5.6 prints `yield_in_function:a` then `GTI`.
#[test]
fn test_generator_function_reached_only_through_dynamic_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php function load($path) { include $path; } load('piece.php');",
            ),
            (
                "piece.php",
                r#"<?php
function gen(): iterable { yield "a"; }
$g = gen();
foreach ($g as $v) { echo "yield_in_function:", $v, "\n"; }
echo $g instanceof Generator ? "G" : "-";
echo $g instanceof Traversable ? "T" : "-";
echo $g instanceof Iterator ? "I" : "-";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "yield_in_function:a\nGTI");
}


/// An interpreted GENERATOR keeps the arguments it was called with, literals included.
///
/// A generator body does not run during the call that creates it: PHP binds the arguments and
/// hands back a `Generator` whose scope OUTLIVES that frame. `bind_method_scope_args` bound every
/// by-value parameter `Borrowed` and the caller then ran `release_owned_bound_args` as soon as the
/// generator object existed, so for an argument the CALLER OWNS — a literal, a concatenation, any
/// temporary — that release was the last one and the generator later read a freed cell:
///
///     function gen($a, $b) { yield $a . '|' . $b; }
///     gen('L1', 'L2');                 // yielded '|'
///     $x = 'V1'; gen($x, 'L2');        // yielded 'V1|' — the VARIABLE survived
///
/// That asymmetry is why it only ever reproduced with literals, and why it never showed up in a
/// compiled generator at all. `retain_generator_scope_args` now retains them into the generator's
/// own scope, the same rule the `$this` binding at each generator site already followed.
///
/// Twig hits it on every render — `$this->unwrap()->yieldBlock('title', $context, $blocks)` sits
/// in every compiled template — where the lost `$name` surfaced as
/// `Block "" on template "base.html.twig" does not exist`.
///
/// The fixture runs through `eval()` so the generator is the INTERPRETER's, which is the only side
/// that was wrong, and pins a plain method beside it so a shared regression cannot hide.
///
/// Oracle: `php -n` prints the asserted lines.
#[test]
fn test_an_interpreted_generator_keeps_its_literal_arguments() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class G {
    public function self_() { return $this; }
    public function plain($a, $b, $c) { return $a . '|' . $b . '|' . $c; }
    public function gen($a, $b, $c) { yield $a . '|' . $b . '|' . $c; }
    public function chained(array $x) { yield from $this->self_()->gen('L1', $x[0], 'L3'); }
}
function free_gen($a, $b) { yield $a . '/' . $b; }
$g = new G();
$v = 'VAR';
echo $g->plain('L1', 'L2', 'L3'), "\n";
echo implode('', iterator_to_array($g->gen('L1', 'L2', 'L3'))), "\n";
echo implode('', iterator_to_array($g->gen($v, 'L2', 3))), "\n";
echo implode('', iterator_to_array($g->chained(['MID']))), "\n";
echo implode('', iterator_to_array(free_gen('A', 'B'))), "\n";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(
        out,
        "L1|L2|L3\nL1|L2|L3\nVAR|L2|3\nL1|MID|L3\nA/B\n"
    );
}
