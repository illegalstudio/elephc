//! Purpose:
//! Verifies direct instance calls for final classes and preserves virtual base dispatch.
//!
//! Called from:
//! - The codegen integration suite through `oop`.
//!
//! Key details:
//! - Emitter assertions disable inlining and cover every supported target.
//! - Runtime cases retain inheritance, nullable receivers, and literal parameter guards.

use super::*;

/// Removes only the directory allocated for the cross-target emitter fixture.
struct DispatchDirectory(std::path::PathBuf);

impl Drop for DispatchDirectory {
    /// Cleans source and generated assembly on success and assertion failure.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Selects a function's main body without including adjacent functions or runtime data.
fn function_body<'a>(assembly: &'a str, name: &str) -> &'a str {
    let start = format!("{}:\n", elephc::names::function_symbol(name));
    let end = format!("{}:", elephc::names::function_epilogue_symbol(name));
    assembly.split_once(&start).unwrap_or_else(|| panic!("missing {start}"))
        .1.split_once(&end).unwrap_or_else(|| panic!("missing {end}")).0
}

/// Final-class methods call their emitted implementation directly on all five targets.
#[test]
fn test_final_class_dispatch_supported_target_assembly() {
    let dir = DispatchDirectory(make_cli_test_dir("elephc_final_class_dispatch"));
    let source = dir.0.join("main.php");
    fs::write(&source, r#"<?php
class ParentDispatch {
    public function inherited(int $value): int { return $value + 1; }
    public function value(int $value): int { return $value + 2; }
}
final class SealedDispatch extends ParentDispatch {
    public function value(int $value): int { return $value + 3; }
}
class DispatchAnimal {}
class DispatchDog extends DispatchAnimal {}
final class VariantDispatch<+T> {
    public function __construct(private T $value) {}
    public function kind(false $value): string { return self::class; }
    final public function finalKind(false $value): string { return self::class; }
}
function read_final(SealedDispatch $object, int $value): int { return $object->value($value); }
function read_inherited(SealedDispatch $object, int $value): int { return $object->inherited($value); }
function read_nullable(?SealedDispatch $object, int $value): int { return $object->value($value); }
function read_base(ParentDispatch $object, int $value): int { return $object->value($value); }
function read_variant(VariantDispatch<DispatchAnimal> $object, mixed $value): string { return $object->kind($value); }
function read_variant_final(VariantDispatch<DispatchAnimal> $object, mixed $value): string { return $object->finalKind($value); }
$object = new SealedDispatch();
echo read_final($object, $argc), read_inherited($object, $argc), read_nullable($object, $argc), read_base($object, $argc);
$variant = new VariantDispatch<DispatchDog>(new DispatchDog());
echo read_variant($variant, false), read_variant_final($variant, false);
"#).unwrap();
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let output = elephc_cli_command(&dir.0)
            .args(["--ir-opt=off", "--emit-asm", "--emit", "staticlib", "--target", target])
            .arg(&source).output().unwrap();
        assert!(output.status.success(), "{target}: {}", String::from_utf8_lossy(&output.stderr));
        let assembly = fs::read_to_string(dir.0.join("main.s")).unwrap();
        let call = if target == "linux-x86_64" { "call" } else { "bl" };
        for (function, implementation) in [
            ("read_final", "_method_SealedDispatch_value"),
            ("read_inherited", "_method_ParentDispatch_inherited"),
            ("read_nullable", "_method_SealedDispatch_value"),
        ] {
            let body = function_body(&assembly, function);
            assert!(body.contains(&format!("{call} {implementation}")), "{target}: {body}");
            assert!(!body.contains("_class_source_vtable_ptrs"), "{target}: {body}");
            assert!(!body.contains("_class_vtable_ptrs"), "{target}: {body}");
        }
        for function in ["read_base", "read_variant", "read_variant_final"] {
            let body = function_body(&assembly, function);
            assert!(body.contains("_class_source_vtable_ptrs") || body.contains("_class_vtable_ptrs"),
                "{target}: runtime-selected {function} dispatch became direct: {body}");
            if function != "read_base" {
                assert!(body.contains("literal_param_ok"), "{target}: missing literal guard: {body}");
                assert!(body.contains("interface_literal_class"), "{target}: guard lost runtime declaring-class diagnostics: {body}");
            }
        }
    }
}

/// Direct final-class calls preserve inherited bodies, overrides, and nullable payloads.
#[test]
fn test_final_class_dispatch_runtime_and_source_abi() {
    let source = r#"<?php
class DispatchBase {
    public function inherited(int $value): int { return $value + 1; }
    public function value(): string { return 'base'; }
}
final class DispatchLeaf extends DispatchBase {
    public function value(): string { return 'leaf'; }
    public function update(int &$value, string $suffix = '!'): string {
        $value++;
        return $this->value() . $suffix;
    }
}
function from_base(DispatchBase $object): string { return $object->value(); }
function from_nullable(?DispatchLeaf $object): string { return $object?->value() ?? 'null'; }
function from_nullable_arrow(?DispatchLeaf $object): string { return $object->value(); }
$leaf = new DispatchLeaf();
$value = $argc;
echo $leaf->inherited($value), ':', from_base($leaf), ':', from_nullable($leaf), ':', from_nullable(null), ':';
echo $leaf->update(value: $value), ':', $value;
echo ':', from_nullable_arrow($leaf);
"#;
    assert_eq!(compile_and_run(source), "2:leaf:leaf:null:leaf!:2:leaf");
    without_ir_opt(|| assert_eq!(compile_and_run(source), "2:leaf:leaf:null:leaf!:2:leaf"));
}

