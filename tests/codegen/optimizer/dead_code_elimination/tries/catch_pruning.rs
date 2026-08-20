//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries catch pruning, including dead code elimination drops unreachable catch after non throwing try, dead code elimination drops unreachable catch before finally, and dead code elimination drops shadowed throwable catch from user assembly.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a catch block is dropped when the try body cannot throw. Confirms "t!".
#[test]
fn test_dead_code_elimination_drops_unreachable_catch_after_non_throwing_try() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
}
echo "!";
"#,
    );

    assert_eq!(out, "t!");
}

/// Verifies that a catch block preceding a finally block is dropped when unreachable.
/// Confirms "tf!".
#[test]
fn test_dead_code_elimination_drops_unreachable_catch_before_finally() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "tf!");
}

/// Verifies that a shadowed catch (Throwable before Exception) is dropped from assembly.
/// Confirms "a!" with "shadowed" absent from user assembly.
#[test]
fn test_dead_code_elimination_drops_shadowed_throwable_catch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_throwable_catch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    throw new Exception("boom");
} catch (Throwable $t) {
    echo "a";
} catch (Exception $e) {
    echo "shadowed";
}
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("shadowed"),
        "shadowed catch body should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a!");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies that adjacent catch blocks with identical bodies (same `pow` call) are merged.
/// Confirms output "1".
#[test]
fn test_dead_code_elimination_merges_identical_adjacent_catches() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_merge_identical_catches");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
function boom($flag) {
    if ($flag) {
        throw new A("a");
    }
    throw new B("b");
}
try {
    boom($argc > 1);
} catch (A $e) {
    echo pow($argc, 3);
} catch (B $e) {
    echo pow($argc, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "1");
}

/// Verifies that multi-catch types are deduplicated when their merged set is identical to
/// one branch. Confirms "8".
#[test]
fn test_dead_code_elimination_deduplicates_merged_catch_types() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_dedup_catch_types");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
class C extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new A("a");
    }
    if ($flag === 2) {
        throw new B("b");
    }
    throw new C("c");
}
try {
    boom($argc);
} catch (A | B $e) {
    echo pow(2, 3);
} catch (B | C $e) {
    echo pow(2, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "8");
}

/// Verifies that multi-catch with sorted types (Zed | Alpha | Mid) is accepted and handles
/// the catch correctly. Confirms "ok".
#[test]
fn test_dead_code_elimination_accepts_sorted_multi_catch_types() {
    let out = compile_and_run(
        r#"<?php
class Alpha extends Exception {}
class Mid extends Exception {}
class Zed extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new Zed("z");
    }
    if ($flag === 2) {
        throw new Alpha("a");
    }
    throw new Mid("m");
}
try {
    boom($argc);
} catch (Zed | Alpha | Mid $e) {
    echo "ok";
}
"#,
    );

    assert_eq!(out, "ok");
}

