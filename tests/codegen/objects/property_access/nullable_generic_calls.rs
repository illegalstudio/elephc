//! Method dispatch on nullable generic object values.

use super::*;

#[test]
fn test_generic_object_missing_method_reports_runtime_class() {
    let out = compile_and_run(r#"<?php
class GenericRawMissingReceiver {}
function genericRawMissingReceiver(): object { return new GenericRawMissingReceiver(); }
try { genericRawMissingReceiver()->missing(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericRawMissingReceiver::missing()");
}

#[test]
fn test_nullable_generic_object_rejects_nonmatching_method_candidate() {
    let out = compile_and_run(r#"<?php
class GenericCandidate { public function selected(): int { return 1; } }
class GenericOtherReceiver {}
function genericCandidate(bool $matching): ?object {
    return $matching ? new GenericCandidate() : new GenericOtherReceiver();
}
try { genericCandidate(false)->selected(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericOtherReceiver::selected()");
}

#[test]
fn test_nullable_generic_object_missing_method_reports_runtime_class() {
    let out = compile_and_run(r#"<?php
class GenericMissingReceiver {}
function genericMissingReceiver(): ?object { return new GenericMissingReceiver(); }
try { genericMissingReceiver()->missing(); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to undefined method GenericMissingReceiver::missing()");
}

#[test]
fn test_nullable_generic_object_return_dispatches_runtime_method() {
    let out = compile_and_run(r#"<?php
class GenericReceiver {
    public function has(string $name): bool { return $name === 'item'; }
}
function makeGenericReceiver(): ?object { return new GenericReceiver(); }
echo makeGenericReceiver()->has('item') ? 'yes' : 'no';
"#);
    assert_eq!(out, "yes");
}

#[test]
fn test_nullable_generic_object_nullsafe_call_skips_arguments() {
    let out = compile_and_run(r#"<?php
class GenericNullableReceiver {
    public function has(string $name): bool { return $name === 'item'; }
}
function genericReceiver(bool $present): ?object { return $present ? new GenericNullableReceiver() : null; }
function argument(): string { echo 'arg:'; return 'item'; }
echo (int) genericReceiver(false)?->has(argument()), '|';
echo (int) genericReceiver(true)?->has(argument());
"#);
    assert_eq!(out, "0|arg:1");
}

#[test]
fn test_nullable_generic_object_null_method_call_throws_error() {
    let out = compile_and_run(r#"<?php
class GenericNullReceiver { public function has(string $name): bool { return true; } }
function genericNullReceiver(bool $present): ?object { return $present ? new GenericNullReceiver() : null; }
try { genericNullReceiver(false)->has('item'); }
catch (Error $error) { echo $error->getMessage(); }
"#);
    assert_eq!(out, "Call to a member function has() on null");
}
