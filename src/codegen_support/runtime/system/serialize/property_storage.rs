//! Purpose:
//! Resolves serialized property rows to native payload slots and initialization markers.
//!
//! Called from:
//! - The default object and __sleep property walks in the serialization emitter.
//!
//! Key details:
//! - Row flags describe physical indirection independently of the logical value tag.
//! - Reference cells carry their uninitialized sentinel in the low word.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::SERPROP_REFERENCE_PROPERTY_FLAG;

/// Follows a flagged native cell and reads its marker without calling or acquiring owners.
pub(super) fn resolve(
    emitter: &mut Emitter, address: &str, tag: &str, marker: &str, prefix: &str,
) {
    let reference = format!("{prefix}_reference");
    let done = format!("{prefix}_resolved");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("tst {tag}, #{SERPROP_REFERENCE_PROPERTY_FLAG}")); // distinguish a cell pointer from an inline value
            emitter.instruction(&format!("b.ne {reference}"));                  // indirect cells use their low-word sentinel
            emitter.instruction(&format!("ldr {marker}, [{address}, #8]"));     // inline slots carry their initialization marker in the high word
            emitter.instruction(&format!("b {done}"));                          // preserve the inline payload address
            emitter.label(&reference);
            emitter.instruction(&format!("ldr {address}, [{address}]"));        // follow the object's native reference-cell pointer
            emitter.instruction(&format!("ldr {marker}, [{address}]"));         // the low word is either the value or the uninitialized sentinel
            emitter.label(&done);
            emitter.instruction(&format!("and {tag}, {tag}, #255"));            // keep storage flags out of value dispatch
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {tag}, {SERPROP_REFERENCE_PROPERTY_FLAG}")); // distinguish a cell pointer from an inline value
            emitter.instruction(&format!("jnz {reference}"));                   // indirect cells use their low-word sentinel
            emitter.instruction(&format!("mov {marker}, QWORD PTR [{address} + 8]")); // inline slots carry their initialization marker in the high word
            emitter.instruction(&format!("jmp {done}"));                        // preserve the inline payload address
            emitter.label(&reference);
            emitter.instruction(&format!("mov {address}, QWORD PTR [{address}]")); // follow the object's native reference-cell pointer
            emitter.instruction(&format!("mov {marker}, QWORD PTR [{address}]")); // the low word is either the value or the uninitialized sentinel
            emitter.label(&done);
            emitter.instruction(&format!("and {tag}, 255"));                    // keep storage flags out of value dispatch
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::{platform::Target, sentinels::SERPROP_REFERENCE_PROPERTY_FLAG};

    /// Serialization rows retain each logical tag independently of native cell indirection.
    #[test]
    fn native_reference_serialization_metadata_on_every_target() {
        let source = "<?php class SerializationCellLayout { public string $text = 'text'; \
            public mixed $value = 1; public mixed $missing; \
            public function &textRef(): string { return $this->text; } } \
            $code = $argc > 0 ? '' : 'echo 1;'; eval($code);";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            let class = &module.class_infos["SerializationCellLayout"];
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let marker = format!("_class_serprop_{}:\n", class.class_id);
            let rows = assembly.split_once(&marker).unwrap().1
                .split_once(".globl _class_serprop_declaring_").unwrap().0;
            assert_eq!(rows.matches(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | 7)).count(), 2,
                "{name}: Mixed value and uninitialized slots remain distinct from their cell pointers");
            assert!(rows.contains(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | 1)),
                "{name}: a raw native string cell keeps the string payload tag");
        }
    }
}
