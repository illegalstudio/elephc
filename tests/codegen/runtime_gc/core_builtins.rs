//! Purpose:
//! End-to-end tests for PHP Core cycle-collector builtins in the AOT backend.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the runtime GC suite.
//!
//! Key details:
//! - Cases cover direct, namespaced, first-class, and runtime string-callable dispatch.
//! - Explicit collection must remain active while automatic safe points are disabled.

use crate::support::*;

/// Mangled native/eval inventories release metadata arrays, keys, and property result cells.
#[test]
fn test_core_eval_mangled_inventory_releases_metadata_temporaries() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeMangledGcParent { private int $secret = 3; protected int $guard = 4; }",
        "class EvalMangledGcChild extends NativeMangledGcParent { public int $secret = 5; }
         $object = new EvalMangledGcChild();",
        "$vars = get_mangled_object_vars($object); unset($vars);",
    );
}

/// SPL mutators own inserted values and offset readers do not consume the eval caller's indexes.
#[test]
fn test_core_eval_spl_intrinsics_preserve_caller_cells_and_stored_values() {
    let source = r#"<?php
$source = '$list = new SplDoublyLinkedList();
$value = str_repeat("v", 24); $index = 0;
$list->push($value); unset($value);
echo strlen($list->offsetGet($index)), ":", $index, "|";
$list->offsetSet($index, "replacement");
echo $list->offsetExists($index), ":", $list->offsetGet($index), "|";
$list->unshift("front"); $list->add(1, "middle");
echo $list->shift(), ":", $list->pop(), ":", $list->pop(), "|";
$queue = new SplQueue(); $queue->enqueue("queued"); echo $queue->dequeue(), "|";
$fixed = new SplFixedArray(1); $value = "fixed";
$fixed->offsetSet($index, $value); unset($value);
echo $fixed->offsetGet($index), ":", $index, "|";
$fixed->offsetUnset($index); echo $fixed->offsetExists($index) ? "bad" : "empty";
try { $fixed->offsetSet(2, "rejected"); } catch (OutOfBoundsException $error) { echo "|range"; }
echo ":", $index;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "24:0|1:replacement|front:replacement:middle|queued|fixed:0|empty|range:0");
}

