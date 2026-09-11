//! Purpose:
//! Checks eval cycle roots and destructor behavior at native cleanup boundaries.
//!
//! Called from:
//! - The codegen test binary's runtime GC module.
//!
//! Key details:
//! - Opaque source uses Magician rather than static eval expansion.
//! - Elephc collects at unset safe points, so destructor ordering is asserted explicitly.
//! - A throwing cleanup must finish and leave the next collection operational.

use crate::support::*;

/// AOT and eval finish every unset operand before collecting a separate cyclic owner.
#[test]
fn test_eval_cycle_multi_operand_unset_order_matches_aot() {
    let body = r#"
class GroupCycle {
    public $name;
    public $self = null;
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$a = new GroupCycle("A"); $a->self = $a;
$b = new GroupCycle("B");
unset($a, $b);
echo "after";
"#;
    for eval in [false, true] {
        let source = if eval {
            format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);")
        } else {
            format!("<?php {body}")
        };
        assert_eq!(compile_and_run(&source), "drop:B:drop:A:after", "eval={eval}");
    }
}

/// Cyclic destructor exceptions are chained while every remaining destructor still executes.
#[test]
fn test_eval_cycle_destructor_exception_chain() {
    let output = compile_and_run(r#"<?php
$source = $argc > 0 ? '
class ChainedCycle {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; throw new Exception($this->name); }
}
$a = new ChainedCycle("A"); $a->self = $a;
$b = new ChainedCycle("B"); $b->self = $b;
unset($a, $b);
' : '';
try { eval($source); }
catch (Exception $e) { echo $e->getMessage() . ":" . $e->getPrevious()->getMessage() . ":"; }
echo "after";
"#);
    assert_eq!(output, "drop:A:drop:B:B:A:after");
}

/// Repeated catch-and-discard of cyclic destructor exceptions transfers each raw Throwable owner once.
#[test]
fn test_eval_cycle_throwable_transfer_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "$box = new OwnedThrowableCycle(); $box->self = $box; try { unset($box); } catch (Exception) {}\n".repeat(count);
        let source = format!(r#"<?php
$source = $argc > 0 ? '
class OwnedThrowableCycle {{
    public function __destruct() {{ throw new Exception("cleanup"); }}
}}
{calls}
echo "ok";
' : '';
eval($source);
"#);
        let result = compile_and_run_with_gc_stats(&source);
        assert!(result.success, "{}", result.stderr);
        assert_eq!(result.stdout, "ok");
        let (allocated, freed) = parse_gc_stats(&result.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "native pending Throwable owners survived eval catches");
}

