//! Purpose:
//! Integration or regression tests for diagnostic coverage of extensions, including packed class rejects non pod field, buffer new rejects non pod element type, and buffer new rejects union element type.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

/// Verifies every PHP 8.5 DOM list/map dimension handler crosses the checker boundary.
#[test]
fn dom_collection_dimension_reads_pass_checker() {
    expect_no_error(
        r#"<?php
function legacy(DOMNodeList $nodes, DOMNamedNodeMap $attributes): void {
    var_dump($nodes[0]);
    var_dump($attributes[0]);
    var_dump($attributes['id']);
}
function modern(
    Dom\NodeList $nodes,
    Dom\HTMLCollection $elements,
    Dom\NamedNodeMap $attributes,
    Dom\DtdNamedNodeMap $declarations,
): void {
    var_dump($nodes[0]);
    var_dump($elements[0]);
    var_dump($elements['id']);
    var_dump($attributes[0]);
    var_dump($attributes['id']);
    var_dump($declarations[0]);
    var_dump($declarations['entity']);
}
"#,
    );
}

/// Verifies PHPT 007's direct root and selected-property dimension writes pass the
/// checker while the loader still carries its literal-false failure alternative.
#[test]
fn simplexml_phpt_007_dimension_writes_pass_checker() {
    expect_no_error(
        r#"<?php
$sxe = simplexml_load_string('<sxe id="elem1"><elem1 attr1="first"/></sxe>');
$sxe['id'] = 'Changed1';
$sxe->elem1['attr1'] = 12;
"#,
    );
}

/// Verifies PHPT 016's property dimension and nested numeric-selection writes both
/// cross the checker boundary without manually narrowing loader failure.
#[test]
fn simplexml_phpt_016_nested_dimension_writes_pass_checker() {
    expect_no_error(
        r#"<?php
$people = simplexml_load_string('<people><person name="Joe"/></people>');
$people->person['name'] = 'JoeFoo';
$people->person[0]['name'] = 'JoeFooBar';
"#,
    );
}

/// Verifies PHPT 028's write through a missing selected property passes the checker;
/// runtime autovivification remains a separate implementation gate.
#[test]
fn simplexml_phpt_028_missing_property_dimension_write_passes_checker() {
    expect_no_error(
        r#"<?php
$people = simplexml_load_string('<people/>');
$people->person['name'] = 'John';
"#,
    );
}

/// Verifies PHPT 034 can replace a selected SimpleXML wrapper with its PHP array cast.
#[test]
fn simplexml_phpt_034_object_to_array_reassignment_passes_checker() {
    expect_no_error(
        r#"<?php
$foo = simplexml_load_string('<foo><bar><p>one</p><p>two</p><p>three</p></bar></foo>');
$p = $foo->bar->p;
$p = (array) $foo->bar->p;
echo count($p);
"#,
    );
}

/// Verifies XPath's `array|null|false` result remains admissible to `count()`.
#[test]
fn simplexml_xpath_fallible_array_count_passes_checker() {
    expect_no_error(
        r#"<?php
$xml = simplexml_load_string('<root><child/></root>');
$nodes = $xml->xpath('/root/child');
echo count($nodes);
"#,
    );
}

/// Verifies the XPath exception does not admit unrelated scalar union arms.
#[test]
fn simplexml_xpath_count_does_not_widen_unrelated_unions() {
    expect_error(
        r#"<?php
function count_nodes(array|bool|null $nodes): int {
    return count($nodes);
}
"#,
        "count() argument must be array or Countable object",
    );
}

/// Verifies a union containing broad `bool` does not inherit the strict SimpleXML
/// dimension-write exemption from its object member.
#[test]
fn simplexml_dimension_write_rejects_bool_union() {
    expect_error(
        r#"<?php
function write_name(SimpleXMLElement|bool $xml): void {
    $xml->person['name'] = 'John';
}
"#,
        "Array index assignment requires an object or typed pointer",
    );
}

/// Verifies a union containing a second object class does not inherit the strict
/// SimpleXML dimension-write exemption from one eligible member.
#[test]
fn simplexml_dimension_write_rejects_multiple_object_union() {
    expect_error(
        r#"<?php
function write_name(SimpleXMLElement|stdClass|false $xml): void {
    $xml->person['name'] = 'John';
}
"#,
        "Array index assignment requires an object or typed pointer",
    );
}

