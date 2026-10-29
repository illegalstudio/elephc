//! Purpose:
//! Pins operand order and failure handling for nested eval dimension unset.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - Computed roots are captured early, while variable containers and indices remain live.
//! - Missing paths stay quiet and unsupported array attachments retain their contents.

use super::super::*;
use super::support::*;

/// Nested unset captures keys once before selecting live storage and retires exceptional operands.
#[test]
fn nested_unset_stages_keys_before_live_receiver_selection() {
    let source = br#"
class NestedUnsetReceiver implements ArrayAccess {
    public string $name = '';
    public function __construct(string $name) { $this->name = $name; }
    public function offsetExists(mixed $offset): bool { return false; }
    public function offsetGet(mixed $offset): mixed { return null; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void { echo $this->name, ':', $offset, '|'; }
}
function nestedUnsetKey(mixed $replacement): string {
    global $nestedUnsetRows;
    echo 'key|'; $nestedUnsetRows = [$replacement]; return 'id';
}
function nestedUnsetInnerKey(): int { echo 'inner|'; return 0; }
function nestedUnsetRoot(): array { global $nestedUnsetRows; echo 'root|'; return $nestedUnsetRows; }
function nestedUnsetLateKey(): string {
    global $nestedUnsetIndex;
    echo 'late|'; $nestedUnsetIndex = 1; return 'id';
}
function nestedUnsetFailKey(): string { echo 'fail|'; throw new Error('stop'); }
$old = new NestedUnsetReceiver('old');
$replacement = new NestedUnsetReceiver('new');
$nestedUnsetRows = [$old];
unset($nestedUnsetRows[nestedUnsetInnerKey()][nestedUnsetKey($replacement)]);
$nestedUnsetRows = [$old];
unset(nestedUnsetRoot()[0][nestedUnsetKey($replacement)]);
$nestedUnsetRows = [$old, $replacement]; $nestedUnsetIndex = 0;
unset($nestedUnsetRows[$nestedUnsetIndex][nestedUnsetLateKey()]);
try { unset(nestedUnsetRoot()[nestedUnsetInnerKey()][nestedUnsetFailKey()]); }
catch (Error $error) { echo 'caught|'; }
$nestedUnsetRows = [];
unset($nestedUnsetRows['missing']['id']); echo 'quiet|';
$nestedUnsetRows = [['keep' => 1]];
try { unset($nestedUnsetRows[0]['keep']); }
catch (Error $error) { echo $error->getMessage(), '|', $nestedUnsetRows[0]['keep']; }
unset($old, $replacement, $nestedUnsetRows, $nestedUnsetIndex, $error);
"#;
    let program = parse_fragment(source).unwrap();
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    assert!(context.set_global_scope(&mut scope));
    let result =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, result).unwrap();
    assert_eq!(values.output,
        "inner|key|new:id|root|key|old:id|late|new:id|root|inner|fail|caught|quiet|\
         Unsupported unset(): erased array storage has no attached removal path|1");
    assert!(values.warnings.is_empty(), "{:?}", values.warnings);
}

/// Intermediate dimensions reject strings and numbers while preserving false and null behavior.
#[test]
fn nested_unset_reports_scalar_intermediate_diagnostics() {
    let source = br#"
$cases = ['abc', '', 5, 1.5, true, false, null];
foreach ($cases as $value) {
    $rows = [$value];
    try { unset($rows[0][0][0]); echo "quiet\n"; }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
}
$holder = new stdClass(); $holder->field = 'abc';
try { unset($holder->field[0][1]); }
catch (Error $error) { echo $error->getMessage(), "\n"; }
$rows = [5]; unset($rows[0]->field[0]); echo "property-quiet\n";
unset($cases, $value, $rows, $holder, $error);
"#;
    let program = parse_fragment(source).unwrap();
    let mut values = FakeOps::default();
    let mut context = ElephcEvalContext::new();
    let mut scope = ElephcEvalScope::new();
    let result =
        execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
    eval_release_value(&mut context, &mut values, result).unwrap();
    let string_error = "Cannot use string offset as an array";
    let scalar_error = "Cannot unset offset in a non-array variable";
    assert_eq!(values.output, format!(
        "{string_error}\n{string_error}\n{scalar_error}\n{scalar_error}\n{scalar_error}\nquiet\nquiet\n{string_error}\nproperty-quiet\n"));
    assert_eq!(values.warnings,
        ["Deprecated: Automatic conversion of false to array is deprecated"]);
}
