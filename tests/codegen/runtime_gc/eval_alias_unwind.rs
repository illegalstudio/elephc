//! Purpose:
//! Keeps reflector receivers and alias-capable call operands owned during caught type errors.
//!
//! Called from:
//! - The executable runtime ownership CI suite.
//!
//! Key details:
//! - A later opaque eval widens earlier PHP locals without changing their typed reads.
//! - Fresh, borrowed and successful alias returns share the same strict heap assertion.

use crate::support::*;

/// Receiver and argument leases survive throws without changing successful alias retirement.
#[test]
fn test_eval_widened_reflector_and_alias_argument_unwind_bundle() {
    let source = r#"<?php
class AliasUnwindNode {
    public mixed $child = null;
    public function __destruct() { echo 'D'; }
}
class AliasUnwindTarget { public AliasUnwindNode $node; }
function aliasUnwindFailure(AliasUnwindNode $node): AliasUnwindNode { return $node->child; }
function aliasUnwindObject(mixed $node, bool $keep): mixed {
    return $keep ? $node : new AliasUnwindNode();
}
function aliasUnwindArray(mixed $array, bool $keep): mixed { return $keep ? $array : [9]; }
function aliasUnwindArithmetic(mixed $value, bool $keep): mixed { return $keep ? $value : 99; }
function aliasUnwindFresh(): void {
    for ($i = 0; $i < 3; $i++) {
        try { aliasUnwindFailure(new AliasUnwindNode()); }
        catch (TypeError $error) { unset($error); echo 'T|'; }
    }
}
$target = new AliasUnwindTarget();
$property = new ReflectionProperty(AliasUnwindTarget::class, 'node');
for ($i = 0; $i < 3; $i++) {
    try { $property->setValue($target, null); }
    catch (TypeError $error) { unset($error); echo 'R|'; }
}
$shared = new AliasUnwindNode();
for ($i = 0; $i < 3; $i++) {
    try { aliasUnwindFailure($shared); }
    catch (TypeError $error) { unset($error); echo 'T|'; }
}
unset($shared); echo '|';
aliasUnwindFresh();
$alias = aliasUnwindObject(new AliasUnwindNode(), true);
echo 'live|'; unset($alias); echo '|';
$alias = aliasUnwindObject(new AliasUnwindNode(), false);
echo 'live|'; unset($alias); echo '|';
$array = aliasUnwindArray([1, 2], true); $array[0] = 7; echo json_encode($array), '|';
$array = aliasUnwindArray([1, 2], false); $array[0] = 7; echo json_encode($array), '|';
echo aliasUnwindArithmetic(PHP_INT_MAX + $argc, false), '|';
$code = $argc > 0 ? '$after = 1;' : ''; eval($code);
echo $after, '|';
"#;
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    let expected = "R|R|R|T|T|T|D|DT|DT|DT|live|D|Dlive|D|[7,2]|[7]|99|1|";
    preserve_stdout_mismatches(source, &assembly, std::slice::from_ref(&out), &[expected]);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "{}", out.stderr);
}
