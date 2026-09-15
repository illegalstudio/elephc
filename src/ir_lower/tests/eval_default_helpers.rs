//! Purpose:
//! Structural regression coverage for compiled eval native-default helpers.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Deep finite literals use ordinary EIR lowering on every supported target.
//! - Constructor, instance-method, and static-method helpers remain distinct.

use super::*;
use crate::ir::Op;

/// Deep defaults outside compact eval metadata still get Mixed-returning EIR helpers.
#[test]
fn deep_native_defaults_get_compiled_helpers_on_every_target() {
    let source = r#"<?php
class Defaults {
    public function __construct($value = [[[[[[[[[[[[[[[[[[[[1]]]]]]]]]]]]]]]]]]]]) {}
    public function method($value = [[[[[[[[[[[[[[[[[[[[2]]]]]]]]]]]]]]]]]]]]) {}
    public static function statik($value = [[[[[[[[[[[[[[[[[[[[3]]]]]]]]]]]]]]]]]]]]) {}
    public function compact($value = [4]) {}
}
$source = $argv[1];
eval($source);
"#;
    for target_name in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = Target::parse(target_name).unwrap();
        let module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), target);
        let class = module.class_infos.get("Defaults").unwrap();
        for (is_static, method) in [(false, "__construct"), (false, "method"), (true, "statik")] {
            let name = crate::ir_lower::eval_native_default_helper_name(
                class.class_id,
                is_static,
                method,
                0,
            );
            let helper = module.functions.iter().find(|function| function.name == name)
                .unwrap_or_else(|| panic!("{target_name}: missing {method} default helper"));
            assert_eq!(helper.return_php_type, crate::types::PhpType::Mixed, "{target_name}");
            assert!(helper.instructions.iter().any(|inst| inst.op == Op::ArrayNew), "{target_name}: {method}");
        }
        let compact_name = crate::ir_lower::eval_native_default_helper_name(
            class.class_id,
            false,
            "compact",
            0,
        );
        assert!(
            module
                .functions
                .iter()
                .all(|function| function.name != compact_name),
            "{target_name}: compact metadata must not grow a compiled helper"
        );

        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target_name}: {error:?}"));
        for symbol in [
            "__elephc_eval_register_native_constructor_param_default_scalar",
            "__elephc_eval_register_native_method_param_default_scalar",
            "__elephc_eval_register_native_static_method_param_default_scalar",
        ] {
            assert!(
                assembly.contains(symbol),
                "{target_name}: missing compiled-default registration {symbol}"
            );
        }
        for (is_static, method) in [(false, "__construct"), (false, "method"), (true, "statik")] {
            let helper = crate::ir_lower::eval_native_default_helper_name(
                class.class_id,
                is_static,
                method,
                0,
            );
            let symbol = crate::names::function_symbol(&helper);
            assert!(
                assembly.contains(&symbol),
                "{target_name}: missing compiled helper symbol {symbol}"
            );
        }
    }
}
