//! Purpose:
//! Checks enum trait constants and adapted contracts on every supported target.
//!
//! Called from:
//! - The AST-to-EIR unit test harness.
//!
//! Key details:
//! - Each target is scheduled independently and emits its complete assembly.
//! - Final enum self returns satisfy late-static trait and interface requirements.

/// Lowers and emits a single target's enum contract fixture.
fn verify_enum_review_target(target: &str) {
    let source = r#"<?php
namespace EnumReview;
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
debug_print_backtrace();
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let variadic = &module.class_infos["EnumReview\\VariadicValue"].methods["f"];
    assert_eq!(crate::types::signatures::variadic_source_element_type_expr(variadic),
        Some(&crate::parser::ast::TypeExpr::Int));
    assert_eq!(variadic.params.last().unwrap().1,
        crate::types::PhpType::Array(Box::new(crate::types::PhpType::Mixed)));
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
