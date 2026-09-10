//! Purpose:
//! Checks temporary callable and return-value ownership around native descriptor invocation.
//!
//! Called from:
//! - The runtime-GC argument guard tests.
//!
//! Key details:
//! - Factory-created closures own destructor-bearing captures without another caller owner.
//! - Argument evaluation, callback execution, and capture cleanup can fail independently.
//! - Exact destructor ordering and clean heaps are required for every invocation shape.

use super::*;

/// Consumes temporary callbacks on success, argument failure, and body failure across invocation forms.
#[test]
fn test_mbstring_descriptor_temporary_callback_ownership() {
    for (success, argument_failure, body_failure) in [
        ("make_descriptor(\"success\", false)(4)",
         "make_descriptor(\"argument\", false)(rejected_argument())",
         "make_descriptor(\"body\", true)(4)"),
        ("make_descriptor(\"success\", false)(value: 4)",
         "make_descriptor(\"argument\", false)(value: rejected_argument())",
         "make_descriptor(\"body\", true)(value: 4)"),
        ("call_user_func(make_descriptor(\"success\", false), 4)",
         "call_user_func(make_descriptor(\"argument\", false), rejected_argument())",
         "call_user_func(make_descriptor(\"body\", true), 4)"),
        ("call_user_func_array(make_descriptor(\"success\", false), [4])",
         "call_user_func_array(make_descriptor(\"argument\", false), rejected_arguments())",
         "call_user_func_array(make_descriptor(\"body\", true), [4])"),
    ] {
        let source = format!(r#"<?php
class DescriptorCaptureValue {{
    public function __construct(public string $name) {{}}
    public function __destruct() {{ echo "release:", $this->name, "\n"; }}
}}
function make_descriptor(string $name, bool $fail): Closure {{
    $held = new DescriptorCaptureValue($name);
    return function(int $value) use ($held, $fail): int {{
        echo "body:", $held->name, "\n";
        if ($fail) {{ throw new RuntimeException("body"); }}
        return $value + mb_strlen("é");
    }};
}}
function rejected_argument(): int {{ echo "argument\n"; throw new RuntimeException("argument"); }}
function rejected_arguments(): array {{ echo "argument\n"; throw new RuntimeException("argument"); }}
for ($i = 0; $i < 8; $i++) {{
    echo "value:", {success}, "\n";
    try {{ {argument_failure}; }} catch (Throwable $error) {{ echo "caught:", $error->getMessage(), "\n"; }}
    try {{ {body_failure}; }} catch (Throwable $error) {{ echo "caught:", $error->getMessage(), "\n"; }}
}}
echo "done\n";
"#);
        let expected = "value:body:success\nrelease:success\n5\nargument\nrelease:argument\ncaught:argument\nbody:body\nrelease:body\ncaught:body\n";
        assert_clean(&source, &format!("{}done\n", expected.repeat(8)));
    }
}

/// Releases an undelivered result when destroying the temporary callback throws after its body returns.
#[test]
fn test_mbstring_descriptor_result_cleanup_after_callback_throw() {
    let source = r#"<?php
class ThrowingDescriptorCapture {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "capture\n";
        if ($this->fail) { throw new RuntimeException("capture"); }
    }
}
class ThrowingDescriptorResult {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "result\n";
        if ($this->fail) { throw new RuntimeException("result"); }
    }
}
function make_result_descriptor(bool $capture_fail, bool $result_fail): Closure {
    $held = new ThrowingDescriptorCapture($capture_fail);
    return function() use ($held, $result_fail): ThrowingDescriptorResult {
        echo "body:", $held->fail ? "throw" : "keep", "\n";
        return new ThrowingDescriptorResult($result_fail);
    };
}
function check_result(bool $capture_fail, bool $result_fail): void {
    try {
        $result = make_result_descriptor($capture_fail, $result_fail)();
        echo "stored\n";
        unset($result);
    } catch (Throwable $error) {
        echo "caught:", $error->getMessage(), "\n";
        $previous = $error->getPrevious();
        if ($previous !== null) { echo "previous:", $previous->getMessage(), "\n"; }
    }
}
for ($i = 0; $i < 8; $i++) {
    check_result(false, false); check_result(false, true);
    check_result(true, false); check_result(true, true);
}
echo "done\n";
"#;
    let expected = "body:keep\ncapture\nstored\nresult\nbody:keep\ncapture\nstored\nresult\ncaught:result\nbody:throw\ncapture\nresult\ncaught:capture\nbody:throw\ncapture\nresult\ncaught:result\nprevious:capture\n";
    assert_clean(source, &format!("{}done\n", expected.repeat(8)));
}

