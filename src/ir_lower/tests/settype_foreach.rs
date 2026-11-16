//! Purpose:
//! Verifies storage-aware scalar settype updates for boxed hash foreach bindings.
//!
//! Called from:
//! - The EIR lowering unit suite.
//!
//! Key details:
//! - All five supported targets use explicit casts and ordinary local stores.

use std::path::Path;

use crate::codegen::platform::Target;
use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

/// Boxed foreach values use the ordinary cast/store ownership path on every target.
#[test]
fn settype_foreach_mixed_storage_emits_on_all_targets() {
    let mut source = String::from("<?php\n");
    for target in ["int", "float", "bool", "string"] {
        let next = if target == "int" { "string" } else { "int" };
        source.push_str(&format!(r#"
function convert_{target}(array<string, int> $source): void {{
    foreach ($source as $value) {{
        for ($i = 0; $i < 2; $i++) {{
            echo gettype($value);
            settype($value, '{target}');
            echo gettype($value), ':', $value;
            settype($value, '{next}');
        }}
    }}
}}
"#));
    }
    source.push_str(r#"
function convert_static(int $n): void {
    static $value = null;
    $value = $n;
    settype($value, 'float');
    echo gettype($value), $value;
}
function convert_static_mixed(): void {
    static $value = null;
    settype($value, 'float');
    echo gettype($value), $value;
    $value = 1.5;
}
"#);
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            &source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        for (suffix, php_type) in [
            ("int", PhpType::Int), ("float", PhpType::Float),
            ("bool", PhpType::Bool), ("string", PhpType::Str),
        ] {
            let function = module.functions.iter()
                .find(|function| function.name == format!("convert_{suffix}")).unwrap();
            assert!(!function.instructions.iter().any(|instruction| {
                instruction.immediate == Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::Function(RuntimeFnId::Settype)))
            }), "{suffix} on {target}: boxed mutation must use storage-aware assignment");
            assert!(function.instructions.iter().any(|instruction| {
                instruction.op == Op::Cast && instruction.result_php_type == php_type
                    && function.value(instruction.operands[0]).unwrap().php_type == PhpType::Mixed
            }), "{suffix} on {target}: missing scalar projection");
            assert!(function.instructions.iter().filter(|instruction| {
                instruction.op == Op::Cast
            }).count() >= 2, "{suffix} on {target}: sequential conversions must retain boxed storage");
        }
        for name in ["convert_static", "convert_static_mixed"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            assert!(function.instructions.iter().any(|instruction| instruction.op == Op::StoreStaticLocal),
                "{name} on {target}: fixture must write static storage");
            assert!(function.instructions.iter().filter(|instruction| instruction.op == Op::LoadStaticLocal)
                .all(|instruction| instruction.result_php_type == PhpType::Mixed),
                "{name} on {target}: converted static reads must retain their boxed runtime tag");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error}"));
    }
}