/// Verifies exact thrown classes remove handlers that cannot match, including a handler
/// that precedes the reachable superclass catch. Confirms "a" without the dead marker.
#[test]
fn test_dead_code_elimination_prunes_handler_disjoint_from_exact_throw_type() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_exact_throw_handler");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
try {
    throw new A("a");
} catch (B $e) {
    echo "dead-exact-handler";
} catch (Exception $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-exact-handler"),
        "handler disjoint from the exact thrown class should be removed:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies fixed-point callable summaries carry an exact thrown class through a direct
/// function call so a disjoint handler can be removed. Confirms "a" without the dead marker.
#[test]
fn test_dead_code_elimination_prunes_handler_disjoint_from_callable_throw_summary() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_callable_throw_handler");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
function boom(): never {
    throw new A("a");
}
try {
    boom();
} catch (B $e) {
    echo "dead-callable-handler";
} catch (A $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-callable-handler"),
        "handler disjoint from a callable throw summary should be removed:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a fixed-point method summary is precise for a syntactically exact `new` receiver,
/// removing the disjoint handler while preserving and executing the matching one.
#[test]
fn test_dead_code_elimination_prunes_handler_from_exact_method_throw_summary() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_exact_method_throw_handler");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
class Worker {
    public function boom(): void {
        throw new A("a");
    }
}
try {
    (new Worker())->boom();
} catch (B $e) {
    echo "dead-method-handler";
} catch (A $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-method-handler"),
        "handler disjoint from an exact method throw summary should be removed:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `$this` calls retain handlers for throwing subclass overrides rather than using the
/// lexical base implementation as an exact dispatch target. Confirms dynamic dispatch prints "a".
#[test]
fn test_dead_code_elimination_keeps_handler_for_this_override_throw() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_this_override_throw");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class Base {
    public function fail(): void {}
    public function run(): void {
        try {
            $this->fail();
        } catch (A $e) {
            echo "a";
        }
    }
}
class Child extends Base {
    public function fail(): void {
        throw new A("a");
    }
}
(new Child())->run();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a class using a trait keeps constructor catches conservative because the effective
/// constructor may come from the trait rather than the class method list. Confirms "a".
#[test]
fn test_dead_code_elimination_keeps_handler_for_trait_constructor_throw() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_trait_constructor_throw");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
trait BuildsWithFailure {
    public function __construct() {
        throw new A("a");
    }
}
class UsesBuilder {
    use BuildsWithFailure;
}
try {
    new UsesBuilder();
} catch (A $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a trait method can override an inherited non-throwing method on an exact receiver,
/// so hierarchy lookup must stop at the trait-use barrier. Confirms the retained catch prints "a".
#[test]
fn test_dead_code_elimination_keeps_handler_for_trait_method_override_throw() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_trait_method_override_throw");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
trait Fails {
    public function fail(): void {
        throw new A("a");
    }
}
class Base {
    public function fail(): void {}
}
class Child extends Base {
    use Fails;
}
try {
    (new Child())->fail();
} catch (A $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies an operator with a statically known PHP failure class routes only to compatible
/// handlers. Division by a numeric literal zero reaches `DivisionByZeroError`, not `Exception`.
#[test]
fn test_dead_code_elimination_routes_exact_operator_failure_type() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_operator_throw_handler");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    echo 1 / 0;
} catch (Exception $e) {
    echo "dead-operator-handler";
} catch (DivisionByZeroError $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-operator-handler"),
        "Exception cannot catch the exact DivisionByZeroError operator failure:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies `Exception` plus `Error` exhaust PHP's `Throwable` roots even when the incoming
/// throwable class is unknown. The later `Throwable` handler is absent and execution prints "error".
#[test]
fn test_dead_code_elimination_prunes_throwable_after_both_root_families() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_throwable_root_partition");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    echo 1 / ($argc - 1);
} catch (Exception $e) {
    echo "exception";
} catch (Error $e) {
    echo "error";
} catch (Throwable $e) {
    echo "dead-root-handler";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-root-handler"),
        "Exception and Error together should exhaust the Throwable domain:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "error");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a caught-variable rethrow from a nested try preserves its exact dynamic class,
/// allowing the outer disjoint handler to be removed. Confirms "a" without the dead marker.
#[test]
fn test_dead_code_elimination_models_nested_catch_variable_rethrow_type() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_nested_rethrow_type");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
try {
    try {
        throw new A("a");
    } catch (A $e) {
        throw $e;
    }
} catch (B $e) {
    echo "dead-rethrow-handler";
} catch (A $e) {
    echo "a";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-rethrow-handler"),
        "outer handler disjoint from the rethrown class should be removed:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies rethrowing a catch variable preserves a constrained unknown `Exception` domain after
/// an unknown callback, while a sibling inner `Error` catch consumes the other Throwable root.
#[test]
fn test_dead_code_elimination_models_constrained_unknown_rethrow_domain() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_constrained_rethrow_domain");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
function relay(callable $callback): void {
    try {
        try {
            $callback();
        } catch (Exception $e) {
            throw $e;
        } catch (Error $e) {
        }
    } catch (Error $e) {
        echo "dead-constrained-rethrow-handler";
    } catch (Exception $e) {
        echo "a";
    }
}
relay(function (): void {
    throw new A("a");
});
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm)
            .contains("dead-constrained-rethrow-handler"),
        "outer Error handler should be disjoint from the constrained Exception rethrow:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a try nested inside a catch inherits the caught variable's exact domain, so its
/// own disjoint handler is removed before codegen. Confirms "a" without the dead marker.
#[test]
fn test_dead_code_elimination_routes_rethrow_inside_nested_try_in_catch() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_catch_nested_try_rethrow");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
try {
    throw new A("a");
} catch (A $e) {
    try {
        throw $e;
    } catch (B $nested) {
        echo "dead-nested-rethrow-handler";
    } catch (A $nested) {
        echo "a";
    }
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-nested-rethrow-handler"),
        "nested try should inherit the caught variable's exact throw domain:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies writes on a different exact exception path do not invalidate guards for the
/// selected handler. Confirms "a" and removes the impossible branch from user assembly.
#[test]
fn test_dead_code_elimination_invalidates_catch_guards_by_matching_throw_type() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_typed_catch_guards");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
function run(bool $flag, bool $throw_b): void {
    if ($flag) {
        try {
            if ($throw_b) {
                $flag = false;
                throw new B("b");
            }
            throw new A("a");
        } catch (A $e) {
            if ($flag) {
                echo "a";
            } else {
                echo "dead-a-guard";
            }
        } catch (B $e) {
            echo "b";
        }
    }
}
run(true, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !asm_without_embedded_script_path(&user_asm).contains("dead-a-guard"),
        "writes on the B path should not invalidate the A handler guard:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies a by-reference write performed by the throwing call itself invalidates the incoming
/// catch guard. Both branches remain in assembly and runtime observes the mutated false value.
#[test]
fn test_dead_code_elimination_invalidates_catch_guard_for_throwing_by_ref_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_throwing_by_ref_call_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
function mutate_then_throw(bool &$flag): never {
    $flag = false;
    throw new A("a");
}
function run(bool $flag): void {
    if ($flag) {
        try {
            mutate_then_throw($flag);
        } catch (A $e) {
            if ($flag) {
                echo "stale-call-guard";
            } else {
                echo "a";
            }
        }
    }
}
run(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        asm_without_embedded_script_path(&user_asm).contains("stale-call-guard"),
        "the by-ref call write must invalidate the outer true guard:\n{}",
        user_asm
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a");

    let _ = fs::remove_dir_all(&dir);
}

/// Verifies caught-variable rebinds in `for` initialization and update positions do not retain
/// the stale incoming exception domain. Both runtime `B` handlers remain reachable.
#[test]
fn test_dead_code_elimination_invalidates_caught_domain_inside_for() {
    let out = compile_and_run(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
try {
    throw new A("a");
} catch (Exception $e) {
    try {
        for ($e = new B("b"); ; ) {
            throw $e;
        }
    } catch (A $nested) {
        echo "stale";
    } catch (B $nested) {
        echo "b";
    }
}
try {
    throw new A("a");
} catch (Exception $e) {
    $i = 0;
    try {
        for (; $i < 2; $e = new B("b")) {
            if ($i === 1) {
                throw $e;
            }
            $i++;
        }
    } catch (A $nested) {
        echo "stale-update";
    } catch (B $nested) {
        echo "u";
    }
}
"#,
    );

    assert_eq!(out, "bu");
}

/// Verifies a caught variable rebound while evaluating an `if` condition cannot retain its
/// incoming exception domain in the branch body. The reachable `B` handler prints "b".
#[test]
fn test_dead_code_elimination_invalidates_caught_domain_after_condition_rebind() {
    let out = compile_and_run(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
try {
    throw new A("a");
} catch (Exception $e) {
    try {
        if (($e = new B("b")) instanceof B) {
            throw $e;
        }
    } catch (A $nested) {
        echo "stale";
    } catch (B $nested) {
        echo "b";
    }
}
"#,
    );

    assert_eq!(out, "b");
}
