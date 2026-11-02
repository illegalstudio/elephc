//! Purpose:
//! Keeps native Mixed property cells shared with escaping eval aliases.
//!
//! Called from:
//! - The executable runtime ownership suite in CI.
//!
//! Key details:
//! - Native declarations stay outside eval, including an inherited property and destructor.
//! - Aliases outlive both objects without retaining their receivers.

use crate::support::*;

/// AOT and eval writes, nested mutations, clone and unset preserve one escaping Mixed alias.
#[test]
fn test_native_mixed_property_escaping_eval_aliases_bundle() {
    let source = r#"<?php
class PersistentNativeMixedParent {
    public mixed $value = ['stage' => 'old'];
    public function &valueRef(): mixed { return $this->value; }
    public function __destruct() { echo 'object|'; }
}
class PersistentNativeMixedChild extends PersistentNativeMixedParent {}
class ImplicitNativeMixedAlias {
    public mixed $value = 'old';
    public function __destruct() { echo 'implicit|'; }
}
function mutatePersistentNativeMixed(mixed &$value): void { $value['argument'][] = 'call'; }
function exercisePersistentNativeMixed(bool $run): void {
    $object = new PersistentNativeMixedChild();
    $native =& $object->valueRef();
    $code = $run ? '
function capturePersistentNativeMixed(&$value): callable {
    return function (bool $write, mixed $next = null) use (&$value): mixed {
        if ($write) { $value = $next; }
        return $value;
    };
}
$escaped = capturePersistentNativeMixed($object->value);
' : '';
    eval($code);
    $object->value = ['stage' => 'property'];
    $code = $run ? 'echo json_encode($escaped(false)), "|";' : '';
    eval($code);
    $native = ['stage' => 'native'];
    eval($code);
    $code = $run ? '$escaped(true, ["stage" => "eval"]);' : '';
    eval($code);
    echo json_encode($native), ':', json_encode($object->value), '|';
    $object->value['nested'][] = 'child';
    mutatePersistentNativeMixed($native);
    $code = $run ? 'echo json_encode($escaped(false)), "|";' : '';
    eval($code);
    $copy = clone $object;
    unset($object->value);
    $object->value = ['stage' => 'recreated'];
    echo json_encode($object->value), '|';
    unset($object, $copy);
    $code = $run ? '$escaped(true, ["stage" => "survived"]); echo json_encode($escaped(false)), "|"; unset($escaped);' : '';
    eval($code);
    echo json_encode($native), '|';
    $implicit = new ImplicitNativeMixedAlias();
    $code = $run ? '$implicitAlias = capturePersistentNativeMixed($implicit->value);' : '';
    eval($code);
    $implicit->value = 'updated';
    unset($implicit);
    $code = $run ? 'echo $implicitAlias(false), "|"; unset($implicitAlias);' : '';
    eval($code);
    unset($native, $code);
}
exercisePersistentNativeMixed($argc > 0);
"#;
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "stdout={} stderr={}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "{\"stage\":\"property\"}|{\"stage\":\"native\"}|{\"stage\":\"eval\"}:{\"stage\":\"eval\"}|{\"stage\":\"eval\",\"nested\":[\"child\"],\"argument\":[\"call\"]}|{\"stage\":\"recreated\"}|object|object|{\"stage\":\"survived\"}|{\"stage\":\"survived\"}|implicit|updated|");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}