/// Protects an owned object result while releasing a temporary argument whose destructor can throw.
#[test]
fn test_mbstring_descriptor_result_cleanup_after_argument_throw() {
    for call in [
        "result_after_argument(new ResultArgument($argument_fail), $result_fail)",
        "call_user_func(\"result_after_argument\", new ResultArgument($argument_fail), $result_fail)",
        "call_user_func_array(\"result_after_argument\", [new ResultArgument($argument_fail), $result_fail])",
        "result_callback()(new ResultArgument($argument_fail), $result_fail)",
        "result_callback()(held: new ResultArgument($argument_fail), fail: $result_fail)",
    ] {
        let source = format!(r#"<?php
class ResultArgument {{
    public function __construct(public bool $fail) {{}}
    public function __destruct() {{
        echo "argument\n";
        if ($this->fail) {{ throw new RuntimeException("argument"); }}
    }}
}}
class ArgumentCallResult {{
    public function __construct(public bool $fail) {{}}
    public function __destruct() {{
        echo "result\n";
        if ($this->fail) {{ throw new RuntimeException("result"); }}
    }}
}}
function result_after_argument(ResultArgument $held, bool $fail): ArgumentCallResult {{
    echo "body\n";
    return new ArgumentCallResult($fail);
}}
function result_callback(): Closure {{ return result_after_argument(...); }}
function check_argument_result(bool $argument_fail, bool $result_fail): void {{
    try {{
        $result = {call};
        echo "stored\n";
        unset($result);
    }} catch (Throwable $error) {{
        echo "caught:", $error->getMessage(), "\n";
        $previous = $error->getPrevious();
        if ($previous !== null) {{ echo "previous:", $previous->getMessage(), "\n"; }}
    }}
}}
for ($i = 0; $i < 8; $i++) {{
    check_argument_result(false, false); check_argument_result(false, true);
    check_argument_result(true, false); check_argument_result(true, true);
}}
echo "done\n";
"#);
        let expected = "body\nargument\nstored\nresult\nbody\nargument\nstored\nresult\ncaught:result\nbody\nargument\nresult\ncaught:argument\nbody\nargument\nresult\ncaught:result\nprevious:argument\n";
        assert_clean(&source, &format!("{}done\n", expected.repeat(8)));
    }
}

/// Preserves caller and captured objects returned through borrowed native parameter slots.
#[test]
fn test_mbstring_descriptor_borrowed_object_return_ownership() {
    let source = r#"<?php
class BorrowedDescriptorResult {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destroy:", $this->name, "\n"; }
}
function borrowed_parameter_descriptor(): Closure {
    return function(BorrowedDescriptorResult $value): BorrowedDescriptorResult { return $value; };
}
function borrowed_capture_descriptor(): Closure {
    $held = new BorrowedDescriptorResult("capture");
    return function() use ($held): BorrowedDescriptorResult { return $held; };
}
for ($i = 0; $i < 8; $i++) {
    $original = new BorrowedDescriptorResult("caller");
    $copy = borrowed_parameter_descriptor()($original);
    echo $copy->name, "\n";
    unset($copy);
    echo "still:", $original->name, "\n";
    unset($original);
    $copy = borrowed_capture_descriptor()();
    echo $copy->name, "\n";
    unset($copy);
}
echo "done\n";
"#;
    let expected = "caller\nstill:caller\ndestroy:caller\ncapture\ndestroy:capture\n";
    assert_clean(source, &format!("{}done\n", expected.repeat(8)));
}

/// Keeps borrowed arguments alive and releases temporary arguments independently of object results.
#[test]
fn test_mbstring_descriptor_typed_object_argument_returns() {
    for (borrowed, temporary) in [
        ("object_identity($original)", "object_identity(new ReturnedObject(\"temporary\"))"),
        ("(object_identity(...))($original)", "(object_identity(...))(new ReturnedObject(\"temporary\"))"),
        ("object_callback()($original)", "object_callback()(new ReturnedObject(\"temporary\"))"),
        ("call_user_func(\"object_identity\", $original)",
         "call_user_func(\"object_identity\", new ReturnedObject(\"temporary\"))"),
        ("call_user_func_array(\"object_identity\", [$original])",
         "call_user_func_array(\"object_identity\", [new ReturnedObject(\"temporary\")])"),
        ("$original->pass($original)", "$original->pass(new ReturnedObject(\"temporary\"))"),
        ("ReturnedObject::static_pass($original)",
         "ReturnedObject::static_pass(new ReturnedObject(\"temporary\"))"),
    ] {
        let source = format!(r#"<?php
class ReturnedObject {{
    public function __construct(public string $name) {{}}
    public function __destruct() {{ echo "destroy:", $this->name, "\n"; }}
    public function pass(ReturnedObject $value): ReturnedObject {{ return $value; }}
    public static function static_pass(ReturnedObject $value): ReturnedObject {{ return $value; }}
}}
function object_identity(ReturnedObject $value): ReturnedObject {{ return $value; }}
function object_callback(): Closure {{ return object_identity(...); }}
for ($i = 0; $i < 8; $i++) {{
    $original = new ReturnedObject("caller");
    $copy = {borrowed};
    echo "borrowed:", $copy->name, "\n";
    unset($copy);
    echo "alive:", $original->name, "\n";
    $copy = {temporary};
    echo "temporary:", $copy->name, "\n";
    unset($copy);
    unset($original);
}}
echo "done\n";
"#);
        let expected = "borrowed:caller\nalive:caller\ntemporary:temporary\ndestroy:temporary\ndestroy:caller\n";
        assert_clean(&source, &format!("{}done\n", expected.repeat(8)));
    }
}

/// Balances local, widened, forwarded, conditional, receiver, and by-reference parameter object returns.
#[test]
fn test_mbstring_descriptor_typed_object_return_sources() {
    let source = r#"<?php
class SourceObject {
    public function __construct(public string $name) {}
    public function __destruct() { echo "destroy:", $this->name, "\n"; }
    public function identity(): SourceObject { return $this; }
}
function source_local(): SourceObject {
    $value = new SourceObject("local");
    return $value;
}
function source_widened(bool $change): SourceObject {
    $value = new SourceObject("widened");
    if ($change) { $saved = $value; $value = 7; return $saved; }
    return $value;
}
function source_select(SourceObject $value, bool $fresh): SourceObject {
    if ($fresh) { return new SourceObject("fresh"); }
    return $value;
}
function source_forward(SourceObject $value, bool $fresh): SourceObject {
    return source_select($value, $fresh);
}
function source_reference(SourceObject &$value): SourceObject { return $value; }
for ($i = 0; $i < 8; $i++) {
    $copy = (source_local(...))();
    echo $copy->name, "\n"; unset($copy);
    $copy = (source_widened(...))(false);
    echo $copy->name, "\n"; unset($copy);
    $copy = (source_widened(...))(true);
    echo $copy->name, "\n"; unset($copy);
    $copy = (source_forward(...))(new SourceObject("argument"), false);
    echo $copy->name, "\n"; unset($copy);
    $copy = (source_forward(...))(new SourceObject("argument"), true);
    echo $copy->name, "\n"; unset($copy);
    $original = new SourceObject("reference");
    $copy = source_reference($original);
    echo $copy->name, "\n"; unset($copy);
    $copy = $original->identity();
    echo $copy->name, "\n"; unset($copy);
    echo "alive:", $original->name, "\n";
    unset($original);
}
echo "done\n";
"#;
    let expected = "local\ndestroy:local\nwidened\ndestroy:widened\nwidened\ndestroy:widened\nargument\ndestroy:argument\ndestroy:argument\nfresh\ndestroy:fresh\nreference\nreference\nalive:reference\ndestroy:reference\n";
    assert_clean(source, &format!("{}done\n", expected.repeat(8)));
}

/// Requires a complete native execution, exact observable lifecycle order, and no residual heap owners.
fn assert_clean(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}\n{source}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