/// Verifies bug55098's untyped closure stays checker-admissible for every
/// SimpleXML handler operation; runtime routing is covered by the codegen fixture.
#[test]
fn simplexml_bug55098_untyped_closure_handlers_pass_checker() {
    expect_no_error(
        r#"<?php
$xml = simplexml_load_string('<root><a><b>1</b><b>2</b><b>3</b></a></root>');
$nodes = $xml->a->b;
$callback = function ($n): void {
    $n->asXml();
    $n->attributes();
    $n->children();
    $n->getNamespaces();
    $n->xpath('/root/a/b');
    $n->addAttribute('attr', 'value');
    (bool) $n['attr'];
    $n->addChild('child', 'value');
    $n->outer[]->inner = 'foo';
    (bool) $n->outer;
    (bool) $n;
    isset($n->outer);
    isset($n['attr']);
    unset($n->outer);
    unset($n['attr']);
    unset($n->child);
};
$callback($nodes);
"#,
    );
}

/// Verifies the untyped-closure change does not erase explicit scalar constraints:
/// a typed integer parameter is still not an indexable PHP value.
#[test]
fn simplexml_untyped_closure_support_does_not_widen_explicit_scalar_parameters() {
    expect_error(
        r#"<?php
$callback = function (int $value): mixed {
    return $value['name'];
};
"#,
        "Cannot index non-array",
    );
}

/// Verifies callback-specific parameter hints remain authoritative instead of being
/// replaced by the default Mixed type adopted for a genuinely untyped closure.
#[test]
fn simplexml_untyped_closure_support_preserves_contextual_callback_hints() {
    expect_error(
        r#"<?php
array_map(function ($value): mixed {
    return $value['name'];
}, [1, 2, 3]);
"#,
        "Cannot index non-array",
    );
}

/// Verifies a SimpleXML debug override cannot declare a scalar return type;
/// php-src permits only `?array` when that return type is explicit.
#[test]
fn simplexml_debug_info_rejects_declared_scalar_return_type() {
    expect_error(
        r#"<?php
class InvalidDebugXml extends SimpleXMLElement {
    public function __debugInfo(): int { return 1; }
}
"#,
        "InvalidDebugXml::__debugInfo(): Return type must be ?array when declared",
    );
}

/// Verifies `ReturnTypeWillChange` applies to SimpleXML's tentative iterator
/// methods only and cannot waive the explicit `?array` debug-info contract.
#[test]
fn simplexml_debug_info_return_type_will_change_does_not_bypass_array_contract() {
    expect_error(
        r#"<?php
class InvalidAttributedDebugXml extends SimpleXMLElement {
    #[\ReturnTypeWillChange]
    public function __debugInfo(): int { return 1; }
}
"#,
        "InvalidAttributedDebugXml::__debugInfo(): Return type must be ?array when declared",
    );
}

/// Verifies that a packed class with a non-POD field (string) is rejected with a specific error message.
#[test]
fn test_error_packed_class_rejects_non_pod_field() {
    expect_error(
        "<?php packed class Bad { public string $name; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

/// Verifies that buffer_new<T> rejects non-POD element types (string).
#[test]
fn test_error_buffer_new_rejects_non_pod_element_type() {
    expect_error(
        "<?php buffer<string> $names = buffer_new<string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

/// Verifies that buffer_new<T> rejects union element types (int|string).
#[test]
fn test_error_buffer_new_rejects_union_element_type() {
    expect_error(
        "<?php buffer<int|string> $values = buffer_new<int|string>(2);",
        "buffer<T> requires a POD scalar, pointer, or packed class element type",
    );
}

/// Verifies that a packed class with a nullable field (?int) is rejected.
#[test]
fn test_error_packed_class_rejects_nullable_field() {
    expect_error(
        "<?php packed class MaybePoint { public ?int $x; }",
        "Packed class fields must use POD scalars, pointers, or packed classes",
    );
}

/// Verifies that assigning a non-buffer element type (bool) to an int buffer element is rejected.
#[test]
fn test_error_buffer_scalar_assign_type_mismatch() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); $values[0] = true;",
        "Buffer element type mismatch",
    );
}

/// Verifies that a statically known string cannot be used to read a buffer,
/// even though a boxed Mixed index is converted to int at runtime.
#[test]
fn test_error_buffer_read_rejects_static_string_index() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); string $index = \"0\"; echo $values[$index];",
        "Buffer index must be integer",
    );
}

/// Verifies that a statically known string cannot be used to write a buffer,
/// preserving the checker boundary around the Mixed runtime conversion.
#[test]
fn test_error_buffer_write_rejects_static_string_index() {
    expect_error(
        "<?php buffer<int> $values = buffer_new<int>(2); string $index = \"0\"; $values[$index] = 1;",
        "Buffer index must be integer",
    );
}

/// Verifies that packed buffer elements cannot be assigned directly; must use field access.
#[test]
fn test_error_buffer_packed_element_requires_field_assignment() {
    expect_error(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); $points[0] = 1;",
        "Assign packed buffer elements through field access like $buf[$i]->field",
    );
}

