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
function nativeCanonicalReplacement(): mixed { return 'changed'; }
function nativeCanonicalBodyView(int &$target): void {
    $target = nativeCanonicalReplacement(); $target[1] = 'X';
    $byte = ($target[2] = 'Y'); echo gettype($target), ':', $target, ':', $byte, '|';
}
function nativeCanonicalArrayReplacement(): mixed { return [0]; }
function nativeCanonicalArrayMutation(int &$target): void {
    $target = nativeCanonicalArrayReplacement(); $target[] = 1; $target[1] = 2;
    echo $target[0], ':', count($target), '|';
    $target[0] = []; $target[0]['key'] = 3; $target[0]['key'] += 1; $target[0][] = 4;
    echo $target[0]['key'], ':', $target[0][0], ':', count($target[0]), '|';
}
function nativeHeaderlessStringMutation(string &$target): void {
    $target = nativeCanonicalReplacement(); $target[1] = 'X';
    $byte = ($target[2] = 'Y'); echo gettype($target), ':', $target, ':', $byte, '|';
}
if ($argc < 0) {
    $target = 1; nativeCanonicalBodyView($target); nativeCanonicalArrayMutation($target); unset($target);
    $raw = 'before'; nativeHeaderlessStringMutation($raw); unset($raw);
}
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
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        call(&mut asm, &elephc::names::function_symbol("nativeCanonicalArrayMutation"));
        address(&mut asm, "x9", "r10", "Lnative_headerless_string");
        instruction(&mut asm, "str x9, [sp, #16]", "mov QWORD PTR [rsp + 16], r10");
        instruction(&mut asm, "mov x9, #6", "mov r10, 6");
        instruction(&mut asm, "str x9, [sp, #24]", "mov QWORD PTR [rsp + 24], r10");
        instruction(&mut asm, "add x0, sp, #16", "lea rdi, [rsp + 16]");
        call(&mut asm, &elephc::names::function_symbol("nativeHeaderlessStringMutation"));
        load(&mut asm, 16);
        call(&mut asm, "__rt_heap_free_safe");
        load(&mut asm, 8);
        call(&mut asm, "__rt_reference_cell_release");
        expect_zero(&mut asm, "_rt_reference_validation_pins_top");
        expect_zero(&mut asm, "_exc_value");
        expect_zero(&mut asm, "_gc_live");
        append_status_output(&mut asm, 32);
        let mut user = inject_main_exit_harness(&user, &asm);
        user.push_str("\n.data\nLnative_headerless_string:\n    .byte 98,101,102,111,114,101,0\n");
        let runtime = runtime_obj_for_asm(&runtime);
        let output = assemble_and_run(
            &user,
            &runtime,
            &dir.0,
            &requirements,
            &default_link_paths(),
            &[],
        );
        assert_eq!(output, "string:cXYnged:Y|0:2|4:4:2|string:cXYnged:Y|1", "optimize={optimize}");
    }
}

/// Opaque eval keeps raw Mixed-element array references writable after logical widening.
#[test]
fn test_native_reference_array_mutations_after_opaque_eval_bundle() {
    let source = r#"<?php
function nativeRawArrayMutation(array<mixed> &$items, string $code): void {
    eval($code); $items[] = 1; $items[1] = 2;
    echo $items[0], ':', $items[1], ':', count($items), '|';
}
function nativeRawNestedArrayMutation(array<mixed> &$items, string $code): void {
    eval($code); $items[0] = []; $items[0]['key'] = 3; $items[0]['key'] += 1; $items[0][] = 4;
    echo $items[0]['key'], ':', $items[0][0], ':', count($items[0]), '|';
}
$items = [0]; $code = $argc > 0 ? '$items[0] = 0;' : '';
nativeRawArrayMutation($items, $code);
echo $items[0], ':', $items[1], ':', count($items);
echo '|'; nativeRawNestedArrayMutation($items, $code);
echo $items[0]['key'], ':', $items[0][0], ':', count($items[0]);
"#;
    for optimize in [false, true] {
        let run = || compile_and_run_with_heap_debug(source);
        let output = if optimize { run() } else { without_ir_opt(run) };
        assert!(output.success, "optimize={optimize}: {}", output.stderr);
        assert_eq!(output.stdout, "0:2:2|0:2:2|4:4:2|4:4:2", "optimize={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "optimize={optimize}: {}", output.stderr);
    }
}

/// A raw string reference keeps its two-word ABI through the same boxed logical value flow.
#[test]
fn test_native_reference_raw_string_offset_write_preserves_caller_alias() {
    let source = r#"<?php
function nativeRawStringReplacement(): mixed { return 'abc'; }
function nativeRawStringMutation(string &$text): void {
    $text = nativeRawStringReplacement(); $text[1] = 'Z';
    $byte = ($text[2] = 'Y'); echo gettype($text), ':', $text, ':', $byte, '|';
}
$text = 'before'; $alias =& $text;
nativeRawStringMutation($text);
echo $text, ':', $alias;
"#;
    for optimize in [false, true] {
        let run = || compile_and_run_with_heap_debug(source);
        let output = if optimize { run() } else { without_ir_opt(run) };
        assert!(output.success, "optimize={optimize}: {}", output.stderr);
        assert_eq!(output.stdout, "string:aZY:Y|aZY:aZY", "optimize={optimize}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "optimize={optimize}: {}", output.stderr);
    }
}
