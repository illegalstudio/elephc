//! Purpose:
//! Checks runtime indexed property protocols before the stdClass-only fallback.
//!
//! Called from:
//! - The supported-target EIR regression suite.
//!
//! Key details:
//! - Dynamic cells remain attached and overloaded values retain their PHP notice protocol.

use crate::ir::{Immediate, Op, RuntimeCallTarget};

/// Every target represents dynamic COW, visibility-aware reads and overloaded-value notices.
fn verify_indexed_property_protocols(target: &str) {
    let module = super::lower_source_at_for_target(r#"<?php
#[AllowDynamicProperties]
class IndexedProtocolDynamic {}
class IndexedProtocolMagic {
    public function __get(string $name): mixed { return [1]; }
}
function indexedProtocolWrite(mixed $object): void { $object->items[0] += 2; }
indexedProtocolWrite(new IndexedProtocolDynamic());
"#, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "indexedProtocolWrite").unwrap();
    assert!(function.instructions.iter().any(|inst| inst.op == Op::DynamicPropGet),
        "{target}: the class dispatcher must preserve read visibility and magic access");
    for target in [RuntimeCallTarget::PropertyHashFetchForWrite, RuntimeCallTarget::WriteDiagnostic] {
        assert!(function.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
            && inst.immediate == Some(Immediate::RuntimeCall(target))),
            "{target:?}: retain the indexed property protocol in EIR");
    }
}

supported_target_cases!(indexed_property_protocols, verify_indexed_property_protocols);

/// Parent-private array writes must stay on their declared slot for a dynamic-capable child.
fn verify_indexed_private_ancestor_storage(target: &str) {
    let source = r#"<?php
#[AllowDynamicProperties]
class IndexedPrivateBase {
    private array $items = [];
    public function __construct(array $options) {
        foreach ($options as $key => $value) { $this->items[$key] = $value; }
    }
}
class IndexedPrivateChild extends IndexedPrivateBase {}
$child = new IndexedPrivateChild([2 => 2, 1005 => 1]);
"#;
    let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let constructor = module.class_methods.iter().find(|function| function.name == "IndexedPrivateBase::__construct").unwrap();
    assert!(!constructor.instructions.iter().any(|inst| inst.op == Op::RuntimeCall
        && inst.immediate == Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashFetchForWrite))),
        "{target}: lexical private array writes must not enter a descendant's dynamic property hash");
}

supported_target_cases!(indexed_private_ancestor_storage, verify_indexed_private_ancestor_storage);
