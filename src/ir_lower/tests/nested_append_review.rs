//! Purpose:
//! Verifies object-aware nested appends and deep false updates on every target.
//!
//! Called from:
//! - AST-to-EIR unit tests through Rust's test harness.
//!
//! Key details:
//! - Objects use a null-key method call while arrays retain attached boxed mutation.
//! - Native emitters must support the same branch graph on all five targets.

/// Every target emits object-aware append dispatch and accepts deeper false promotion.
#[test]
fn nested_append_review_all_targets() {
    let source = r#"<?php
class Bag implements ArrayAccess {
    public function offsetExists(mixed $key): bool { return false; }
    public function offsetGet(mixed $key): mixed { return null; }
    public function offsetUnset(mixed $key): void {}
    public function offsetSet(mixed $key, mixed $value): void { echo $value; }
}

$items = [new Bag(), [1], null];
for ($i = 0; $i < 3; $i++) { $items[$i][] = 2; }
$deep = [false]; ++$deep[0][0][1];
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let module = super::lower_source_at_for_target(source,
            std::path::Path::new("main.php"), std::path::Path::new("."), target);
        let ir = crate::ir::print_module(&module);
        assert!(ir.contains("nested.append.arrayaccess"), "{name}: {ir}");
        assert!(ir.contains("mixed_array_append"), "{name}: {ir}");
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every target rejects callable payloads through the shared class-name Error entry.
#[test]
fn nested_append_review_closure_error_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(name).unwrap();
        let mut emitter = crate::codegen_support::emit::Emitter::new(target);
        emitter.dead_strip = emitter.platform == crate::codegen_support::platform::Platform::MacOS;
        crate::codegen_support::runtime::emit_runtime(&mut emitter,
            crate::codegen_support::RuntimeFeatures::none());
        let asm = emitter.output();
        assert!(asm.contains("__rt_mixed_array_set_closure_not_indexable:"), "{name}");
        assert!(asm.contains("__rt_throw_object_not_array_name:"), "{name}");
        assert!(asm.contains("_closure_class_name"), "{name}");
        let rejected = asm.split("__rt_mixed_array_set_closure_not_indexable:").nth(1).unwrap();
        let release = rejected.find("__rt_decref_mixed").unwrap();
        let throw = rejected.find("__rt_throw_object_not_array_name").unwrap();
        assert!(release < throw, "{name}: the rejected value must retire before throwing");
    }
}
