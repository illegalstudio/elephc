//! Purpose:
//! Checks strip_tags array normalization and temporary owners on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR regression tests.
//!
//! Key details:
//! - A boxed allow-list must use checked dense values, not the hash payload directly.
//! - Normalized array owners retire after the join and before the string result returns.
//! - Per-file strict scalar binding survives the typed runtime-call boundary.

/// Generates one target's boxed array allow-list path and checks its conversion boundary.
fn verify(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
function stripWith(mixed $allow): string {
    return strip_tags('<p>A</p><b>B</b>', $allow);
}
class AllowedTag { public function __toString(): string { return '<p>'; } }
class ThrowingTag { public function __toString(): string { throw new Exception('tag'); } }
echo stripWith(['key' => 'p']);
echo stripWith(new AllowedTag());
try { echo stripWith([new ThrowingTag()]); } catch (Throwable $error) { echo 'caught'; }
echo stripWith([str_repeat('p', 70000 + $argc)]);
echo strip_tags('<>x', []), strip_tags('<>x', ['']);
echo strip_tags('<>x', [null]), strip_tags('<>x', [false]);
echo strip_tags('<>x', ['tag' => '']);
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    assert!(assembly.contains("__rt_array_to_mixed"), "{target}: normalize packed scalar slots");
    assert!(assembly.contains("__rt_decref_array"), "{target}: retire the normalized owner");
    assert!(assembly.contains("__rt_strip_tags"), "{target}");
    assert!(assembly.contains("__rt_cleanup_call_operand_owner"), "{target}: protect normalized owners over callbacks");
    assert!(assembly.contains("__rt_heap_free_safe"), "{target}: retire owned allow-list strings");
    if target.ends_with("x86_64") {
        assert!(assembly.lines().collect::<Vec<_>>().windows(2).any(|pair|
            pair[0].contains("test r8, r8")
                && pair[1].contains("strip_tags_allow_wrap_done")),
            "{target}: wrap according to array count, not rendered string length");
    } else {
        assert!(assembly.lines().any(|line|
            line.contains("cbz x5,") && line.contains("strip_tags_allow_wrap_done")),
            "{target}: wrap according to array count, not rendered string length");
    }
    let wrapper = module.functions.iter().find(|function|
        function.name.eq_ignore_ascii_case("stripWith")).unwrap();
    assert!(wrapper.instructions.iter().any(|instruction|
        instruction.op == crate::ir::Op::Cast
            && instruction.result_php_type == crate::types::PhpType::Str),
        "{target}: Stringable conversion must be explicit EIR, not hidden backend ownership");

    let strict_module = super::lower_source_at_for_target(r#"<?php
declare(strict_types=1);
function stripStrict(mixed $allow): string {
    return strip_tags('<b>z</b>', $allow);
}
function invokeStrict(callable $f, mixed $allow): string {
    return $f('<b>z</b>', $allow);
}
try { echo stripStrict(1); } catch (TypeError $error) { echo 'caught'; }
try { echo invokeStrict(strip_tags(...), 1); } catch (TypeError $error) { echo 'caught'; }
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen_support::platform::Target::parse(target).unwrap());
    assert!(strict_module.functions.iter().flat_map(|function| &function.instructions).any(|inst|
        matches!(inst.immediate, Some(crate::ir::Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::ProfiledFunction {
                target: crate::ir::RuntimeFnId::StripTags, strict_types: Some(true), ..
            }
        )))
    ), "{target}: preserve strict caller binding");
    let descriptor_caller = strict_module.functions.iter().find(|function|
        function.name.eq_ignore_ascii_case("invokeStrict")).unwrap();
    let invoke = descriptor_caller.instructions.iter().find(|inst|
        inst.op == crate::ir::Op::CallableDescriptorInvoke).unwrap();
    assert_eq!(invoke.operands.len(), 3, "{target}: attach invocation-source strictness");
    let crate::ir::ValueDef::Instruction { inst, .. } =
        descriptor_caller.value(invoke.operands[2]).unwrap().def else { panic!("{target}"); };
    assert_eq!(descriptor_caller.instruction(inst).unwrap().immediate,
        Some(crate::ir::Immediate::Bool(true)), "{target}: strict source, not creation profile");
    crate::codegen::generate_user_asm_from_ir(&strict_module, false, false).unwrap();
}

/// Schedules each supported target independently under the CI timeout.
macro_rules! target_test {
    ($name:ident, $target:literal) => {
        /// Checks boxed allow-list normalization for this supported target.
        #[test]
        fn $name() { verify($target); }
    };
}

target_test!(strip_tags_review_macos, "macos-aarch64");
target_test!(strip_tags_review_ios, "ios-arm64");
target_test!(strip_tags_review_ios_sim, "ios-sim-arm64");
target_test!(strip_tags_review_linux_arm, "linux-aarch64");
target_test!(strip_tags_review_linux_x86, "linux-x86_64");
