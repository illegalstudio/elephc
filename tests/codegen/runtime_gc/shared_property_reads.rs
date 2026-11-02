//! Purpose:
//! Checks shared property dispatch for visibility, magic accessors and exception ownership.
//!
//! Called from:
//! - Executable runtime GC CI on native and opaque-eval paths.
//!
//! Key details:
//! - Both paths share one compile and retain separate strict heap checks.

use crate::support::*;

/// Shared helpers preserve class scope, dynamic hashes, probes and catchable getter failures.
#[test]
fn test_shared_property_read_owners_bundle() {
    let body = r#"
class SharedReadVault {
    public string $visible = "public";
    private string $secret = "private";
    public function one(mixed $object, string $name): mixed { return $object->{$name}; }
    public function two(mixed $object, string $name): mixed { return $object->{$name}; }
}
#[AllowDynamicProperties] class SharedReadDynamic {}
class SharedReadMagic {
    public function __isset(string $name): bool { echo "isset:", $name, "|"; return true; }
    public function __get(string $name): mixed { echo "get:", $name, "|"; return "magic"; }
}
class SharedReadThrow {
    public function __get(string $name): mixed { throw new RuntimeException("getter"); }
}
class SharedReadUninitialized { public string $value; }
class SharedReadBag { public array $items = [1, 2, 3]; }
function SharedReadGlobalOne(mixed $object, string $name): mixed { return $object->{$name}; }
function SharedReadGlobalTwo(mixed $object, string $name): mixed { return $object->{$name}; }
function SharedReadProbeOne(mixed $object, string $name): mixed { return $object->{$name} ?? "none"; }
function SharedReadProbeTwo(mixed $object, string $name): mixed { return $object->{$name} ?? "none"; }
function SharedReadName(): string { echo "name|"; return "visible"; }
function SharedReadQuietOne(mixed $object, string $name): void { unset($object->{$name}[1]); }
function SharedReadQuietTwo(mixed $object, string $name): void { unset($object->{$name}[2]); }
$object = new SharedReadVault();
echo SharedReadGlobalOne($object, SharedReadName()), "|", SharedReadGlobalTwo($object, "visible"), "|";
echo $object->one($object, "secret"), "|", $object->two($object, "secret"), "|";
try { SharedReadGlobalOne($object, "secret"); }
catch (Error $error) { echo "private|"; unset($error); }
echo SharedReadProbeOne($object, "secret"), "|", SharedReadProbeTwo($object, "secret"), "|";
$dynamic = new SharedReadDynamic(); $dynamic->key = "dynamic";
echo SharedReadGlobalOne($dynamic, "key"), "|", SharedReadGlobalTwo($dynamic, "key"), "|";
$magic = new SharedReadMagic();
echo SharedReadGlobalOne($magic, "key"), "|", SharedReadGlobalTwo($magic, "key"), "|";
echo SharedReadProbeOne($magic, "key"), "|", SharedReadProbeTwo($magic, "key"), "|";
try { SharedReadGlobalOne(new SharedReadThrow(), "key"); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { SharedReadGlobalTwo(new SharedReadThrow(), "key"); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { SharedReadGlobalOne(new SharedReadUninitialized(), "value"); }
catch (Error $error) { echo "uninitialized|"; unset($error); }
$bag = new SharedReadBag();
SharedReadQuietOne($bag, 'items'); SharedReadQuietTwo($bag, 'items');
echo json_encode($bag->items), '|'; unset($bag);
unset($object, $dynamic, $magic);
"#;
    let (declarations, program) = body.split_once("$object =").unwrap();
    let eval = body.replace("SharedRead", "EvalSharedRead").replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {declarations} if ($argc === 1) {{ $object = {program} }} else {{ $code = $argc > 0 ? '{eval}' : ''; eval($code); unset($code); }}");
    let expected = "name|public|public|private|private|private|none|none|dynamic|dynamic|get:key|magic|get:key|magic|isset:key|get:key|magic|isset:key|get:key|magic|getter|getter|uninitialized|[1]|";
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(&source, &[&[], &["eval"]]);
    preserve_stdout_mismatches(&source, &assembly, &runs, &[expected, expected]);
    let mut failures = Vec::new();
    for (mode, out) in runs.iter().enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean")
            || out.stderr.contains("Warning:")
        { failures.push(format!("mode={mode}: stdout={} stderr={}", out.stdout, out.stderr)); }
    }
    assert_eq!(runs.len(), 2);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
