//! Purpose:
//! Exercises normalized string arguments crossing the eval-to-native call boundary.
//!
//! Called from:
//! - The codegen integration harness through `runtime_gc`.
//!
//! Key details:
//! - By-value arguments borrow rooted bytes; reference arguments keep owning writeback storage.
//! - Repeated calls must not retain a second string conversion per argument.

use crate::support::*;

/// Weak coercion, embedded NUL bytes, stack arguments, and returned borrows survive native calls.
#[test]
fn test_core_eval_native_string_arguments_preserve_coercions_and_returns() {
    let source = r#"<?php
class NativeStringArguments {
    public string $text;
    public function __construct(string $text) { $this->text = $text; }
    public function identity(string $text): string { return $text; }
    public static function join8(string $a, string $b, string $c, string $d,
        string $e, string $f, string $g, string $h): string {
        return $a . $b . $c . $d . $e . $f . $g . $h;
    }
    public function change(string &$text): void { $text .= "!"; }
}
$source = '$object = new NativeStringArguments(123);
echo $object->text, "|";
$result = $object->identity("a\0b"); echo strlen($result), ":", bin2hex($result), "|";
$result = NativeStringArguments::join8(1, 2, 3, 4, 5, 6, 7, 8); echo $result, "|";
$result = $object->identity(456); $reuse = "replacement"; echo $result, "|";
$reference = "ref"; $object->change($reference); echo $reference;
unset($object); echo "|", $result;' . ' // ' . $argc;
eval($source);
"#;
    assert_eq!(compile_and_run(source), "123|3:610062|12345678|456|ref!|456");
}

/// Native method and constructor calls do not leak redundant persisted by-value string copies.
#[test]
fn test_core_eval_native_string_arguments_release_each_activation() {
    let native = r#"
class NativeStringArgumentSink {
    public function __construct(string $text) {}
    public function accept(string $text): void {}
    public static function acceptStatic(string $text): void {}
}
"#;
    let body = r#"
$temporary = new NativeStringArgumentSink("constructor"); unset($temporary);
$object->accept("literal"); $object->accept(123);
NativeStringArgumentSink::acceptStatic("static");
NativeStringArgumentSink::acceptStatic(456);
"#;
    let live = |iterations| {
        let body = body.repeat(iterations);
        let source = format!(r#"<?php {native}
$source = '$object = new NativeStringArgumentSink("setup"); {body}
unset($object); return 42;' . ' // ' . $argc;
echo eval($source);
"#);
        let output = compile_and_run_with_gc_stats(&source);
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "42", "{}", output.stderr);
        let (allocated, freed) = parse_gc_stats(&output.stderr);
        allocated as i128 - freed as i128
    };
    assert_eq!(live(5), live(1), "native string argument staging leaked");
}
