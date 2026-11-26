//! Purpose:
//! Keeps boxed array replacements compatible with native HTTP global storage.
//!
//! Called from:
//! - The AST-to-EIR unit suite.
//!
//! Key details:
//! - Request-global stores and later hash reads must agree on every target ABI.

use crate::ir::{Immediate, IrHeapKind, IrType, Op};

/// Main and function assignments publish hash payloads instead of boxed array cells.
#[test]
fn boxed_web_superglobal_assignments_keep_hash_storage_on_all_targets() {
    let source = r#"<?php
function empty_request_array(): array { return array(); }
function request_array_input(mixed $value): array {
    if (!is_array($value)) return array();
    return (array) $value;
}
function merge_request_arrays(array $arrays): array {
    $result = empty_request_array();
    foreach ($arrays as $value) {
        $source = request_array_input($value);
        foreach ($source as $key => $item) {
            if (is_int($key)) $result[] = $item;
            else $result[(string) $key] = $item;
        }
    }
    return $result;
}
function replace_request(mixed $values): void {
    $_SERVER = $values;
    $_REQUEST = $values;
    echo count($_REQUEST);
}
$_GET = json_decode('{"probe":"ok"}', true);
replace_request(merge_request_arrays(array($_POST)));
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let program = crate::name_resolver::resolve(program).unwrap();
        let checked = crate::types::check_with_target(&program, target).unwrap();
        let module = crate::ir_lower::lower_program(&program, &checked, target, true).unwrap();
        let mut stores = 0;
        let mut request_layout_conversions = 0;
        for function in &module.functions {
            for inst in function.instructions.iter().filter(|inst| inst.op == Op::StoreGlobal) {
                let Some(Immediate::GlobalName(global)) = inst.immediate else { continue; };
                let global_name = &module.data.global_names[global.as_raw() as usize];
                if !crate::superglobals::is_superglobal(global_name) {
                    continue;
                }
                let value = function.value(inst.operands[0]).unwrap();
                assert_eq!(value.ir_type, IrType::Heap(IrHeapKind::Hash), "{name}: {}", function.name);
                assert_eq!(value.php_type, crate::superglobals::superglobal_type(), "{name}");
                if global_name == "_REQUEST" {
                    assert!(function.instructions.iter().any(|candidate| candidate.op == Op::MixedUnbox),
                        "{name}: request assignment unboxes the boxed PHP array");
                    assert!(function.instructions.iter().any(|candidate| candidate.op == Op::ArrayToHash),
                        "{name}: request assignment selects the runtime array layout");
                    request_layout_conversions += 1;
                }
                stores += 1;
            }
        }
        assert_eq!(stores, 3, "{name}: cover main and function request-global assignments");
        assert_eq!(request_layout_conversions, 1, "{name}: cover the request merge assignment");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