/// Native exceptions caught and rethrown by opaque eval balance both ownership transfers.
#[test]
fn test_eval_throwable_native_round_trip_releases_both_box_owners() {
    let result = compile_and_run_with_heap_debug(r#"<?php
function throwNativePreviousOwner(): void { throw new RuntimeException("inner", 13); }
function checkNativeEvalPreviousOwner(string $source): void {
    try { eval($source); }
    catch (Exception $outer) {
        $previous = $outer->getPrevious();
        unset($outer);
        if ($previous !== null) { echo $previous->getMessage(), ":", $previous->getCode(), "|"; }
        else { echo "missing|"; }
        unset($previous);
    }
}
$source = 'try { throwNativePreviousOwner(); } catch (RuntimeException $inner) { throw new Exception("outer", 7, $inner); } // ' . $argc;
checkNativeEvalPreviousOwner($source);
checkNativeEvalPreviousOwner($source);
unset($source);
"#);
    assert!(result.success, "{}", result.stderr);
    assert_eq!(result.stdout, "inner:13|inner:13|", "{}", result.stderr);
    assert!(result.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", result.stderr);
}

/// Dynamic instanceof metadata retires native adapter boxes without consuming borrowed Mixed inputs.
#[test]
fn test_eval_dynamic_instanceof_releases_native_boxes_and_preserves_borrowed_target() {
    let result = compile_and_run_with_heap_debug(r#"<?php
class EvalDynamicOwnerBase {}
class EvalDynamicOwnerChild extends EvalDynamicOwnerBase {}
function inspectEvalDynamicOwner(mixed $borrowedTarget, string $source): void {
    eval($source);
    $typedTarget = "EvalDynamicOwnerBase";
    $first = new EvalDynamicOwnerChild();
    echo $first instanceof $typedTarget ? "typed|" : "bad|";
    unset($first);
    $second = new EvalDynamicOwnerChild();
    echo $second instanceof $borrowedTarget ? "borrowed|" : "bad|";
    echo $borrowedTarget, "|";
    unset($second);
}
$source = 'return null; // ' . $argc;
$borrowedTarget = $argc > 0 ? "EvalDynamicOwnerBase" : 0;
inspectEvalDynamicOwner($borrowedTarget, $source);
inspectEvalDynamicOwner($borrowedTarget, $source);
echo $borrowedTarget;
unset($borrowedTarget, $source);
"#);
    assert!(result.success, "{}", result.stderr);
    assert_eq!(
        result.stdout,
        "typed|borrowed|EvalDynamicOwnerBase|typed|borrowed|EvalDynamicOwnerBase|EvalDynamicOwnerBase",
        "{}",
        result.stderr
    );
    assert!(result.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", result.stderr);
}

/// A surviving object alias keeps its dynamic-property cycle readable through mbstring calls.
#[test]
fn test_mbstring_eval_cycle_root_survives_conversion() {
    let output = compile_and_run(r#"<?php
$source = $argc > 0 ? '
class RootedCycle {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo "drop:" . $this->name . ":"; }
}
$box = new RootedCycle("alive");
$box->self = $box;
$alias = $box;
unset($box);
echo mb_convert_encoding($alias->self->name, "UTF-8", "UTF-8"), ":";
unset($alias);
echo "after";
' : '';
eval($source);
"#);
    assert_eq!(output, "alive:drop:alive:after");
}

/// Every destructor can inspect other objects in a cycle before any property storage is freed.
#[test]
fn test_eval_cycle_destructors_read_intact_peer_properties() {
    let output = compile_and_run(r#"<?php
$source = $argc > 0 ? '
class PeerCycle {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() { echo $this->name . ":" . $this->peer->name . ":"; }
}
$a = new PeerCycle("A");
$b = new PeerCycle("B");
$a->peer = $b;
$b->peer = $a;
unset($a);
echo "rooted:";
unset($b);
echo "after";
' : '';
eval($source);
"#);
    assert_eq!(output, "rooted:A:B:B:A:after");
}

/// A dynamic destructor exception is catchable and does not disable a later cycle scan.
#[test]
fn test_eval_cycle_throwing_destructor_restores_collection() {
    let output = compile_and_run(r#"<?php
$source = $argc > 0 ? '
class ThrowingCycle {
    public function __construct($name) { $this->name = $name; }
    public function __destruct() {
        echo "drop:" . $this->name . ":";
        throw new Exception($this->name);
    }
}
$a = new ThrowingCycle("A"); $a->self = $a;
try { unset($a); } catch (Exception $e) { echo "caught:" . $e->getMessage() . ":"; }
$b = new ThrowingCycle("B"); $b->self = $b;
try { unset($b); } catch (Exception $e) { echo "caught:" . $e->getMessage() . ":"; }
echo "after";
' : '';
eval($source);
"#);
    assert_eq!(output, "drop:A:caught:A:drop:B:caught:B:after");
}

/// Releasing the last argument-array snapshot runs a non-throwing native destructor.
#[test]
fn test_eval_call_array_last_owner_native_non_throwing_destructor_boundary() {
    assert_eval_call_array_last_owner_destructor_boundary(true, false);
}

/// Releasing the last argument-array snapshot catches a throwing native destructor.
#[test]
fn test_eval_call_array_last_owner_native_throwing_destructor_boundary() {
    assert_eval_call_array_last_owner_destructor_boundary(true, true);
}

/// Releasing the last argument-array snapshot runs a non-throwing eval destructor.
#[test]
fn test_eval_call_array_last_owner_eval_non_throwing_destructor_boundary() {
    assert_eval_call_array_last_owner_destructor_boundary(false, false);
}

/// Releasing the last argument-array snapshot catches a throwing eval destructor.
#[test]
fn test_eval_call_array_last_owner_eval_throwing_destructor_boundary() {
    assert_eval_call_array_last_owner_destructor_boundary(false, true);
}

/// Runs one native/eval and throwing/non-throwing destructor boundary case.
fn assert_eval_call_array_last_owner_destructor_boundary(native: bool, throwing: bool) {
    let throw = if throwing {
        "throw new Exception(\"cleanup\");"
    } else {
        ""
    };
    let class = format!(
        "class CallArrayLastOwner {{ public function __destruct() {{ echo \"drop:\"; {throw} }} }}"
    );
    let (native_class, eval_class) = if native {
        (class.as_str(), "")
    } else {
        ("", class.as_str())
    };
    let source = format!(r#"<?php
{native_class}
$source = $argc > 0 ? '
{eval_class}
$arguments = [new CallArrayLastOwner()];
$callback = function($value) use (&$arguments) {{ $arguments = []; echo "body:"; }};
try {{ call_user_func_array($callback, $arguments); }}
catch (Exception $e) {{ echo "caught:" . $e->getMessage() . ":"; }}
echo "after";
' : '';
eval($source);
"#);
    let output = compile_and_run(&source);
    let expected = if throwing {
        "body:drop:caught:cleanup:after"
    } else {
        "body:drop:after"
    };
    assert_eq!(output, expected, "native={native}, throwing={throwing}");
}
