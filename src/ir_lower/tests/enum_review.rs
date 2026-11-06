//! Purpose:
//! Checks enum trait constants, adapted contracts and runtime argument binding on every target.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Each target is scheduled independently and emits its complete assembly.
//! - Final enum self returns satisfy late-static trait and interface requirements.
//! - Promoted variadics preserve declared hints/counts and Iterable receives raw payload storage.

/// Lowers and emits a single target's enum contract fixture.
fn verify_enum_review_target(target: &str) {
    let source = r#"<?php
namespace EnumReview;
interface BuiltinFactory extends \BackedEnum {}
enum BuiltinText: string implements BuiltinFactory { case Word = 'word'; }
enum BuiltinNumber: int implements BuiltinFactory { case Seven = 7; }
echo BuiltinText::from('word')->value, BuiltinNumber::from(7)->value;
interface RedeclaredFactory extends \BackedEnum { public static function from(int|string $value): static; }
enum RedeclaredText: string implements RedeclaredFactory { case Word = 'word'; }
trait FactoryRequirement { abstract public static function from(int|string $value): static; }
enum TraitFactoryText: string { use FactoryRequirement; case Word = 'word'; }
echo RedeclaredText::from('word')->value, TraitFactoryText::from('word')->value;
trait Requirement {
    const OWNER = __CLASS__;
    const TRAIT_NAME = __TRAIT__;
    abstract public function current(): static;
    abstract public function count(int $x): int;
}
trait Adapted { use Requirement { count as protected; } }
interface ParentContract { public function label(): string; }
interface Contract extends ParentContract { public static function make(): static; }
enum E implements Contract {
    use Adapted;
    case A;
    public function current(): self { return $this; }
    protected function count(int $x = 1, int $extra = 2): int { return $x + $extra; }
    public function label(): string { return 'value:' . $this->count(); }
    public static function make(): self { return self::A; }
}
function describe(ParentContract $value): string { return $value->label(); }
echo describe(E::make()), E::A->current()->name, E::OWNER, E::TRAIT_NAME;
interface OptionalContract { public function f(int $x): string; }
enum OptionalValue implements OptionalContract {
    case A;
    const DATA = ['v'];
    public function f(int $x, string $prefix = 'ok', array $data = self::DATA): string {
        return $prefix . $data[0] . $x;
    }
}
function render(OptionalContract $value): string { return $value->f(2); }
echo render(OptionalValue::A);
interface RefContract { public function &f(int &$value): int; }
enum RefValue implements RefContract {
    case A;
    public function &f(int &$value, int $extra = 2): int { $value += $extra; return $value; }
}
function update(RefContract $enum, int &$value): int {
    $r = &$enum->f($value); $r += 10; return $r;
}
$value = 3;
echo update(RefValue::A, $value), $value;
interface ArrayRefContract { public function &data(): array; }
enum ArrayRefValue implements ArrayRefContract {
    case A;
    public function &data(array $extra = [2]): array { $value = [1]; $value[0] += $extra[0]; return $value; }
}
function update_array(ArrayRefContract $enum): int {
    $r = &$enum->data(); $r[0] += 10; return $r[0];
}
echo update_array(ArrayRefValue::A);
interface AddedRefContract { public function data(): array; }
enum AddedRefValue implements AddedRefContract {
    case A;
    public function &data(int $extra = 1): array { $value = [1 + $extra]; return $value; }
}
function read_array(AddedRefContract $enum): int { $value = $enum->data(); return $value[0]; }
echo read_array(AddedRefValue::A);
interface VariadicContract { public function f(int $x): int; }
enum VariadicValue implements VariadicContract {
    case A;
    public function f(int $x, int ...$rest): int { foreach ($rest as $value) { $x += $value; } return $x; }
}
function invoke(VariadicContract $value): int { return $value->f(1); }
echo invoke(VariadicValue::A);
interface VariadicRefContract { public function values(int ...$values): array; }
enum VariadicRefValue implements VariadicRefContract {
    case A;
    public function &values(int ...$values): array { return $values; }
}
function read_variadic(VariadicRefContract $value): int { $values = $value->values(1, 2); return $values[1]; }
echo read_variadic(VariadicRefValue::A);
interface DefaultContract { public function f(int $x): int; }
enum DefaultValue implements DefaultContract {
    case A;
    public function f(int $x = 10, int $y = 2): int { return $x + $y; }
}
function read_default(DefaultContract $value): int { return $value->f(); }
echo read_default(DefaultValue::A);
interface CapturedOptionalContract { public function f(int $x = 1): int; }
enum CapturedOptionalValue implements CapturedOptionalContract {
    case A;
    public function f(int $x = 10, int $y = 2): int { return $x + $y; }
}
function read_optional(CapturedOptionalContract $value): int { return $value->f(); }
echo read_optional(CapturedOptionalValue::A);
interface PrefixContract { public function f(int ...$values): int; }
enum PrefixValue implements PrefixContract {
    case A;
    public function f(int $first = 7, int ...$values): int {
        foreach ($values as $value) { $first += $value; } return $first;
    }
}
function read_prefix(PrefixContract $value): int { return $value->f(1, 2); }
echo read_prefix(PrefixValue::A);
interface HeterogeneousContract { public function f(int $x = 1): int; }
enum WideValue implements HeterogeneousContract {
    case A; public function f(int $x = 10, int $y = 2): int { return $x + $y; }
}
enum ShortValue implements HeterogeneousContract {
    case A; public function f(int $x = 20): int { return $x; }
}
function heterogeneous_named(HeterogeneousContract $value): int { return $value->f(y: 4); }
function heterogeneous_positional(HeterogeneousContract $value): int { return $value->f(4, 6); }
echo heterogeneous_named(WideValue::A), heterogeneous_positional(ShortValue::A);
try { echo heterogeneous_named(ShortValue::A); } catch (\Error $error) { echo $error->getMessage(); }
interface FixedAbsorbContract { public function f(int $x, int $y): int; }
enum FixedAbsorbValue implements FixedAbsorbContract {
    case A; public function f(int ...$rest): int { return count($rest); }
}
function fixed_absorb(FixedAbsorbContract $value): int { return $value->f(1, 2); }
echo fixed_absorb(FixedAbsorbValue::A);
interface PrefixAbsorbContract { public function f(int $x, int ...$rest): int; }
enum PrefixAbsorbValue implements PrefixAbsorbContract {
    case A; public function f(int ...$rest): int { return count($rest); }
}
function prefix_absorb(PrefixAbsorbContract $value): int { return $value->f(1, 2, 3); }
echo prefix_absorb(PrefixAbsorbValue::A);
interface RequiredContract { public function f(int $x): int; }
enum OptionalRequiredValue implements RequiredContract {
    case A; public function f(int $x = 1, int $y = 2): int { return $x + $y; }
}
enum RequiredValue implements RequiredContract {
    case A; public function f(int $x, int $y = 3): int { return $x + $y; }
}
function missing_positional(RequiredContract $value): int { return $value->f(); }
function missing_named(RequiredContract $value): int { return $value->f(y: 4); }
echo missing_positional(OptionalRequiredValue::A), missing_named(OptionalRequiredValue::A);
try { missing_positional(RequiredValue::A); } catch (\ArgumentCountError $error) { echo $error->getMessage(); }
try { missing_named(RequiredValue::A); } catch (\ArgumentCountError $error) { echo $error->getMessage(); }
function dynamic_unpack(RequiredContract $value, array $args): int { return $value->f(...$args); }
try { dynamic_unpack(RequiredValue::A, []); } catch (\ArgumentCountError $error) { echo $error->getMessage(); }
echo dynamic_unpack(RequiredValue::A, [4]), dynamic_unpack(RequiredValue::A, ['x' => 4]);
interface LaterRequiredContract { public function f(int $a, int $b): int; }
enum LaterRequiredValue implements LaterRequiredContract {
    case A; public function f(int $a = 1, int $b): int { return $a + $b; }
}
function later_required(LaterRequiredContract $value): int { return $value->f(1); }
function later_required_bad(LaterRequiredContract $value): int { return $value->f('no'); }
try { later_required(LaterRequiredValue::A); } catch (\ArgumentCountError $error) { echo $error->getMessage(); }
try { later_required_bad(LaterRequiredValue::A); } catch (\TypeError $error) { echo $error->getMessage(); }
interface UnpackReferenceContract { public function &f(int $x): array; }
enum UnpackReferenceValue implements UnpackReferenceContract {
    case A; public function &f(int $x = 1): array { $a = [$x]; return $a; }
}
function reference_unpack(UnpackReferenceContract $value, array $args): int {
    $result = &$value->f(...$args); $result[0] += 10; return $result[0];
}
echo reference_unpack(UnpackReferenceValue::A, []), reference_unpack(UnpackReferenceValue::A, [2]);
interface TransportContract { public function f(int $x, int ...$rest): int; }
enum TransportValue implements TransportContract {
    case A; public function f(int $x = 1, int ...$rest): int {
        echo func_num_args(); foreach ($rest as $item) { $x += $item; } return $x;
    }
}
function transport_unpack(TransportContract $value, array $args): int { return $value->f(...$args); }
echo transport_unpack(TransportValue::A, [1, 2, 'extra' => 3]);
interface IterableContract { public function f(iterable $value): int; }
enum IterableValue implements IterableContract {
    case A; public function f(iterable $value): int {
        $sum = 0; foreach ($value as $item) { $sum += $item; } return $sum;
    }
}
function iterable_unpack(IterableContract $value, array $args): int { return $value->f(...$args); }
echo iterable_unpack(IterableValue::A, [[1, 2]]);
try { iterable_unpack(IterableValue::A, [1]); } catch (\TypeError $error) { echo $error->getMessage(); }
interface FalseContract { public function f(false $value): false; }
enum FalseValue implements FalseContract { case A; public function f(false $value): false { return $value; } }
function false_unpack(FalseContract $value, array $args): false { return $value->f(...$args); }
try { false_unpack(FalseValue::A, [true]); } catch (\TypeError $error) { echo $error->getMessage(); }
function spread_bad(): string { echo 'Y'; return 'no'; }
function static_shortage(LaterRequiredContract $value): int { return $value->f(...[spread_bad()]); }
try { static_shortage(LaterRequiredValue::A); } catch (\TypeError $error) { echo $error->getMessage(); }
function keyed_shortage(LaterRequiredContract $value): int { return $value->f(...[1 => spread_bad()]); }
function named_shortage(LaterRequiredContract $value): int { return $value->f(...['a' => spread_bad()]); }
function named_gap(LaterRequiredContract $value): int { return $value->f(...['b' => spread_bad()]); }
function fractional_shortage(LaterRequiredContract $value): int { return $value->f(...[1.5 => spread_bad()]); }
try { keyed_shortage(LaterRequiredValue::A); } catch (\TypeError $error) { echo $error->getMessage(); }
try { named_shortage(LaterRequiredValue::A); } catch (\TypeError $error) { echo $error->getMessage(); }
try { named_gap(LaterRequiredValue::A); } catch (\ArgumentCountError $error) { echo $error->getMessage(); }
try { fractional_shortage(LaterRequiredValue::A); } catch (\TypeError $error) { echo $error->getMessage(); }
trait SameOwner { const OWNER = __CLASS__; }
class SameClass { use SameOwner; const OWNER = __CLASS__; }
enum SameEnum { use SameOwner; const OWNER = __CLASS__; case A; }
echo SameClass::OWNER, SameEnum::OWNER;
interface RequiredPositionContract { public function f(int $a = 1, int $b): int; }
enum RequiredPositionValue implements RequiredPositionContract {
    case A; public function f(int $a, int $b): int { return $a + $b; }
}
interface ScalarUnionContract { public function f(int|float $value): mixed; }
enum ScalarUnionValue implements ScalarUnionContract {
    case A; public function f(int|float $value): mixed { return $value; }
}
function scalar_union_unpack(ScalarUnionContract $value, array $args): mixed { return $value->f(...$args); }
echo scalar_union_unpack(ScalarUnionValue::A, [true]);
debug_print_backtrace();
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let variadic = &module.class_infos["EnumReview\\VariadicValue"].methods["f"];
    let factory = &module.class_infos["EnumReview\\TraitFactoryText"];
    assert_eq!(factory.static_methods["from"].params[0].1, crate::types::PhpType::Str,
        "{target}: public contract validation must not widen the concrete factory ABI");
    assert!(!factory.method_decls.iter().any(|method| method.name.eq_ignore_ascii_case("from")),
        "{target}: a trait requirement must not replace the synthesized factory");
    assert_eq!(crate::types::signatures::variadic_source_element_type_expr(variadic),
        Some(&crate::parser::ast::TypeExpr::Int));
    assert_eq!(variadic.params.last().unwrap().1,
        crate::types::PhpType::Array(Box::new(crate::types::PhpType::Mixed)));
    let transported = &module.class_infos["EnumReview\\TransportValue"];
    let transport_signature = &transported.methods["f"];
    assert_eq!(crate::types::signatures::variadic_source_element_type_expr(transport_signature),
        Some(&crate::parser::ast::TypeExpr::Int));
    assert_eq!(transport_signature.params.last().unwrap().1,
        crate::types::PhpType::Array(Box::new(crate::types::PhpType::Mixed)));
    assert!(crate::func_args::sig_has_hidden_argc_param(transport_signature));
    let wrapper = crate::names::interface_method_wrapper_symbol(transported.class_id,
        module.interface_infos["EnumReview\\TransportContract"].interface_id, "f");
    assert!(module.functions.iter().any(|function| function.name == wrapper),
        "{target}: promoted equally variadic transport must have an EIR reboxing adapter");
    let iterable = module.functions.iter().find(|function| function.name == "EnumReview\\iterable_unpack")
        .expect("missing iterable-unpack fixture");
    assert!(iterable.instructions.iter().any(|instruction| instruction.op == crate::ir::Op::MixedUnbox
        && instruction.result_php_type == crate::types::PhpType::Iterable),
        "{target}: Iterable receives an owned raw container rather than a Mixed cell");
    let unpack = module.functions.iter().find(|function| function.name == "EnumReview\\dynamic_unpack")
        .expect("missing dynamic-unpack fixture");
    assert!(unpack.blocks.iter().flat_map(|block| &block.instructions)
        .any(|id| unpack.instruction(*id).is_some_and(|inst| inst.op == crate::ir::Op::MethodCall)),
        "{target}: dynamic PHP arrays bind before the ordinary selected-method ABI");
    assert!(!unpack.blocks.iter().flat_map(|block| &block.instructions)
        .any(|id| unpack.instruction(*id).is_some_and(|inst| inst.op == crate::ir::Op::CallableDescriptorInvoke)),
        "{target}: selected method calls must not use call_user_func_array semantics");
    let union = module.functions.iter().find(|function| function.name == "EnumReview\\scalar_union_unpack")
        .expect("missing scalar-union fixture");
    assert!(union.instructions.iter().any(|instruction|
        instruction.op == crate::ir::Op::MixedNumericBinop
            && instruction.immediate == Some(crate::ir::Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Add))),
        "{target}: weak numeric union binding uses PHP numeric kind selection");
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

/// Schedules each complete target-specific enum fixture separately.
macro_rules! enum_review_target_test {
    ($name:ident, $target:literal) => {
        /// Verifies enum trait and interface contracts on this supported target.
        #[test]
        fn $name() { verify_enum_review_target($target); }
    };
}

enum_review_target_test!(enum_review_macos, "macos-aarch64");
enum_review_target_test!(enum_review_ios, "ios-arm64");
enum_review_target_test!(enum_review_ios_sim, "ios-sim-arm64");
enum_review_target_test!(enum_review_linux_arm, "linux-aarch64");
enum_review_target_test!(enum_review_linux_x86, "linux-x86_64");
