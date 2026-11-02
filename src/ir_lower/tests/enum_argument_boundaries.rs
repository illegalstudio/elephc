//! Purpose:
//! Verifies enum unpack integer diagnostics and concrete collectors on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Runtime array parameters retain the implicit integer cast through optimization.
//! - Each target independently emits both float and quoted-string diagnostic paths.
//! - Mixed hash entries retain raw callable descriptors through rooted literal construction.

/// Lowers and emits the enum argument boundary fixture for one target.
fn verify(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
interface BoundArguments { public function describe(int ...$values): string; }
enum BoundValue implements BoundArguments {
    case A;
    public function describe(int ...$values): string {
        $out = ''; foreach ($values as $value) { $out .= gettype($value); } return $out;
    }
    public function text(string ...$values): string { return implode('|', $values); }
}
function bound_call(BoundArguments $target, array $args): string { return $target->describe(...$args); }
function text_call(?BoundValue $target, array $args): ?string { return $target?->text(...$args); }
echo bound_call(BoundValue::A, [1.5, '2.75']), text_call(BoundValue::A, ['ok']);
interface MissingArguments { public function value(int $first, int $second): int; }
enum MissingValue implements MissingArguments {
    case A;
    public function value(int $first, int $second): int { return $first + $second; }
}
function good_float(): float { return 1.5; }
function missing_call(MissingArguments $target): void {
    try { $target->value(good_float()); } catch (ArgumentCountError $error) { echo 'missing'; }
}
missing_call(MissingValue::A);
interface NamedHole { public function value(int $a, int $b, int $c): void; }
enum NamedHoleValue implements NamedHole {
    case A; public function value(int $a, int $b, int $c): void {}
}
function named_hole_call(NamedHole $target): void {
    try { $target->value(good_float(), c: 2); } catch (ArgumentCountError $error) { echo 'hole'; }
}
named_hole_call(NamedHoleValue::A);
interface StaticSelected { public function value(int &$out, int $input): int; }
enum StaticLoose implements StaticSelected {
    case A; public function value(int &$out, mixed $input): int { ++$out; return 11; }
}
enum StaticTight implements StaticSelected {
    case A; public function value(int &$out, int $input): int { ++$out; return $input; }
}
function static_selected(StaticSelected $target, string $input): void {
    $out = 1;
    try { echo $target->value(input: $input, out: $out); }
    catch (TypeError $error) { echo $error->getMessage(), $out; }
}
static_selected(StaticLoose::A, 'bad'); static_selected(StaticTight::A, 'bad');
class SelectedReferenceBase {}
class SelectedReferenceNarrow extends SelectedReferenceBase {}
interface SelectedReferenceContract { public function value(SelectedReferenceNarrow &$input): string; }
enum SelectedReferenceTight implements SelectedReferenceContract {
    case A;
    public function value(SelectedReferenceNarrow &$input): string { return get_class($input); }
}
enum SelectedReferenceLoose implements SelectedReferenceContract {
    case A;
    public function value(SelectedReferenceBase &$input): string { return get_class($input); }
}
function selected_reference(SelectedReferenceContract $target, SelectedReferenceBase $input): void {
    try { echo $target->value($input); }
    catch (TypeError $error) { echo $error->getMessage(); }
}
selected_reference(SelectedReferenceTight::A, new SelectedReferenceBase());
selected_reference(SelectedReferenceLoose::A, new SelectedReferenceBase());
interface SelectedReferenceTail { public function value(int &...$items): string; }
enum SelectedReferenceTailTight implements SelectedReferenceTail {
    case A; public function value(int &...$items): string { return gettype($items[0]); }
}
enum SelectedReferenceTailLoose implements SelectedReferenceTail {
    case A; public function value(mixed &...$items): string { return gettype($items[0]); }
}
function selected_reference_tail(SelectedReferenceTail $target, mixed $input): void {
    try { echo $target->value($input), gettype($input); }
    catch (TypeError $error) { echo $error->getMessage(); }
}
selected_reference_tail(SelectedReferenceTailTight::A, '7');
selected_reference_tail(SelectedReferenceTailTight::A, 'bad');
selected_reference_tail(SelectedReferenceTailLoose::A, 'bad');
interface SelectedReferenceUnion { public function value(int|string &$input): string; }
enum SelectedReferenceUnionTight implements SelectedReferenceUnion {
    case A; public function value(int|string &$input): string { return gettype($input); }
}
enum SelectedReferenceUnionLoose implements SelectedReferenceUnion {
    case A; public function value(mixed &$input): string { return gettype($input); }
}
function selected_reference_union(SelectedReferenceUnion $target, mixed $input): void {
    try { echo $target->value($input), gettype($input); }
    catch (TypeError $error) { echo $error->getMessage(); }
}
selected_reference_union(SelectedReferenceUnionTight::A, []);
selected_reference_union(SelectedReferenceUnionTight::A, true);
selected_reference_union(SelectedReferenceUnionLoose::A, []);
class SelectedArgumentLifetime {
    public function __toString(): string { return str_repeat('x', 3); }
    public function __destruct() { echo 'released'; }
}
interface SelectedLifetimeContract {
    public function reference(string|bool &$input): string;
    public function value(string|bool ...$items): string;
}
enum SelectedLifetimeTight implements SelectedLifetimeContract {
    case A;
    public function reference(string|bool &$input): string { return (string)$input; }
    public function value(string|bool ...$items): string { return 'value'; }
}
enum SelectedLifetimeLoose implements SelectedLifetimeContract {
    case A;
    public function reference(mixed &$input): string { return 'loose'; }
    public function value(mixed ...$items): string { return 'loose'; }
}
function selected_lifetime_source(): mixed { return new SelectedArgumentLifetime(); }
function selected_lifetime_args(): array { return [selected_lifetime_source()]; }
function selected_lifetime_call(SelectedLifetimeContract $target): void {
    $input = selected_lifetime_source();
    echo $target->reference($input), $target->value(selected_lifetime_source());
    echo $target->value(...selected_lifetime_args());
}
selected_lifetime_call(SelectedLifetimeTight::A);
interface UnpackEffects { public function value(int $a): int; }
enum UnpackEffectsValue implements UnpackEffects {
    case A; public function value(int $a): int { return $a; }
}
function unpack_effect(int $value): int { echo $value; return $value; }
function unpack_key(): string { return str_repeat('a', 1); }
function unpack_effect_call(UnpackEffects $target): int {
    return $target->value(...['a' => unpack_effect(1), 'a' => unpack_effect(2)]);
}
echo unpack_effect_call(UnpackEffectsValue::A);
function unpack_key_call(UnpackEffects $target): int {
    return $target->value(...[unpack_key() => unpack_effect(3)]);
}
echo unpack_key_call(UnpackEffectsValue::A);
function raw_callback_hash(): array {
    return ['callback' => function(): int { return 7; }, 'other' => 1];
}
$callbacks = raw_callback_hash(); echo ($callbacks['callback'])();
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    assert!(crate::ir::print_module(&module).contains("implicit_int_cast"), "{target}: diagnostic EIR cast");
    let callback = module.functions.iter().find(|function| function.name == "raw_callback_hash").unwrap();
    assert!(callback.instructions.iter().any(|instruction|
        instruction.op == crate::ir::Op::HashSet && callback.value(instruction.operands[2])
            .is_some_and(|value| value.php_type == crate::types::PhpType::Callable)),
        "{target}: mixed hash callback entries preserve their raw descriptor contract");
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("implicit_int_float"), "{target}: target-aware implicit int binding");
}

/// Covers the macOS ARM64 diagnostic and collector ABI.
#[test]
fn enum_argument_boundaries_macos() { verify("macos-aarch64"); }

/// Covers the iOS device diagnostic and collector ABI.
#[test]
fn enum_argument_boundaries_ios() { verify("ios-arm64"); }

/// Covers the iOS Simulator diagnostic and collector ABI.
#[test]
fn enum_argument_boundaries_ios_sim() { verify("ios-sim-arm64"); }

/// Covers the Linux ARM64 diagnostic and collector ABI.
#[test]
fn enum_argument_boundaries_linux_arm() { verify("linux-aarch64"); }

/// Covers the Linux x86_64 diagnostic and collector ABI.
#[test]
fn enum_argument_boundaries_linux_x86() { verify("linux-x86_64"); }