/// Verifies that buffer_len rejects a non-buffer argument (int).
#[test]
fn test_error_buffer_len_requires_buffer_argument() {
    expect_error(
        "<?php echo buffer_len(1);",
        "buffer_len() argument must be buffer<T>",
    );
}

/// Verifies that buffer_free rejects a non-buffer argument (int).
#[test]
fn test_error_buffer_free_requires_buffer_argument() {
    expect_error(
        "<?php buffer_free(42);",
        "buffer_free() argument must be buffer<T>",
    );
}

/// Verifies that buffer_free rejects calls with more than one argument.
#[test]
fn test_error_buffer_free_wrong_arg_count() {
    expect_error(
        "<?php buffer<int> $b = buffer_new<int>(1); buffer_free($b, $b);",
        "buffer_free() takes exactly 1 argument",
    );
}

/// Verifies that buffer_free rejects calls with a temporary buffer_new result instead of a local variable.
#[test]
fn test_error_buffer_free_requires_local_variable() {
    expect_error(
        "<?php buffer_free(buffer_new<int>(1));",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is passed as a reference parameter.
#[test]
fn test_error_buffer_free_rejects_ref_param() {
    expect_error(
        "<?php function drop(&$buf) { buffer_free($buf); } buffer<int> $buf = buffer_new<int>(1); drop($buf);",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is accessed via a global alias inside a function.
#[test]
fn test_error_buffer_free_rejects_global_alias() {
    expect_error(
        "<?php buffer<int> $buf = buffer_new<int>(1); function drop() { global $buf; buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that buffer_free rejects when the buffer is stored in a static variable inside a function.
#[test]
fn test_error_buffer_free_rejects_static_slot() {
    expect_error(
        "<?php function drop() { static $buf = buffer_new<int>(1); buffer_free($buf); } drop();",
        "buffer_free() argument must be a local variable",
    );
}

/// Verifies that extern function parameters with unknown C types (badtype) are rejected.
#[test]
fn test_error_extern_unknown_type() {
    expect_error(
        "<?php extern function foo(badtype $x): int;",
        "Unknown C type: badtype",
    );
}

/// Verifies that an empty extern block is rejected.
#[test]
fn test_error_extern_block_empty() {
    expect_error("<?php extern \"lib\" { }", "Empty extern block");
}

/// Verifies that calling an extern function with too few arguments is rejected.
#[test]
fn test_error_extern_wrong_arg_count() {
    expect_error(
        "<?php extern function abs(int $n): int; abs();",
        "Extern function 'abs' expects 1 arguments, got 0",
    );
}

/// Verifies that calling an extern function with a mismatched argument type (int instead of string) is rejected.
#[test]
fn test_error_extern_wrong_arg_type() {
    expect_error(
        "<?php extern function strlen(string $s): int; strlen(123);",
        "Extern function 'strlen' parameter $s expects Str, got Int",
    );
}

/// Verifies that declaring the same extern function twice is rejected.
#[test]
fn test_error_duplicate_extern_function() {
    expect_error(
        "<?php extern function foo(int $x): int; extern function foo(int $y): int;",
        "Duplicate function declaration: foo",
    );
}

/// Verifies that extern global declarations that would shadow PHP superglobals ($argc, $argv, etc.) are rejected.
#[test]
fn test_error_extern_global_reserved_name() {
    expect_error(
        "<?php extern global int $argc;",
        "extern global $argc would shadow a reserved superglobal",
    );
}

/// Verifies that extern global declarations with void type are rejected.
#[test]
fn test_error_extern_global_void_type() {
    expect_error(
        "<?php extern global void $bad;",
        "Extern global $bad uses an unsupported type",
    );
}

/// Verifies extern callback string variables are rejected when they are not callable descriptors.
#[test]
fn test_error_extern_callable_requires_literal_function_name() {
    // Verifies that passing a variable string as an extern callback is rejected because it is not a callable descriptor.
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function on_signal($sig) {} $fn = \"on_signal\"; signal(15, $fn);",
        "expects a string literal naming a user function or a callable value",
    );
}

/// Verifies that passing an undefined function name to an extern callable function is rejected.
#[test]
fn test_error_extern_callable_requires_defined_function() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; signal(15, \"missing_handler\");",
        "Undefined callback function: missing_handler",
    );
}

/// Verifies that an extern callable callback with a non-C-compatible return type (string) is rejected.
#[test]
fn test_error_extern_callable_requires_c_compatible_return_type() {
    expect_error(
        "<?php extern function signal(int $sig, callable $handler): ptr; function bad_handler($sig) { return \"oops\"; } signal(15, \"bad_handler\");",
        "unsupported return type",
    );
}

/// Verifies that extern class fields with void type are rejected.
#[test]
fn test_error_extern_class_void_field() {
    expect_error(
        "<?php extern class Bad { void $field; }",
        "Extern class 'Bad' field $field uses an unsupported type",
    );
}
