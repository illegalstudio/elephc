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
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
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
