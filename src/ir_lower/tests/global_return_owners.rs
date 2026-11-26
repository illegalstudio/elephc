//! Purpose:
//! Preserves independent owners for values returned from mutable global storage.
//!
//! Called from:
//! - The AST-to-EIR ownership unit suite on every supported target.
//!
//! Key details:
//! - Mixed cells detach from the global; array payloads retain a separate caller lease.

use crate::ir::{Op, Ownership};

/// A function-local reference must borrow its global symbol, never an uninitialized local.
#[test]
fn named_global_aliases_use_global_addresses_on_all_targets() {
    let source = r#"<?php
$shared = json_decode('[1,2]', true);
function aliasGlobal(mixed $replacement): mixed {
    global $shared;
    $alias =& $shared;
    $alias = $replacement;
    return $shared;
}
echo json_encode(aliasGlobal(['updated']));
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let function = module.functions.iter().find(|function| function.name == "aliasGlobal").unwrap();
        let reference = function.instructions.iter().find(|inst| inst.op == Op::LoadGlobalRefCell)
            .expect("the reference must name global storage");
        let Some(crate::ir::Immediate::GlobalName(name)) = reference.immediate else { panic!("{target}: missing global name"); };
        assert_eq!(module.data.global_names[name.as_raw() as usize], "shared", "{target}");
        assert_eq!(reference.result_php_type, crate::types::PhpType::Pointer(None), "{target}");
        assert!(!function.instructions.iter().any(|inst| inst.op == Op::PromoteLocalRefCell),
            "{target}: the global declaration has no local payload to promote");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("global_reference_publish"), "{target}: stores must preserve persistent global references");
    }
}

/// Global reads cannot escape as borrowed results into an owning direct call.
#[test]
fn global_value_returns_acquire_independent_owners_on_all_targets() {
    let source = r#"<?php
$shared = json_decode('[1,2]', true);
$rows = [3, 4];
function returnGlobalMixed(): mixed { global $shared; return $shared; }
function returnGlobalRows(): array { global $rows; return $rows; }
$copy = returnGlobalMixed(); $rowCopy = returnGlobalRows();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        for name in ["returnGlobalMixed", "returnGlobalRows"] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let global = function.instructions.iter().find(|inst| inst.op == Op::LoadGlobal)
                .expect("the return must read global storage").result.unwrap();
            if name == "returnGlobalMixed" {
                assert!(function.instructions.iter().any(|inst| inst.op == Op::MixedClone && inst.operands == [global]),
                    "{target}: mutable global cells must detach before returning");
            }
            for block in &function.blocks {
                if let Some(crate::ir::Terminator::Return { value: Some(value) }) = &block.terminator {
                    assert_eq!(function.value(*value).unwrap().ownership, Ownership::Owned, "{target}: {name}");
                }
            }
        }
    }
}
