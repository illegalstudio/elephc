//! Purpose:
//! Verifies JSON reference-property metadata and native payload reads on every target.
//!
//! Called from:
//! - The focused runtime emitter and EIR unit suites in CI.
//!
//! Key details:
//! - Descriptor flags retain PHP types while object slots hold reference-cell pointers.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Target}};
use std::path::Path;

/// A native reference slot is followed once and encoded using its original PHP type.
#[test]
fn json_reference_property_metadata_and_payloads_on_every_target() {
    let source = r#"<?php
class NativeJsonReference {
    public string $text = 'value';
    public function &textRef(): string { return $this->text; }
}
$object = new NativeJsonReference();
$alias =& $object->textRef();
echo json_encode($object);
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), target,
        );
        let class = &module.class_infos["NativeJsonReference"];
        assert!(class.owned_reference_properties.contains("text"), "{name}: owned storage");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let marker = format!("_class_json_desc_{}:\n", class.class_id);
        let descriptor = assembly.split_once(&marker).unwrap().1;
        let descriptor = descriptor.split_once(".globl _class_gc_desc_").unwrap().0;
        let tag = super::JSON_REFERENCE_PROPERTY_FLAG | super::JSON_DECLARED_PROPERTY_FLAG | 1;
        assert!(descriptor.contains(&format!("    .quad {tag}\n")), "{name}: string tag survives promotion");
        let mut emitter = Emitter::new(target);
        super::emit_json_encode_object(&mut emitter);
        let runtime = emitter.output();
        let follow = if target.arch == Arch::AArch64 { "ldr x9, [x9]" }
            else { "mov rax, QWORD PTR [rax]" };
        let payload = runtime.find("__rt_json_obj_direct_payload").unwrap();
        assert!(runtime[payload..].contains(follow), "{name}: follow reference payload before encoding");
    }
}
