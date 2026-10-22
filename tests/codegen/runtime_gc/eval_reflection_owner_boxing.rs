//! Purpose:
//! Verifies AOT and native-to-eval Reflection factories transfer their fresh references.
//! Covers nested metadata collections and getter results that outlive their parent.
//!
//! Called from:
//! - The runtime GC integration suite.
//!
//! Key details:
//! - Each opaque-eval case runs twenty constructions in a fresh process and requires a clean heap.

use crate::support::compile_and_run_with_heap_debug_per_argv;

/// Opaque eval retires the initial native Reflection owner after transferring it into a box.
#[test]
fn test_dynamic_eval_reflection_factories_transfer_fresh_object_owners() {
    let program = r#"<?php
function ownerDefault(array $items = ['x']): array { return $items; }
class OwnerFixture {
    public const LABEL = 'label';
    public string $name = 'value';
    public function method(int $value = 1): string { return 'result'; }
}
enum OwnerEnum { case Item; }
eval($argv[1]);
"#;
    let cases = [
        ("for ($i=0;$i<20;$i++) { $p=(new ReflectionFunction('ownerDefault'))->getParameters(); echo $p[0]->getName(), '|'; unset($p); }", "items|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $f=new ReflectionMethod('OwnerFixture', 'method'); $p=$f->getParameters(); unset($f); echo $p[0]->getName(), '|'; unset($p); }", "value|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionParameter('ownerDefault', 0); $t=$p->getType(); unset($p); echo $t->getName(), '|'; unset($t); }", "array|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionParameter(['OwnerFixture', 'method'], 0); echo $p->getName(), '|'; unset($p); }", "value|".repeat(20)),
        ("$receiver=new OwnerFixture(); for ($i=0;$i<20;$i++) { $p=new ReflectionParameter([$receiver, 'method'], 0); echo $p->getName(), '|'; unset($p); } unset($receiver);", "value|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { try { new ReflectionParameter(['OwnerFixture', 'method'], 99); } catch (ReflectionException $e) { echo 'caught|'; unset($e); } }", "caught|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionParameter('ownerDefault', 0); echo $p->getName(), '|'; unset($p); }", "items|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionFunction('ownerDefault'); echo $p->getName(), '|'; unset($p); }", "ownerDefault|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionClass('OwnerFixture'); echo $p->getName(), '|'; unset($p); }", "OwnerFixture|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionMethod('OwnerFixture', 'method'); echo $p->getName(), '|'; unset($p); }", "method|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionProperty('OwnerFixture', 'name'); echo $p->getName(), '|'; unset($p); }", "name|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=new ReflectionClassConstant('OwnerFixture', 'LABEL'); echo $p->getValue(), '|'; unset($p); }", "label|".repeat(20)),
        ("for ($i=0;$i<20;$i++) { $p=(new ReflectionFunction('ownerDefault'))->getReturnType(); echo $p->getName(), '|'; unset($p); }", "array|".repeat(20)),
    ];
    let arguments = cases.iter().map(|(source, _)| vec![*source]).collect::<Vec<_>>();
    let argvs = arguments.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &argvs);
    for (output, (source, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{source}\n{}", output.stderr);
        assert_eq!(output.stdout, expected, "{source}");
        assert!(output.stderr.contains("leak summary: clean"), "{source}\n{}", output.stderr);
    }
}

/// Issue #1352's literal AOT constructor must retire its nested reflected metadata at every count.
#[test]
fn test_aot_reflection_parameter_transfers_metadata_owners() {
    let program = r#"<?php
$count = (int)$argv[1];
for ($i = 0; $i < $count; $i++) {
    $parameter = new ReflectionParameter('strlen', 'string');
    if ($parameter->name !== 'string') { echo 'bad'; }
    unset($parameter);
}
echo 'ok';
"#;
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &[&["1"], &["10"], &["50"]]);
    for output in outputs {
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, "ok", "{}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{}", output.stderr);
    }
}

/// AOT getter results keep their own reflected type and function after their parent is unset.
#[test]
fn test_aot_reflection_parameter_children_outlive_parent() {
    let program = r#"<?php
function other(int $other): int { return $other; }
for ($i = 0; $i < 20; $i++) {
    $parameter = new ReflectionParameter('strlen', 'string');
    $type = $parameter->getType();
    $function = $parameter->getDeclaringFunction();
    unset($parameter);
    $replacement = new ReflectionParameter('other', 'other');
    if ($replacement->name !== 'other') { echo 'bad-replacement|'; }
    if ($type instanceof ReflectionNamedType) {
        echo $type->getName(), ':', $function->getName(), '|';
    } else { echo 'bad-type|'; }
    unset($replacement, $type, $function);
}
"#;
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &[&[]]);
    assert!(outputs[0].success, "{}", outputs[0].stderr);
    assert_eq!(outputs[0].stdout, "string:strlen|".repeat(20), "{}", outputs[0].stderr);
    assert!(outputs[0].stderr.contains("leak summary: clean"), "{}", outputs[0].stderr);
}

/// AOT ReflectionClass collection getters preserve their results after retiring the reflector.
#[test]
fn test_aot_reflection_collections_outlive_owner() {
    let program = r#"<?php
class AotOtherFixture { public const OTHER = 'other'; public int $other = 8; }
class AotOwnerFixture {
    public const LABEL = 'label';
    public int $value = 7;
}
for ($i = 0; $i < 20; $i++) {
    $owner = new ReflectionClass(AotOwnerFixture::class);
    $constants = $owner->getConstants();
    $defaults = $owner->getDefaultProperties();
    unset($owner);
    $replacement = new ReflectionClass(AotOtherFixture::class);
    if ($replacement->getName() !== 'AotOtherFixture') { echo 'bad-replacement|'; }
    echo $constants['LABEL'], ':', $defaults['value'], '|';
    unset($replacement, $constants, $defaults);
}
"#;
    let outputs = compile_and_run_with_heap_debug_per_argv(program, &[&[]]);
    assert!(outputs[0].success, "{}", outputs[0].stderr);
    assert_eq!(outputs[0].stdout, "label:7|".repeat(20), "{}", outputs[0].stderr);
    assert!(outputs[0].stderr.contains("leak summary: clean"), "{}", outputs[0].stderr);
}