/// SPL insertion and offset operations release their transferred owners across repeated invocations.
#[test]
fn test_core_eval_spl_intrinsic_transfers_release_consumed_cells() {
    assert_core_eval_collection_cleanup(
        "",
        "$list = new SplDoublyLinkedList(); $index = 0;
         $list->push(\"first\"); $list->offsetSet($index, \"second\");
         $value = $list->offsetGet($index); unset($value);
         $present = $list->offsetExists($index); unset($present);
         $list->offsetUnset($index); unset($list);
         $fixed = new SplFixedArray(1); $fixed->offsetSet($index, \"fixed\");
         $value = $fixed->offsetGet($index); unset($value);
         $fixed->offsetUnset($index); unset($fixed); unset($index);",
    );
}

/// Native property storage must retain borrowed variable cells independently of later assignments.
#[test]
fn test_core_eval_property_assignments_keep_independent_cell_owners() {
    let source = r#"<?php
$source = '$box = new stdClass(); $value = "first";
$box->value = $value; $value = "second";
echo $box->value, ":", $value, "|";
$name = "value"; $box->{$name} = $value; unset($value);
echo $box->value, "|";
class PropertyAlias { public $value = "old"; }
$alias = new PropertyAlias(); $original = "A";
$alias->value =& $original; $original = "B";
echo $alias->value, "|"; $alias->value = "C";
echo $original; return 42;' . ' // ' . $argc;
echo ":", eval($source);
"#;
    assert_eq!(compile_and_run(source), ":first:second|second|B|C42");
}

/// Direct and dynamic property assignments release fresh values and receiver/name leases.
#[test]
fn test_core_eval_property_assignments_release_temporary_cells() {
    assert_core_eval_collection_cleanup(
        "$box = new stdClass(); $name = \"value\";",
        "$box->value = \"first\";
         $box->{$name} = \"second\";
         $box->{\"value\"} = [1, 2, 3];",
    );
}

/// Eval object defaults and assignment values are freed when their sole object owner disappears.
#[test]
fn test_core_eval_property_defaults_release_temporary_cells() {
    assert_core_eval_collection_cleanup(
        "class OwnedDefaults { public string $value = \"initial\"; }",
        "$box = new OwnedDefaults(); $box->value = \"changed\"; unset($box);",
    );
}

/// Concatenation releases the owned string copies created for both boxed operands.
#[test]
fn test_core_eval_concat_releases_string_cast_copies() {
    assert_core_eval_collection_cleanup(
        "$left = \"left\"; $right = \"right\";",
        "$joined = $left . $right; unset($joined);",
    );
}

/// Object concatenation releases string-hook results in either operand and after a later throw.
#[test]
fn test_core_eval_concat_releases_tostring_result_cells() {
    assert_core_eval_collection_cleanup(
        "class ConcatValue { public function __toString(): string { return \"value\"; } }
         class ConcatFailure { public function __toString(): string { throw new Exception(\"stop\"); } }
         $object = new ConcatValue(); $failure = new ConcatFailure();",
        "$joined = $object . \"right\"; unset($joined);
         $joined = \"left\" . $object; unset($joined);
         $joined = $object . $object; unset($joined);
         try { $joined = $object . $failure; } catch (Exception $caught) { unset($caught); }",
    );
}

/// Successful dynamic string hooks release their returned cells without relying on exception cleanup.
#[test]
fn test_core_eval_concat_successful_hooks_release_results() {
    assert_core_eval_collection_cleanup(
        "class ConcatSuccess { public function __toString(): string { return \"value\"; } }
         $object = new ConcatSuccess();",
        "$joined = $object . $object; unset($joined);",
    );
}

/// Repeated native calls with borrowed arguments release the bridge's boxed argument indexes.
#[test]
fn test_core_eval_native_argument_packing_releases_indexes() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeArgumentSink { public function accept(int $value): void {} }",
        "$object = new NativeArgumentSink(); $value = 7;",
        "$object->accept($value);",
    );
}

/// Native method syntax releases fresh positional, named, and dynamic-call operands.
#[test]
fn test_core_eval_native_method_calls_release_source_temporaries() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeTemporarySink {
            public function accept(int $value): void {}
            public static function acceptStatic(string $value): void {}
         }",
        "$object = new NativeTemporarySink(); $method = \"accept\";
         $class = \"NativeTemporarySink\"; $staticMethod = \"acceptStatic\";",
        "$object->accept(7);
         $object->accept(value: 8);
         $object->{$method}(9);
         $object?->accept(10);
         $object?->{$method}(11);
         $object->accept(...[12]);
         $object->accept(...[\"value\" => 13]);
         NativeTemporarySink::acceptStatic(\"temporary\");
         $class::{$staticMethod}(\"dynamic\");",
    );
}

/// Source argument cleanup preserves returns aliasing native and eval method parameters.
#[test]
fn test_core_eval_method_argument_cleanup_preserves_return_values() {
    let source = r#"<?php
class NativeIdentityMethod {
    public static function identity(string $value): string { return $value; }
}
$source = 'class DynamicIdentityMethod {
    public function identity($value) { return $value; }
}
$object = new DynamicIdentityMethod();
$first = NativeIdentityMethod::identity("native");
$second = $object->identity("dynamic");
echo $first, ":", $second;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "native:dynamic");
}

/// Native static calls free nested default arrays and their materialization operands.
#[test]
fn test_core_eval_native_default_arguments_release_nested_arrays() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeDefaultSink { public static function accept(array $items = [1, 2]): void {} }",
        "",
        "NativeDefaultSink::accept();",
    );
}

/// Repeated reflection writes release displaced strings and arrays without leaking same-cell writes.
#[test]
fn test_core_eval_reflection_static_replacements_release_displaced_values() {
    assert_core_eval_collection_cleanup(
        "class RetainedStaticSlots { public static $value = null; }
         $class = new ReflectionClass(\"RetainedStaticSlots\");
         $property = new ReflectionProperty(\"RetainedStaticSlots\", \"value\");",
        "$class->setStaticPropertyValue(\"value\", str_repeat(\"x\", 24));
         $property->setValue(null, [\"payload\" => [7, 8]]);
         $same = RetainedStaticSlots::$value;
         $class->setStaticPropertyValue(\"value\", $same);
         unset($same);
         $property->setValue(null, null);",
    );
}

/// The string-producing operand used by reflection GC fixtures leaves no per-call argument cells.
#[test]
fn test_core_eval_str_repeat_releases_operand_cells() {
    assert_core_eval_collection_cleanup("", "$text = str_repeat(\"x\", 24); unset($text);");
}

/// Object-valued defaults release both their constructor operands and the completed temporary object.
#[test]
fn test_core_eval_native_default_arguments_release_objects() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeDefaultPayload {
            public string $text;
            public function __construct(string $text) { $this->text = $text; }
         }
         class NativeObjectDefaultSink {
            public static function accept(NativeDefaultPayload $value = new NativeDefaultPayload(\"value\")): void {}
         }",
        "",
        "NativeObjectDefaultSink::accept();",
    );
}

/// Unchanged nullable references release native slot owners for constructors and both method kinds.
#[test]
fn test_core_eval_native_unchanged_mixed_references_release_activation_leases() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeUnchangedReferenceSink {
            public function __construct(?string &$value) {}
            public function instance(?string &$value): void {}
            public static function unchanged(?string &$value): void {}
         }",
        "",
        "$value = \"kept\";
         $object = new NativeUnchangedReferenceSink($value);
         $object->instance($value);
         NativeUnchangedReferenceSink::unchanged($value);
         $value = null;
         $object->instance($value);
         NativeUnchangedReferenceSink::unchanged($value);
         unset($object, $value);",
    );
}

/// Native by-value returns detach from nullable Mixed reference cells before bridge writeback.
#[test]
fn test_core_eval_native_mixed_reference_returns_survive_writeback() {
    let source = r#"<?php
class NativeReturnedReference {
    public function replace(?string &$value, ?string $next): ?string {
        $value = $next;
        return $value;
    }
    public static function read(?string &$value): ?string { return $value; }
}
$source = '$box = new NativeReturnedReference(); $value = "old";
$result = $box->replace($value, "new");
echo $result, ":", $value, "|";
unset($result);
echo $value, "|";
$result = NativeReturnedReference::read($value);
$value = "changed";
echo $result, ":", $value, "|";
$result = $box->replace($value, null);
echo is_null($result), ":", is_null($value);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "new:new|new|new:changed|1:1");
}

/// Repeated native Mixed reference returns balance both the returned owner and caller storage.
#[test]
fn test_core_eval_native_mixed_reference_returns_release_independent_owners() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeReturnedReferenceGC {
            public function replace(?string &$value, ?string $next): ?string {
                $value = $next; return $value;
            }
            public static function read(?string &$value): ?string { return $value; }
         }",
        "$box = new NativeReturnedReferenceGC();",
        "$value = \"old\";
         $result = $box->replace($value, \"new\"); unset($result);
         $result = NativeReturnedReferenceGC::read($value); unset($result);
         $result = $box->replace($value, null); unset($result, $value);",
    );
}

/// Native reference coercions leave caller variables usable after the activation releases its lease.
#[test]
fn test_core_eval_native_reference_coercion_retains_caller_value() {
    let source = r#"<?php
class NativeReferenceSink {
    public function change(string &$value): void { $value = $value . "!"; }
}
$source = '$object = new NativeReferenceSink(); $value = 7;
$object->change($value); unset($object); echo $value; unset($value);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "7!");
}

/// Native exception construction releases argument defaults independently of string-hook execution.
#[test]
fn test_core_eval_native_exception_construction_releases_arguments() {
    assert_core_eval_collection_cleanup(
        "",
        "try { throw new Exception(\"stop\"); } catch (Exception $caught) { unset($caught); }",
    );
}

/// Empty defaults and explicit previous-exception chains release all construction owners.
#[test]
fn test_core_eval_native_exception_defaults_and_previous_release() {
    assert_core_eval_collection_cleanup(
        "",
        "try { throw new Exception(); } catch (Exception $caught) { unset($caught); }
         try { throw new Exception(\"outer\", 7, new Exception(\"cause\")); }
         catch (Exception $caught) { unset($caught); }",
    );
}

/// Native string, Mixed, array, and object properties release previous owners including self-assignment.
#[test]
fn test_core_eval_native_property_replacements_release_displaced_values() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeOwnedSlots { public string $text = \"seed\"; public mixed $value = null;
            public array $items = []; public object $child; }
         class NativeOwnedChild {}",
        "$box = new NativeOwnedSlots();",
        "$box->text = str_repeat(\"x\", 24); $box->text = $box->text;
         $box->value = [\"nested\" => [1, 2]]; $box->value = $box->value;
         $box->items = [3, 4]; $box->items = $box->items;
         $box->child = new NativeOwnedChild(); $box->child = $box->child;",
    );
}

/// Native and eval string hooks preserve conversion order, operands, and exceptions.
#[test]
fn test_core_eval_concat_native_and_dynamic_string_hooks() {
    let source = r#"<?php
class NativeConcatValue {
    public function __toString(): string { echo "N"; return "native"; }
}
$source = 'class EvalConcatValue {
    public function __toString(): string { echo "E"; return "eval"; }
}
class EvalConcatFailure {
    public function __toString(): string { echo "T"; throw new Exception("stop"); }
}
$native = new NativeConcatValue(); $dynamic = new EvalConcatValue();
$joined = $native . $dynamic;
echo ":", $joined, "|";
try { $joined = $dynamic . new EvalConcatFailure(); }
catch (Exception $caught) { echo ":caught|"; }
echo $native . $dynamic;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "NE:nativeeval|ET:caught|NEnativeeval");
}

/// Concatenation keeps scalar cast scratch separate from owned string copies.
#[test]
fn test_core_eval_concat_preserves_borrowed_scalar_cast_results() {
    let source = r#"<?php
$source = '$text = "x"; $number = 23; $flag = true; $nothing = null;
echo $text . $number, ":", $number . $text, ":", $flag . $text,
    ":", $text . $nothing, ":", $text;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "x23:23x:1x:x:x");
}

