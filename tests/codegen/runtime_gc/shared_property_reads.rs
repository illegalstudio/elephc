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
    public function literalOne(mixed $object): mixed { return $object->secret; }
    public function literalTwo(mixed $object): mixed { return $object->{'secret'}; }
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
class SharedReadLiteralUninitialized { public string $visible; }
class SharedReadLiteralArray { public array $visible = ["original"]; }
function SharedReadGlobalOne(mixed $object, string $name): mixed { return $object->{$name}; }
function SharedReadGlobalTwo(mixed $object, string $name): mixed { return $object->{$name}; }
function SharedReadProbeOne(mixed $object, string $name): mixed { return $object->{$name} ?? "none"; }
function SharedReadProbeTwo(mixed $object, string $name): mixed { return $object->{$name} ?? "none"; }
function SharedReadName(): string { echo "name|"; return "visible"; }
function SharedReadLiteralOne(mixed $object): mixed { return $object->visible; }
function SharedReadLiteralTwo(mixed $object): mixed { return $object->{'visible'}; }
function SharedReadLiteralSecretOne(mixed $object): mixed { return $object->secret; }
function SharedReadLiteralSecretTwo(mixed $object): mixed { return $object->{'secret'}; }
function SharedReadLiteralProbeOne(mixed $object): mixed { return $object->{'visible'} ?? "none"; }
function SharedReadLiteralProbeTwo(mixed $object): mixed { return $object->{'visible'} ?? "none"; }
function SharedReadReceiver(): mixed { echo "object|"; return new SharedReadVault(); }
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
echo SharedReadLiteralOne(SharedReadReceiver()), "|", SharedReadLiteralTwo(SharedReadReceiver()), "|";
echo $object->literalOne($object), "|", $object->literalTwo($object), "|";
try { SharedReadLiteralSecretOne($object); }
catch (Error $error) { echo "private|"; unset($error); }
try { SharedReadLiteralSecretTwo($object); }
catch (Error $error) { echo "private|"; unset($error); }
echo SharedReadLiteralProbeOne(null), "|", SharedReadLiteralProbeTwo(null), "|";
echo SharedReadLiteralOne($magic), "|", SharedReadLiteralTwo($magic), "|";
$literal = new SharedReadLiteralArray();
$snapshot = SharedReadLiteralOne($literal); $snapshot[] = "local";
echo json_encode(SharedReadLiteralTwo($literal)), "|", json_encode($snapshot), "|";
unset($snapshot, $literal);
try { SharedReadLiteralOne(new SharedReadThrow()); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { SharedReadLiteralTwo(new SharedReadThrow()); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { SharedReadLiteralOne(new SharedReadLiteralUninitialized()); }
catch (Error $error) { echo "uninitialized|"; unset($error); }
unset($object, $dynamic, $magic);
"#;
    let (declarations, program) = body.split_once("$object =").unwrap();
    let eval = body.replace("SharedRead", "EvalSharedRead").replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php {declarations} if ($argc === 1) {{ $object = {program} }} else {{ $code = $argc > 0 ? '{eval}' : ''; eval($code); unset($code); }}");
    let expected = concat!(
        "name|public|public|private|private|private|none|none|dynamic|dynamic|get:key|magic|get:key|magic|isset:key|get:key|magic|isset:key|get:key|magic|getter|getter|uninitialized|",
        "object|public|object|public|private|private|private|private|none|none|get:visible|magic|get:visible|magic|[\"original\"]|[\"original\",\"local\"]|getter|getter|uninitialized|"
    );
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(&source, &[&[], &["eval"]]);
    preserve_stdout_mismatches(&source, &assembly, &runs, &[expected, expected]);
    // A semantic-only test also passes with the old inline ladders. Require actual helper calls.
    let literal_helper_calls = assembly.lines().filter(|line|
        line.contains("call _eir_shared_literal_property_read_")
            || line.contains("bl _eir_shared_literal_property_read_")).count();
    assert!(literal_helper_calls > 0, "literal reads must call their shared dispatch helper");

    let mut failures = Vec::new();
    for (mode, out) in runs.iter().enumerate() {
        if !out.success || out.stdout != expected || !out.stderr.contains("HEAP DEBUG: leak summary: clean")
            || out.stderr.contains("Warning:")
        { failures.push(format!("mode={mode}: status={} stdout={} stderr={}", out.status, out.stdout, out.stderr)); }
    }
    assert_eq!(runs.len(), 2);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
