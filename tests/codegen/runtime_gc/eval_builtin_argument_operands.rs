//! Purpose:
//! Keeps direct eval builtin argument keys and computed receivers from escaping their reads.
//!
//! Called from:
//! - Native runtime GC integration coverage through the codegen test suite.
//!
//! Key details:
//! - One opaque fragment covers scalar keys, nested reads, ArrayAccess and exception cleanup.
//! - Receiver destruction follows PHP before the builtin or its later arguments execute.

use crate::support::*;

/// Value-only builtin arguments release their read operands on normal and exceptional paths.
#[test]
fn test_eval_builtin_value_argument_owners_bundle() {
    let body = r#"
class EvalArgTargetBox { public string $text = "AB"; }
class EvalArgTargetAccess implements ArrayAccess {
    public function offsetGet(mixed $offset): mixed {
        if ($offset === 'boom') { throw new Exception('boom'); }
        return new EvalArgTargetBox();
    }
    public function offsetExists(mixed $offset): bool { return true; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function evalArgTargetBox(): EvalArgTargetBox { return new EvalArgTargetBox(); }
class EvalArgReadonly { public readonly int $number; public function __construct() { $this->number = 7; } }
class EvalArgTemporary {
    public array $items = ['A'];
    public function __destruct() { echo 'drop|'; }
}
function evalArgTemporary(): EvalArgTemporary { echo 'make|'; return new EvalArgTemporary(); }
function evalArgLaterFailure(): int { echo 'fail|'; throw new RuntimeException('later'); }
$text = 'AB'; $rows = [['AB']]; $objects = [new EvalArgTargetBox()]; $access = new EvalArgTargetAccess();
echo ord($text[0]), ord($text[1]), '|';
echo get_class($objects[0]), '|';
echo json_encode(evalArgTargetBox()->text), '|';
echo ord($rows[0][0][1]), '|', ord(str_repeat('C', 2)[1]), '|';
echo get_class($access[7]), '|';
try { echo get_class($access['boom']); } catch (Exception $error) { echo $error->getMessage(), '|'; }
echo json_encode((new EvalArgReadonly())->number), '|';
echo json_encode(evalArgTemporary()->items), '|';
try { json_encode(evalArgTemporary()->items, evalArgLaterFailure()); }
catch (RuntimeException $error) { echo $error->getMessage(), '|'; }
unset($text, $rows, $objects, $access, $error);
"#;
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source); unset($source);");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, concat!(
        "6566|EvalArgTargetBox|\"AB\"|66|67|EvalArgTargetBox|boom|7|",
        "make|drop|[\"A\"]|make|drop|fail|later|",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
    if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let expected = Command::new("php").args(["-r", &source[5..]]).output().expect("PHP oracle");
        assert!(expected.status.success(), "{}", String::from_utf8_lossy(&expected.stderr));
        assert_eq!(out.stdout.as_bytes(), expected.stdout);
    }
}