/// A final override may widen a parent's false-only parameter without losing its own guard.
#[test]
fn test_final_class_dispatch_literal_parameter_guards() {
    let out = compile_and_run(r#"<?php
class LiteralDispatchBase {
    public function accept(false $value): string { return 'base'; }
}
final class LiteralDispatchLeaf extends LiteralDispatchBase {
    public function accept(bool $value): string { return $value ? 'true' : 'false'; }
    public function onlyFalse(false $value): string { return 'false-only'; }
    final public function onlyFalseFinal(false $value): string { return 'final-false'; }
}
function widened(LiteralDispatchLeaf $object, bool $value): string { return $object->accept($value); }
function guarded(?LiteralDispatchLeaf $object, mixed $value): void {
    try { echo $object->onlyFalse($value); } catch (TypeError $error) { echo 'false-error'; }
    echo ':';
    try { echo $object->onlyFalseFinal($value); } catch (TypeError $error) { echo 'final-error'; }
}
$leaf = new LiteralDispatchLeaf();
echo widened($leaf, $argc > 0), ':';
guarded($leaf, false);
echo ':';
guarded($leaf, null);
"#);
    assert_eq!(out, "true:false-only:final-false:false-error:final-error");
}

/// Variance keeps the actual generic body's lexical class and per-instantiation static state.
#[test]
fn test_final_class_dispatch_generic_variance() {
    let source = r#"<?php
class DispatchAnimal {}
class DispatchDog extends DispatchAnimal {}
final class CovariantDispatch<+T> {
    private static int $made = 0;
    public function __construct(private T $value) { self::$made++; }
    public function kind(): string { return self::class . ':' . self::$made; }
    final public function finalKind(): string { return self::class . ':' . self::$made; }
    public function onlyFalse(false $value): string { return 'wrong'; }
    final public function onlyFalseFinal(false $value): string { return 'wrong'; }
}
class ContravariantDispatch<-T> {
    private static int $made = 0;
    public function __construct() { self::$made++; }
    public function consume(T $value): string { return self::class . ':' . self::$made; }
    final public function finalConsume(T $value): string { return self::class . ':' . self::$made; }
    final public function onlyFalse(false $value): string { return 'wrong'; }
}
function inspect_covariant(CovariantDispatch<DispatchAnimal> $object): string {
    return $object->kind() . '|' . $object->finalKind();
}
function inspect_contravariant(ContravariantDispatch<DispatchDog> $object, DispatchDog $dog): string {
    return $object->consume($dog) . '|' . $object->finalConsume($dog);
}
function inspect_covariant_literals(?CovariantDispatch<DispatchAnimal> $object, mixed $value): void {
    try { echo $object->onlyFalse($value); } catch (TypeError $error) { echo $error->getMessage(); }
    echo ':';
    try { echo $object->onlyFalseFinal($value); } catch (TypeError $error) { echo $error->getMessage(); }
}
function inspect_contravariant_literal(ContravariantDispatch<DispatchDog> $object, mixed $value): void {
    try { echo $object->onlyFalse($value); } catch (TypeError $error) { echo $error->getMessage(); }
}
$dog = new DispatchDog();
$covariant = new CovariantDispatch<DispatchDog>($dog);
$contravariant = new ContravariantDispatch<DispatchAnimal>();
echo inspect_covariant($covariant), '|', inspect_contravariant($contravariant, $dog), '|';
inspect_covariant_literals($covariant, $argc > 0);
echo '|';
inspect_contravariant_literal($contravariant, $argc > 0);
"#;
    let expected = concat!(
        "CovariantDispatch<DispatchDog>:1|CovariantDispatch<DispatchDog>:1|",
        "ContravariantDispatch<DispatchAnimal>:1|ContravariantDispatch<DispatchAnimal>:1|",
        "CovariantDispatch<DispatchDog>::onlyfalse(): Argument #1 ($value) must be of type false, true given:",
        "CovariantDispatch<DispatchDog>::onlyfalsefinal(): Argument #1 ($value) must be of type false, true given|",
        "ContravariantDispatch<DispatchAnimal>::onlyfalse(): Argument #1 ($value) must be of type false, true given",
    );
    assert_eq!(compile_and_run(source), expected);
    without_ir_opt(|| assert_eq!(compile_and_run(source), expected));
}

/// Rebound closure receivers stay boxed and select their actual class before method resolution.
#[test]
fn test_final_class_dispatch_rebound_closure_receivers() {
    let source = r#"<?php
class ClosureDispatchAnimal { public function speak(): string { return 'animal'; } }
final class ClosureDispatchDog extends ClosureDispatchAnimal {
    public string $name = 'rex';
    public function speak(): string { return 'dog ' . $this->name; }
    public function speaker() { return function () { return $this->speak(); }; }
}
echo (new ClosureDispatchAnimal())->speak(), '|';
$speaker = (new ClosureDispatchDog())->speaker();
echo $speaker->bindTo(new ClosureDispatchAnimal())(), '|';
echo $speaker->call(new ClosureDispatchAnimal()), '|';
echo Closure::bind($speaker, new ClosureDispatchAnimal())();
"#;
    assert_eq!(compile_and_run(source), "animal|animal|animal|animal");
    without_ir_opt(|| assert_eq!(compile_and_run(source), "animal|animal|animal|animal"));
}
