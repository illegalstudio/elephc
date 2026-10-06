//! Purpose:
//! Checks delayed static receiver evaluation and ownership on all supported targets.
//!
//! Called from:
//! - The EIR lowering unit test harness.
//!
//! Key details:
//! - A plain assignment evaluates an effectful RHS before loading its static receiver.
//! - Nullable writes retain a checked object payload and throw catchable null Errors.
//! - Indirect appends delay pure receiver traversal without delaying computed dimensions.

/// Emits the shared fixture and checks the RHS precedes the concrete static receiver load.
fn verify(target: &str) {
    let source = r#"<?php
class O { public int $v = 1; public array $items = [1]; public array $nested = [[1]]; }
class Root { public O $child; public function __construct() { $this->child = new O(); } }
class C { public static O $o; public static ?O $nullable = null; }
class R { public static Root $root; }
function replace(): int { C::$o = new O(); return 9; }
function write(): void { C::$o->v = replace(); }
function writeNested(): void { C::$o->nested[0][] = replace(); }
function writeChild(): void { R::$root->child->items[] = replace(); }
function writeNull(): void { C::$nullable->v = replace(); }
interface Store { public function get(): string; }
interface Named { public function name(): string; }
class Both implements Store, Named {
    public function get(): string { return 'V'; }
    public function name(): string { return 'N'; }
}
class InterfaceHolder { public static ?Store $value = null; }
function dispatch(): void {
    $value = InterfaceHolder::$value;
    if ($value !== null) {
        echo $value->get();
        if ($value instanceof Named) { echo $value->name(); }
    }
}
C::$o = new O(); C::$nullable = new O();
R::$root = new Root();
write(); writeNested(); writeChild();
try { writeNull(); } catch (Error $error) {}
InterfaceHolder::$value = new Both(); dispatch();
C::$o->items[0] = replace();
$name = 'v'; C::$o->$name = replace();
C::$nullable->items[0] = 9;
C::$nullable->items[] = 3;
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    for name in ["write", "writeNested", "writeChild", "writeNull"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let rhs = function.instructions.iter().position(|inst| inst.op == crate::ir::Op::Call).unwrap();
        let receiver = function.instructions.iter().position(|inst| inst.op == crate::ir::Op::LoadStaticProperty).unwrap();
        assert!(rhs < receiver, "{target}: {name} RHS must precede receiver fetch");
        if name == "write" {
            assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::Acquire));
        }
        if name == "writeNull" {
            assert!(function.blocks.iter().any(|block| block.name == "property.write.null"));
            assert!(function.blocks.iter().any(|block|
                matches!(block.terminator, Some(crate::ir::Terminator::Throw { .. }))));
            assert!(function.instructions.iter().any(|inst| inst.op == crate::ir::Op::MixedUnbox));
        }
    }
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Schedules each supported target independently under the CI timeout envelope.
macro_rules! receiver_target_test {
    ($name:ident, $target:literal) => {
        /// Checks receiver evaluation and array mutation emission for this target.
        #[test]
        fn $name() { verify($target); }
    };
}

receiver_target_test!(static_receiver_review_macos, "macos-aarch64");
receiver_target_test!(static_receiver_review_ios, "ios-arm64");
receiver_target_test!(static_receiver_review_ios_sim, "ios-sim-arm64");
receiver_target_test!(static_receiver_review_linux_arm, "linux-aarch64");
receiver_target_test!(static_receiver_review_linux_x86, "linux-x86_64");