/// Compound writes and increments release read results, arithmetic cells, and receiver/name leases.
#[test]
fn test_core_eval_property_compound_updates_release_temporary_cells() {
    assert_core_eval_collection_cleanup(
        "$box = new stdClass(); $box->n = 0; $box->text = \"\"; $name = \"n\"; $delta = 3;",
        "$box->n += $delta; $box->{$name} += 2;
         $box->n++; --$box->{$name}; $box->text .= \"x\";",
    );
}

/// Compound property statements evaluate receiver, dynamic name, and RHS once in source order.
#[test]
fn test_core_eval_property_compound_updates_preserve_evaluation_order() {
    let source = r#"<?php
$source = 'function receiver($box) { echo "R"; return $box; }
function member() { echo "N"; return "n"; }
function delta() { echo "V"; return 3; }
$box = new stdClass(); $box->n = 2;
receiver($box)->{member()} += delta();
echo ":", $box->n, "|";
receiver($box)->{member()}++;
echo ":", $box->n; return 42;' . ' // ' . $argc;
echo ":", eval($source);
"#;
    assert_eq!(compile_and_run(source), ":RNV:5|RN:642");
}

/// Indexed, appended, compound, and unset property writes release keys, clones, and read cells.
#[test]
fn test_core_eval_property_array_mutations_release_temporary_cells() {
    assert_core_eval_collection_cleanup(
        "$box = new stdClass(); $box->items = [1]; $name = \"items\"; $key = \"0\";",
        "$box->items[] = \"a\"; $box->{$name}[$key] = 3;
         $box->items[\"label\"] = \"b\"; $box->{$name}[\"label\"] .= \"c\";
         unset($box->items[1]); unset($box->{$name}[\"label\"]);",
    );
}

/// Property array writes preserve borrowed source arrays and evaluate receiver, key, then RHS.
#[test]
fn test_core_eval_property_array_mutations_preserve_cow_and_order() {
    let source = r#"<?php
$source = 'function receiver($box) { echo "R"; return $box; }
function member() { echo "N"; return "items"; }
function itemKey() { echo "K"; return "0"; }
function replacement() { echo "V"; return 2; }
$items = [1]; $box = new stdClass(); $box->items = $items;
$copy = $box->items;
receiver($box)->{member()}[itemKey()] = replacement();
echo ":", $items[0], $copy[0], $box->items[0], "|";
$box->items["label"] = 3;
unset($box->items[0]);
echo count($box->items), ":", $box->items["label"], ":", count($copy);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "RNKV:112|1:3:1");
}

/// COW property arrays keep PHP element references through append, assignment, and unrelated unset.
#[test]
fn test_core_eval_property_array_mutations_preserve_element_references() {
    let source = r#"<?php
$source = '$value = 1; $original = [&$value];
$box = new stdClass(); $box->items = $original;
$box->items[] = 2;
$value = 3; echo $box->items[0], ":";
$box->items[0] = 4; echo $value, ":", $original[0], ":";
unset($box->items[1]);
$value = 5; echo $box->items[0], ":";
unset($box->items[0]);
$value = 6; echo count($box->items);' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "3:4:4:5:0");
}

/// A variable aliased to a property keeps its own owner after writing and destroying the object.
#[test]
fn test_core_eval_property_reference_survives_receiver_release() {
    let source = r#"<?php
$source = 'class OwnedAlias { public $value; }
$box = new OwnedAlias(); $original = "before";
$box->value =& $original; $box->value = "after";
unset($box); echo $original; return 42;' . ' // ' . $argc;
echo eval($source);
"#;
    assert_eq!(compile_and_run(source), "after42");
}

/// Native and eval class-method inventories release temporary names on direct, CUF, and FCC paths.
#[test]
fn test_core_eval_class_method_results_release_temporary_cells() {
    for target in ["\"NativeMethodOwners\"", "\"EvalMethodOwners\""] {
        assert_core_eval_collection_cleanup_with_native(
            "class NativeMethodOwners { public function first(): void {} public function second(): void {} }",
            "class EvalMethodOwners { public function first(): void {} public function second(): void {} }
             $methods = get_class_methods(...);",
            &format!("$result = get_class_methods({target}); unset($result);
                      $result = call_user_func(\"get_class_methods\", {target}); unset($result);
                      $result = $methods({target}); unset($result);"),
        );
    }
}

/// Repeated caught builtin errors release constructor operands as well as the exception object.
#[test]
fn test_core_eval_builtin_error_construction_releases_temporary_cells() {
    assert_core_eval_collection_cleanup(
        "",
        "try { trigger_error(\"invalid\", 2); } catch (ValueError $error) { unset($error); }
         try { get_class_methods(\"MissingMethodOwner\"); } catch (TypeError $error) { unset($error); }
         try { get_class_vars(\"MissingVarsOwner\"); } catch (TypeError $error) { unset($error); }",
    );
}

/// CUF string arguments and returned aliases retire after both successful calls and caught failures.
#[test]
fn test_core_eval_call_user_func_operands_release_after_success_and_throw() {
    assert_core_eval_collection_cleanup(
        "function ownedCufIdentity($value) { return $value; }",
        "$result = call_user_func(\"ownedCufIdentity\", \"temporary\");
         if ($result !== \"temporary\") { echo \"bad\"; } unset($result);
         $result = call_user_func(\"strlen\", \"temporary\"); unset($result);
         try { call_user_func(\"get_class_methods\", \"MissingCufOwner\"); }
         catch (TypeError $error) { unset($error); }",
    );
}

/// Compares deep cleanup after repeated eval results, without allocating loop-control temporaries.
fn assert_core_eval_collection_cleanup(setup: &str, body: &str) {
    assert_core_eval_collection_cleanup_with_native("", setup, body);
}

/// Reflection adapters may return a fallback operand, which must outlive source argument cleanup.
#[test]
fn test_core_eval_reflection_default_result_survives_argument_cleanup() {
    let source = r#"<?php
class NativeMissingStaticValue {}
$source = '$reflection = new ReflectionClass("NativeMissingStaticValue");
$result = $reflection->getStaticPropertyValue("missing", "fallback");
$reuse = "overwritten";
echo $result;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "fallback");
}

/// Measures repeated eval cleanup with native declarations kept outside the opaque source.
pub(super) fn assert_core_eval_collection_cleanup_with_native(native: &str, setup: &str, body: &str) {
    let outstanding = |iterations| {
        let repeated = body.repeat(iterations);
        let source = format!(r#"<?php
{native}
$source = '{setup} {repeated} return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42");
        let (allocations, frees) = parse_gc_stats(&output.stderr);
        (allocations, allocations as i128 - frees as i128)
    };
    let (once_allocated, once_live) = outstanding(1);
    let (repeated_allocated, repeated_live) = outstanding(5);
    assert!(repeated_allocated > once_allocated);
    assert_eq!(repeated_live, once_live, "eval collection retained per-call storage: {body}");
}

/// Repeated typed-property unset releases the old nested payload without releasing it twice.
#[test]
fn test_core_eval_native_typed_unset_releases_nested_property_payloads() {
    assert_core_eval_collection_cleanup_with_native(
        "class NativeUnsetPayload { public mixed $payload; }",
        "class EvalUnsetPayload extends NativeUnsetPayload { public mixed $payload; }
         $box = new EvalUnsetPayload();",
        "$box->payload = [1, [2, 3]]; unset($box->payload); unset($box->payload);",
    );
}

/// Both nested constant categories and their boxed operands are released with the outer result.
#[test]
fn test_core_eval_categorized_constants_release_nested_results() {
    assert_core_eval_collection_cleanup(
        "define(\"GC_USER_PAYLOAD\", [1, 2, 3]); $categorized = true;",
        "$result = get_defined_constants($categorized); unset($result);",
    );
}

/// Direct GC snapshots retire the raw hash beneath the result box.
#[test]
fn test_core_aot_gc_status_box_owns_the_only_hash_reference() {
    assert_gc_status_call_cleanup("gc_status()");
}

/// Returning a GC snapshot from a native function transfers its box without another owner.
#[test]
fn test_core_aot_gc_status_returned_snapshot_is_heap_clean() {
    assert_gc_status_call_cleanup("snapshotCollectorStatus()");
}

/// Positional call_user_func retires every returned collector snapshot.
#[test]
fn test_core_aot_gc_status_call_user_func_snapshot_is_heap_clean() {
    assert_gc_status_call_cleanup("call_user_func(\"gc_status\")");
}

/// Descriptor invocation of a first-class GC callable balances its result ownership.
#[test]
fn test_core_aot_gc_status_first_class_snapshot_is_heap_clean() {
    assert_gc_status_call_cleanup("invokeCollectorStatus(gc_status(...))");
}

/// Isolates each GC call surface so one ownership failure cannot mask another.
fn assert_gc_status_call_cleanup(call: &str) {
    let source = format!(r#"<?php
function snapshotCollectorStatus(): mixed {{ return gc_status(); }}
function invokeCollectorStatus(callable $callback): mixed {{ return $callback(); }}
for ($i = 0; $i < 3; $i++) {{
    $status = {call};
    echo count($status), "|";
    unset($status);
}}
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "12|".repeat(3), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Declaration inventories, frame inventories, and GC status free their nested temporary cells.
#[test]
fn test_core_eval_metadata_collections_release_each_result() {
    assert_core_eval_collection_cleanup(
        "$core = \"core\";",
        "$result = get_defined_functions(); unset($result);
         $result = get_extension_funcs($core); unset($result);
         $result = get_included_files(); unset($result);
         $result = get_defined_vars(); unset($result);
         $result = gc_status(); unset($result);
         $result = debug_backtrace(); unset($result);",
    );
}

/// Repeated string-byte reads preserve their length and release no borrowed input storage.
#[test]
fn test_core_eval_extension_name_byte_views_do_not_allocate() {
    assert_core_eval_collection_cleanup(
        "$core = \"CoRe\";",
        "$result = get_extension_funcs($core); unset($result);",
    );
}

/// Native byte-view ABI results preserve the string length and leave caller variables readable.
#[test]
fn test_core_eval_extension_name_byte_views_preserve_contents() {
    let source = r#"<?php
$source = '$core = "CoRe"; $first = get_extension_funcs($core);
$second = get_extension_funcs($core); return count($first) + count($second);' . ' // ' . $argc;
echo eval($source);
"#;
    assert_eq!(compile_and_run(source), "118");
}

/// Explicit numeric options release conversion cells even when the input variables are borrowed.
#[test]
fn test_core_eval_numeric_options_release_conversion_cells() {
    assert_core_eval_collection_cleanup(
        "$mask = 0; $options = 2; $limit = 1;",
        "$result = error_reporting($mask); unset($result);
         $result = debug_backtrace($options, $limit); unset($result);",
    );
}

/// Literal, cast, comparison, and branch-produced Core arguments release their temporary owners.
#[test]
fn test_core_eval_scalar_argument_expressions_release_temporaries() {
    assert_core_eval_collection_cleanup(
        "$flag = false; $nothing = null;",
        "get_defined_constants(false);
         get_defined_constants((bool) 0);
         get_defined_constants(1 > 2);
         get_defined_constants(true ? $flag : true);
         get_defined_constants(false ?: $flag);
         get_defined_constants($nothing ?? false);
         get_defined_constants(match (1) { 1 => $flag, default => true });",
    );
}

/// Repeated assignment to the same ordinary and referenced cells leaves exactly one scope owner.
#[test]
fn test_core_eval_same_cell_assignments_release_previous_leases() {
    assert_core_eval_collection_cleanup(
        "$value = \"keep\"; $alias =& $value; $plain = \"plain\";",
        "$value = $value; $alias = $alias; $plain = $plain;
         $result = get_defined_vars(); unset($result);",
    );
}

/// Nested literal defaults release keys and values without consuming persistent constants.
#[test]
fn test_core_eval_class_constant_defaults_release_inventory_leases() {
    assert_core_eval_collection_cleanup(
        "class GcDefaultConstant {
            const TOKEN = \"keep\";
            public string $value = self::TOKEN;
            public array $items = [1, \"nested\" => [self::TOKEN]];
        }",
        "$result = get_class_vars(\"GcDefaultConstant\"); unset($result);",
    );
}

/// Defaults referring to scalar, nested-array, and enum constants survive freeing both inventories.
#[test]
fn test_core_eval_class_constant_defaults_remain_readable() {
    let source = r#"<?php
$source = 'enum SharedDefaultCase { case Ready; }
class SharedDefaults {
    const TOKEN = "keep";
    const ITEMS = [7, 8];
    public string $value = self::TOKEN;
    public array $items = self::ITEMS;
    public $case = SharedDefaultCase::Ready;
}
$first = get_class_vars("SharedDefaults"); unset($first);
$second = get_class_vars("SharedDefaults"); unset($second);
echo SharedDefaults::TOKEN, ":", SharedDefaults::ITEMS[0], ":", SharedDefaultCase::Ready->name;
return 42;' . ' // ' . $argc;
echo ":", eval($source);
"#;
    assert_eq!(compile_and_run(source), ":keep:7:Ready42");
}

/// First-use and cached builtin enum cases stay alive after boolean conversion and condition cleanup.
#[test]
fn test_core_eval_property_hook_case_survives_temporary_conditions() {
    let source = r#"<?php
$source = '$first = (bool) PropertyHookType::Get; unset($first);
if (PropertyHookType::Get) { echo "live:"; }
echo PropertyHookType::Get->name;
return 42;' . ' // ' . $argc;
echo eval($source);
"#;
    assert_eq!(compile_and_run(source), "live:Get42");
}

/// Repeating opaque eval constant inventories leaves no extra live allocations after cleanup.
#[test]
fn test_core_eval_flat_constant_inventory_releases_each_result() {
    let outstanding = |iterations: usize| {
        let source = format!(r#"<?php
$source = 'define("INVENTORY_PAYLOAD", [1, 2, 3]);
for ($iteration = 0; $iteration < {iterations}; $iteration++) {{
    $flat = get_defined_constants();
    unset($flat);
    $flat = get_defined_constants(false);
    unset($flat);
}}
return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42");
        let (allocations, frees) = parse_gc_stats(&output.stderr);
        assert!(allocations > 0, "fixture must exercise the runtime heap");
        (allocations, allocations as i128 - frees as i128)
    };
    let (once_allocated, once_live) = outstanding(1);
    let (repeated_allocated, repeated_live) = outstanding(5);
    assert!(repeated_allocated > once_allocated, "repeat must actually materialize more inventories");
    assert_eq!(repeated_live, once_live, "flat constant inventories leaked per-call storage");
}

/// Verifies GC controls and the PHP 8 status schema across callable forms.
#[test]
fn test_core_gc_controls_and_status_schema() {
    let out = compile_and_run(
        r#"<?php
namespace Demo;
echo \GC_ENABLED() ? "on" : "bad"; echo ":";
gc_disable();
echo call_user_func("gc_enabled") ? "bad" : "off"; echo ":";
$enable = gc_enable(...);
$enable();
echo gc_enabled() ? "on" : "bad"; echo ":";
$status = call_user_func("gc_status");
echo count($status); echo ":";
echo is_bool($status["running"]) && is_bool($status["protected"]) && is_bool($status["full"]) ? "bool" : "bad"; echo ":";
echo is_int($status["runs"]) && is_int($status["collected"]) && is_int($status["roots"]) ? "int" : "bad"; echo ":";
echo $status["threshold"] . ":" . $status["buffer_size"] . ":";
echo is_float($status["application_time"]) && is_float($status["collector_time"]) && is_float($status["destructor_time"]) && is_float($status["free_time"]) ? "float" : "bad"; echo ":";
echo $status["application_time"] >= 0.0 && $status["collector_time"] >= 0.0 && $status["destructor_time"] >= 0.0 && $status["free_time"] >= 0.0 ? "nonnegative" : "bad"; echo ":";
echo gc_mem_caches() >= 0 ? "cache" : "bad";
"#,
    );
    assert_eq!(out, "on:off:on:12:bool:int:0:0:float:nonnegative:cache");
}

/// Verifies explicit collection reclaims a cycle while automatic collection is disabled.
#[test]
fn test_core_gc_explicit_collection_bypasses_disable() {
    let out = compile_and_run(
        r#"<?php
class CoreGcNode { public $next = null; }
gc_disable();
$node = new CoreGcNode();
$node->next = $node;
unset($node);
$collected = gc_collect_cycles();
$status = gc_status();
echo $collected > 0 ? "collected" : "bad"; echo ":";
echo $status["runs"] > 0 ? "ran" : "bad"; echo ":";
echo $status["collected"] >= $collected ? "counted" : "bad"; echo ":";
echo gc_enabled() ? "bad" : "disabled";
gc_enable();
"#,
    );
    assert_eq!(out, "collected:ran:counted:disabled");
}

/// Verifies collector roots and phase clocks report live runtime work.
#[test]
fn test_core_gc_status_reports_live_roots_and_timings() {
    let out = compile_and_run(
        r#"<?php
class TimedGcNode {
    public $next = null;
    public function __destruct() { usleep(2000); }
}
$node = new TimedGcNode();
$node->next = $node;
$before = gc_status();
unset($node);
gc_collect_cycles();
$after = gc_status();
echo $before["roots"] > 0 ? "roots" : "bad"; echo ":";
echo $after["application_time"] >= 0.0 ? "app" : "bad"; echo ":";
echo $after["collector_time"] > 0.0 ? "collector" : "bad"; echo ":";
echo $after["destructor_time"] > 0.0 ? "destructor" : "bad"; echo ":";
echo $after["free_time"] > 0.0 ? "free" : "bad";
"#,
    );
    assert_eq!(out, "roots:app:collector:destructor:free");
}

/// Verifies cache reclamation drains small bins once and leaves reusable heap state valid.
#[test]
fn test_core_gc_mem_caches_drains_small_bins_once() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$first = str_repeat("a", 8);
$guard = str_repeat("b", 8);
unset($first);
$released = gc_mem_caches();
$again = gc_mem_caches();
$reuse = str_repeat("c", 8);
echo $released > 0 ? "released" : "bad"; echo ":";
echo $again === 0 ? "empty" : "bad"; echo ":";
echo $guard . $reuse;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "released:empty:bbbbbbbbcccccccc");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "cache drain corrupted allocator state: {}",
        out.stderr
    );
}
