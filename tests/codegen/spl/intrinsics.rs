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

/// Compiles PHP source to assembly and returns only the user-code assembly string.
///
/// The fixture is compiled with a large stack size (8 MiB) and with intrinsics
/// extraction enabled so the user assembly shows direct runtime helper calls.
/// The temporary directory is cleaned up after extraction.
fn compile_intrinsic_fixture(source: &str) -> String {
    let dir = make_cli_test_dir("elephc_intrinsic_asm");
    let (user_asm, _runtime_asm, _required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    user_asm
}

/// Returns the target-specific CALL instruction for a runtime helper label.
///
/// AArch64 uses `bl` (branch-with-link) while x86_64 uses `call`. This helper
/// abstracts the architecture difference so assertions remain target-agnostic.
fn direct_runtime_call(label: &str) -> String {
    match target().arch {
        Arch::AArch64 => format!("bl {}", label),
        Arch::X86_64 => format!("call {}", label),
    }
}

/// Verifies that `Generator::current()` routes through `__rt_gen_current`.
///
/// Regression test: ensures the codegen path for generator methods remains a
/// direct call to the runtime helper rather than an indirect/fallback route.
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

/// Verifies that `Fiber::suspend()` and `Fiber::start()` both route through
/// their respective runtime helpers.
///
/// Regression test: ensures the codegen path for fiber static and instance
/// methods remains direct and is not broken by future refactoring of the
/// shared intrinsic registry.
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
