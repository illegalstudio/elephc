//! Purpose:
//! Verifies independent ownership for eval-materialized public Reflection metadata mirrors.
//!
//! Called from:
//! - AST-to-EIR and codegen unit tests on every supported target.
//!
//! Key details:
//! - Mutable public `name` and `class` slots must not share string allocations with private
//!   Reflection metadata because unset and reference overwrite free the public payload directly.

/// Checks independent eval Reflection string stores for one bounded target compilation.
fn assert_eval_reflection_mirrors_persist_independent_strings(name: &str) {
    let source = r#"<?php
#[Attribute]
class ReflectionMirrorOwnershipAttribute {}
#[ReflectionMirrorOwnershipAttribute]
class ReflectionMirrorOwnershipTarget {
    public function action(string $value): void {}
}
eval('class ReflectionMirrorOwnershipEvalTrigger {}');
$method = new ReflectionMethod(ReflectionMirrorOwnershipTarget::class, 'action');
$parameter = $method->getParameters()[0];
$attribute = (new ReflectionClass(ReflectionMirrorOwnershipTarget::class))->getAttributes()[0];
echo $method->name, $parameter->name, $attribute->name;
try { echo ($method->name = 'changed'); }
catch (ReflectionException $error) { echo $error->getMessage(); }
function writeNullableReflectionOwnershipMirror(?ReflectionMethod $reflection): void {
    try { $reflection->name = 'changed'; }
    catch (Throwable $error) { echo $error->getMessage(); }
}
writeNullableReflectionOwnershipMirror($argc > 1 ? $method : null);
"#;
    let target = crate::codegen::platform::Target::parse(name).unwrap();
    let module = super::lower_source_at_for_target(
        source,
        std::path::Path::new("main.php"),
        std::path::Path::new("."),
        target,
    );
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    let attribute_label = format!(
        "{}:\n",
        target.extern_symbol("__elephc_eval_reflection_attribute_new")
    );
    let owner_label = format!(
        "{}:\n",
        target.extern_symbol("__elephc_eval_reflection_owner_new")
    );
    let attribute_body = asm
        .split_once(&attribute_label)
        .unwrap()
        .1
        .split_once(&owner_label)
        .unwrap()
        .0;
    assert_eq!(
        attribute_body.matches("__rt_str_persist").count(),
        2,
        "{name}: private and public attribute names need separate allocations"
    );
    let owner_body = asm.split_once(&owner_label).unwrap().1;
    for (class, property) in [
        ("ReflectionMethod", "__name"),
        ("ReflectionMethod", "name"),
        ("ReflectionMethod", "class"),
        ("ReflectionParameter", "name"),
    ] {
        let offset = module.class_infos[class].property_offsets[property];
        let copy_and_store = match target.arch {
            crate::codegen::platform::Arch::AArch64 => format!(
                "bl __rt_str_persist\n    ldr x9, [sp, #32]\n    str x1, [x9, #{offset}]\n    str x2, [x9, #{}]",
                offset + 8,
            ),
            crate::codegen::platform::Arch::X86_64 => format!(
                "call __rt_str_persist\n    mov r10, QWORD PTR [rbp - 40]\n    mov QWORD PTR [r10 + {offset}], rax\n    mov QWORD PTR [r10 + {}], rdx",
                offset + 8,
            ),
        };
        assert!(
            owner_body.contains(&copy_and_store),
            "{name}: {class}::${property} must store its own persisted string"
        );
    }
}

/// Eval Reflection mirrors own independent strings on macOS ARM64.
#[test]
fn eval_reflection_mirrors_persist_independent_strings_macos_aarch64() {
    assert_eval_reflection_mirrors_persist_independent_strings("macos-aarch64");
}

/// Eval Reflection mirrors own independent strings on iOS device ARM64.
#[test]
fn eval_reflection_mirrors_persist_independent_strings_ios_arm64() {
    assert_eval_reflection_mirrors_persist_independent_strings("ios-arm64");
}

/// Eval Reflection mirrors own independent strings on iOS Simulator ARM64.
#[test]
fn eval_reflection_mirrors_persist_independent_strings_ios_sim_arm64() {
    assert_eval_reflection_mirrors_persist_independent_strings("ios-sim-arm64");
}

/// Eval Reflection mirrors own independent strings on Linux ARM64.
#[test]
fn eval_reflection_mirrors_persist_independent_strings_linux_aarch64() {
    assert_eval_reflection_mirrors_persist_independent_strings("linux-aarch64");
}

/// Eval Reflection mirrors own independent strings on Linux x86_64.
#[test]
fn eval_reflection_mirrors_persist_independent_strings_linux_x86_64() {
    assert_eval_reflection_mirrors_persist_independent_strings("linux-x86_64");
}
