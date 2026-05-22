//! Purpose:
//! Codegen regression tests for runtime-managed SPL/core object intrinsic calls.
//! Verifies direct method interception routes through the shared intrinsic registry.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - These tests inspect user assembly so the direct runtime-helper path stays visible.

use crate::support::*;

fn compile_intrinsic_fixture(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_intrinsic_asm");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    user_asm
}

fn direct_runtime_call(label: &str) -> String {
    match target().arch {
        Arch::AArch64 => format!("bl {}", label),
        Arch::X86_64 => format!("call {}", label),
    }
}

#[test]
fn test_generator_method_routes_through_intrinsic_runtime_helper() {
    let user_asm = compile_intrinsic_fixture(
        r#"<?php
function nums() {
    yield 7;
}
$g = nums();
echo $g->current();
"#,
    );

    let expected = direct_runtime_call("__rt_gen_current");
    assert!(
        user_asm.contains(&expected),
        "expected direct Generator intrinsic call `{}` in:\n{}",
        expected,
        user_asm
    );
}

#[test]
fn test_fiber_static_and_instance_methods_route_through_intrinsics() {
    let user_asm = compile_intrinsic_fixture(
        r#"<?php
$f = new Fiber(function(): void {
    Fiber::suspend("ready");
});
echo $f->start();
"#,
    );

    for label in ["__rt_fiber_suspend", "__rt_fiber_start"] {
        let expected = direct_runtime_call(label);
        assert!(
            user_asm.contains(&expected),
            "expected direct Fiber intrinsic call `{}` in:\n{}",
            expected,
            user_asm
        );
    }
}
