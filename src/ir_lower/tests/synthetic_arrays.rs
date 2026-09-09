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

/// User and inherited hydration hooks select boxed input while synthetic SPL keeps its internal ABI.
#[test]
fn hydration_parameter_metadata_preserves_boxed_and_synthetic_abis_on_all_targets() {
    let source = r#"<?php
class H { public function __unserialize(array $data): void { echo $data["x"]; } }
class ChildH extends H {}
class U { public function __unserialize($data): void { echo $data["x"]; } }
$value = unserialize('O:6:"ChildH":1:{s:1:"x";i:42;}');
$untyped = new U();
$untyped->__unserialize(["x" => 9]);
$storage = new SplObjectStorage();
$storage->__unserialize([]);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let (_, table) = assembly.split_once("_class_unserialize_data_boxed:\n").unwrap();
        let flags = table.lines().map_while(|line| line.trim().strip_prefix(".quad "))
            .map(|value| value.parse::<u8>().unwrap()).collect::<Vec<_>>();
        for (class, expected) in [("H", 1), ("ChildH", 1), ("U", 1), ("SplObjectStorage", 0)] {
            let class_info = module.class_infos.get(class).unwrap();
            assert_eq!(flags[class_info.class_id as usize], expected, "{name}: {class}");
        }
    }
}

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
