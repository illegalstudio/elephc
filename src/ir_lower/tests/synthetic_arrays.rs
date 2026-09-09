//! Purpose:
//! Verifies compiler-generated SPL arrays keep their internal container representation.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Source PHP array declarations remain boxed; synthetic metadata uses an explicit array ABI.

use crate::codegen::platform::Target;
use crate::ir::Op;
use crate::types::PhpType;
use std::path::Path;

/// SPL storage reconstruction writes concrete array payloads back into its internal properties.
#[test]
fn spl_storage_array_reconstruction_preserves_internal_abi_on_all_targets() {
    let source = r#"<?php
class StoredSyntheticValue { public int $number = 42; }
$storage = new SplObjectStorage();
$value = new StoredSyntheticValue();
$storage->attach($value, "kept");
$storage->detach($value);
echo count($storage);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let method = module.class_methods.iter().find(|method| method.name == "SplObjectStorage::detach").unwrap();
        let stores = method.instructions.iter().filter(|inst| inst.op == Op::PropSet).collect::<Vec<_>>();
        assert_eq!(stores.len(), 2, "{name}");
        for store in stores {
            let value = method.value(*store.operands.last().unwrap()).unwrap();
            assert_eq!(value.php_type.codegen_repr(), PhpType::Array(Box::new(PhpType::Mixed)), "{name}");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
    }
}
