//! Purpose:
//! Verifies that native reference body writes expose their published PHP type.
//!
//! Called from:
//! - Supported-host native reference regressions in CI.
//!
//! Key details:
//! - The entry declaration remains int while a canonical cell can own a string child.
//! - Explicit native ownership checks cover both copied read values and the shared cell.

use super::native_reference_validation_support::*;
use crate::support::*;

/// A canonical scalar reference's new string value is visible before the native body returns.
#[test]
fn test_native_reference_body_write_uses_current_boxed_payload() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_body_view"));
    let source = r#"<?php
function nativeCanonicalBodyView(int &$target): void {
    $target = 'changed'; echo gettype($target), ':', $target, '|';
}
if ($argc < 0) { $target = 1; nativeCanonicalBodyView($target); unset($target); }
"#;
    for optimize in [false, true] {
        let compile =
            || compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true);
        let (user, runtime, requirements) = if optimize {
            compile()
        } else {
            without_ir_opt(compile)
        };
        let mut asm = String::new();
        instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
        instruction(&mut asm, "mov x0, #0", "xor eax, eax");
        instruction(&mut asm, "mov x1, #41", "mov edi, 41");
        instruction(&mut asm, "mov x2, #0", "xor esi, esi");
        call(&mut asm, "__rt_mixed_from_value");
        store(&mut asm, 16);
        call(&mut asm, "__rt_reference_new");
        store(&mut asm, 0);
        load(&mut asm, 16);
        call(&mut asm, "__rt_decref_mixed");
        instruction(&mut asm, "mov x0, #7", "mov edi, 7");
        call(&mut asm, "__rt_reference_cell_new");
        store(&mut asm, 8);
        instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
        instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        call(
            &mut asm,
            &elephc::names::function_symbol("nativeCanonicalBodyView"),
        );
        load(&mut asm, 8);
        call(&mut asm, "__rt_reference_cell_release");
        expect_zero(&mut asm, "_rt_reference_validation_pins_top");
        expect_zero(&mut asm, "_exc_value");
        expect_zero(&mut asm, "_gc_live");
        append_status_output(&mut asm, 32);
        let user = inject_main_exit_harness(&user, &asm);
        let runtime = runtime_obj_for_asm(&runtime);
        let output = assemble_and_run(
            &user,
            &runtime,
            &dir.0,
            &requirements,
            &default_link_paths(),
            &[],
        );
        assert_eq!(output, "string:changed|1", "optimize={optimize}");
    }
}
