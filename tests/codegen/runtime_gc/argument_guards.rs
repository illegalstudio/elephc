//! Purpose:
//! Exercises EIR captured-argument ownership across normal calls and PHP exception unwinding.
//!
//! Called from:
//! - The focused codegen runtime-GC suite.
//!
//! Key details:
//! - Mbstring's shared argument strategy requests the generic owned-value guards.
//! - Repetition isolates retained owners from stable program-lifetime allocations.
//! - Nested catches must preserve guards established before their handler was installed.

use crate::support::*;

#[path = "argument_guards/callable_temporaries.rs"]
mod callable_temporaries;

/// Releases fresh closure descriptors and their captures on both pre-entry and validation failures.
#[test]
fn test_mbstring_argument_guards_callable_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let source = format!(r#"<?php
function rejected_callable_encoding(): string {{ throw new RuntimeException("before entry"); }}
$payload = str_repeat("x", 64);
for ($i = 0; $i < {count}; $i++) {{
    $callback = fn(): string => $payload;
    try {{ mb_strlen($callback); }} catch (TypeError) {{}}
    try {{ mb_strlen($callback, rejected_callable_encoding()); }} catch (RuntimeException) {{}}
}}
echo strlen($callback());
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "64");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "exception guards retained closure descriptors or captures");
}

/// Balances scalar, Mixed, array, named, and function-scoped captures when a later argument throws.
#[test]
fn test_mbstring_argument_guards_before_entry() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = r#"
try { mb_strlen($subject, stopped_mb_argument()); } catch (RuntimeException) {}
try { mb_check_encoding($items, stopped_mb_argument()); } catch (RuntimeException) {}
try { mb_strlen($mixed, stopped_mb_argument()); } catch (RuntimeException) {}
try { mb_strlen(encoding: $encoding, string: stopped_mb_argument()); } catch (RuntimeException) {}
try { guarded_mb_scope($subject); } catch (RuntimeException) {}
"#.repeat(count);
        let source = format!(r#"<?php
function stopped_mb_argument(): string {{ throw new RuntimeException("before entry"); }}
function guarded_mb_scope(string $subject): void {{ mb_strlen($subject, stopped_mb_argument()); }}
$subject = str_repeat("x", 64); $encoding = "UTF-8";
$items = ["label" => [$subject]]; $mixed = $argc > 0 ? $subject : [];
{calls}
echo strlen($subject), ":", mb_check_encoding($items) ? "valid" : "invalid";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "64:valid");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "pre-entry unwinding retained captured arguments");
}

/// Preserves an outer capture while a later argument catches and recovers from its own mbstring failure.
#[test]
fn test_mbstring_argument_guards_nested_catch() {
    let source = r#"<?php
function recovered_mb_encoding(): string {
    try { mb_strlen("nested", "invalid"); }
    catch (ValueError) { echo "recovered:"; }
    return "UTF-8";
}
$subject = str_repeat("猫", 32);
var_dump(mb_strlen($subject, recovered_mb_encoding()));
var_dump(mb_strlen(string: $subject, encoding: recovered_mb_encoding()));
echo strlen($subject);
"#;
    assert_eq!(compile_and_run(source), "recovered:int(32)\nrecovered:int(32)\n96");
}

/// Releases a freshly captured object before entering the catch for a later argument's exception.
#[test]
fn test_mbstring_argument_guards_destructor_order() {
    let source = r#"<?php
class CapturedMbArgument {
    public function __toString(): string { echo "unexpected conversion\n"; return "text"; }
    public function __destruct() { echo "released\n"; }
}
function rejected_mb_encoding(): string { echo "argument\n"; throw new RuntimeException("stopped"); }
try { mb_strlen(new CapturedMbArgument(), rejected_mb_encoding()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
echo "after";
"#;
    assert_eq!(compile_and_run(source), "argument\nreleased\nstopped\nafter");
}

/// Removes guards in parameter order without discarding newer captures made in named source order.
#[test]
fn test_mbstring_argument_guards_named_success_ownership() {
    let mut residual = Vec::new();
    for count in [1, 24] {
        let calls = "mb_substr(encoding: $encoding, length: 2, string: $subject, start: 1); mb_str_pad(pad_string: $padding, string: $subject, length: 8, encoding: $encoding);\n".repeat(count);
        let source = format!(r#"<?php
$subject = str_repeat("é", 4); $encoding = "UTF-8"; $padding = ".";
{calls}
echo "done";
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "done");
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        residual.push(allocated as i64 - freed as i64);
    }
    assert_eq!(residual[0], residual[1], "normal named calls retained guard ownership");
}

/// Cleans evaluated arguments in parameter order and continues after a destructor raises a new exception.
#[test]
fn test_mbstring_argument_guards_parameter_cleanup_order() {
    let source = r#"<?php
class FirstMbCapture {
    public function __toString(): string { return "abc"; }
    public function __destruct() { echo "first\n"; }
}
class SecondMbCapture {
    public function __toString(): string { return "."; }
    public function __destruct() { echo "second\n"; throw new RuntimeException("destructor"); }
}
function failed_mb_capture_encoding(): string { throw new RuntimeException("argument"); }
try { mb_strimwidth(new FirstMbCapture(), 0, 1, new SecondMbCapture(), failed_mb_capture_encoding()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
try { mb_strimwidth(trim_marker: new SecondMbCapture(), string: new FirstMbCapture(),
    start: 0, width: 1, encoding: failed_mb_capture_encoding()); }
catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
echo "after";
"#;
    assert_eq!(compile_and_run(source), "first\nsecond\ndestructor\nfirst\nsecond\ndestructor\nafter");
}

/// Releases an owning regular method argument before a variadic child whose destructor throws.
#[test]
fn test_method_argument_guards_preserve_variadic_cleanup_order_after_throw() {
    let source = r#"<?php
class OrderedMethodArgument {
    public function __construct(public string $name, public bool $fail) {}
    public function __destruct() {
        echo "drop:", $this->name, "|";
        if ($this->fail) { throw new RuntimeException($this->name); }
    }
}
class ThrowingVariadicMethod {
    public function invoke(OrderedMethodArgument $regular, OrderedMethodArgument ...$tail): void {
        echo "body|";
        throw new RuntimeException("body");
    }
}
$target = new ThrowingVariadicMethod();
try {
    $target->invoke(
        new OrderedMethodArgument("regular", false),
        new OrderedMethodArgument("tail", true),
    );
} catch (Throwable $error) {
    echo "caught:", $error->getMessage(), "|";
    $previous = $error->getPrevious();
    echo "previous:", $previous?->getMessage() ?? "none", "|after";
}
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "program failed: {}", output.stderr);
    assert_eq!(
        output.stdout,
        "body|drop:regular|drop:tail|caught:tail|previous:body|after"
    );
    assert!(
        output.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected regular and variadic method argument owners to be released, got: {}",
        output.stderr
    );
}
