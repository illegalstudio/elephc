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

/// Writable SPL selectors retain raw storage while returning attached cells and respecting overrides.
#[test]
fn spl_write_fetch_separates_raw_storage_and_selects_only_internal_getters_on_all_targets() {
    let source = r#"<?php
class StorageSetterOverride extends ArrayObject {
    public function offsetSet(mixed $key, mixed $value): void { echo 'setter'; }
}
class StorageGetterOverride extends ArrayObject {
    public function offsetGet(mixed $key): mixed { return parent::offsetGet($key); }
}
$object = new ArrayObject(['x' => [1]]); $object['x'][] = 2;
$iterator = new ArrayIterator(['x' => [1]]); $iterator['x'][] = 2;
$setter = new StorageSetterOverride(['x' => [1]]); $setter['x'][] = 2;
$getter = new StorageGetterOverride(['x' => [1]]); $getter['x'][] = 2;
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        for class in ["ArrayObject", "ArrayIterator"] {
            let method_name = format!("{class}::__elephc_fetch_for_write");
            let method = module.class_methods.iter().find(|method| method.name == method_name).unwrap();
            let storage = method.instructions.iter().find(|inst| inst.op == Op::PropGetForWrite).unwrap();
            assert_eq!(storage.result_php_type, PhpType::Array(Box::new(PhpType::Mixed)), "{name}: {class}");
            let fetch = method.instructions.iter().find(|inst| inst.op == Op::ArrayGetForWrite).unwrap();
            assert_eq!(fetch.operands[0], storage.result.unwrap(), "{name}: {class}");
            assert_eq!(fetch.result_php_type, PhpType::Mixed, "{name}: {class}");
            assert!(!method.instructions.iter().any(|inst| inst.op == Op::LoadArrayElemRefCell), "{name}: {class}");
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let (_, table) = assembly.split_once("_class_offsetget_forwrite_ptrs:\n").unwrap();
        let entries = table.lines().map_while(|line| line.trim().strip_prefix(".quad ")).collect::<Vec<_>>();
        for class in ["ArrayObject", "ArrayIterator", "StorageSetterOverride"] {
            let id = module.class_infos[class].class_id as usize;
            assert!(entries[id].contains("__elephc_fetch_for_write"), "{name}: {class}: {}", entries[id]);
        }
        let id = module.class_infos["StorageGetterOverride"].class_id as usize;
        assert_eq!(entries[id], "0", "{name}: a user getter must keep its value semantics");
    }
}

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
