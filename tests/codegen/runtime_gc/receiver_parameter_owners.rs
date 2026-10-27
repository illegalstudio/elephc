//! Purpose:
//! Checks object parameter receiver lifetimes with receiver-root preparation enabled and disabled.
//!
//! Called from:
//! - The codegen runtime ownership suite on every executable supported target.
//!
//! Key details:
//! - Immutable parameters remain alive across throwing argument evaluation.
//! - Reassigned, aliased and by-reference bindings preserve the original receiver through invocation.

use crate::support::compile_and_run_with_heap_debug;

/// Receiver borrows and required leases preserve receiver identity and retire every owner once.
#[test]
fn test_fileinfo_parameter_receiver_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class ParameterRootGuard {
    public static int $argumentFailures = 0;
    public static int $freed = 0;
    public int $id;
    public function __construct(int $id) { $this->id = $id; }
    public function read(int $value): int { return $this->id + $value; }
    public function __destruct() {
        self::$freed++;
        if ($this->id === 2) { self::$argumentFailures++; }
        if ($this->id === 1) { echo "D1|"; }
    }
}
function parameterRootArgument(bool $throw): int {
    if ($throw) { echo "A|"; throw new Error("argument"); }
    return 2;
}
function immutableParameterRoot(ParameterRootGuard $receiver, bool $throw): int {
    return $receiver->read(parameterRootArgument($throw));
}
function reassignedParameterRoot(ParameterRootGuard $receiver): int {
    return $receiver->read(($receiver = new ParameterRootGuard(4))->read(1));
}
function aliasedParameterRoot(ParameterRootGuard $receiver): int {
    $alias = &$receiver;
    return $receiver->read(($alias = new ParameterRootGuard(6))->read(1));
}
function referencedParameterRoot(ParameterRootGuard &$receiver): int {
    return $receiver->read(($receiver = new ParameterRootGuard(8))->read(1));
}
function replaceAndThrowParameterRoot(ParameterRootGuard &$slot, int $id): int {
    $slot = new ParameterRootGuard($id);
    throw new Error("rebind");
}
function referencedThrowParameterRoot(ParameterRootGuard &$receiver): int {
    return $receiver->read(replaceAndThrowParameterRoot($receiver, 10));
}
function aliasedThrowParameterRoot(ParameterRootGuard $receiver): int {
    $alias = &$receiver;
    return $receiver->read(replaceAndThrowParameterRoot($alias, 12));
}
for ($i = 0; $i < 3; $i++) {
    echo immutableParameterRoot(new ParameterRootGuard(1), false), "|";
    try { immutableParameterRoot(new ParameterRootGuard(2), true); }
    catch (Error $error) { echo $error->getMessage(), "|"; }
    echo reassignedParameterRoot(new ParameterRootGuard(3)), "|";
    echo aliasedParameterRoot(new ParameterRootGuard(5)), "|";
    $reference = new ParameterRootGuard(7);
    echo referencedParameterRoot($reference), "|";
    $throwReference = new ParameterRootGuard(9);
    try { referencedThrowParameterRoot($throwReference); }
    catch (Error $error) { echo $error->getMessage(), "|"; }
    try { aliasedThrowParameterRoot(new ParameterRootGuard(11)); }
    catch (Error $error) { echo $error->getMessage(), "|"; }
    unset($reference, $throwReference, $error);
    echo "E", ParameterRootGuard::$argumentFailures, ":F", ParameterRootGuard::$freed, "|\n";
}
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    let expected = (1..=3).map(|count| format!("D1|3|A|argument|8|12|16|rebind|rebind|E{count}:F{}|\n", count * 12)).collect::<String>();
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Disabling EIR optimization preserves the same lifetime and exception behavior.
#[test]
fn test_fileinfo_parameter_receiver_owners_without_ir_opt() {
    crate::support::without_ir_opt(test_fileinfo_parameter_receiver_owners);
}
