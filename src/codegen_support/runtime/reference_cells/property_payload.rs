//! Purpose:
//! Resolves native property descriptors to payload slots and initialization markers.
//!
//! Called from:
//! - Serialization, value renderers and object-to-array projection emitters.
//!
//! Key details:
//! - Row flags describe physical indirection independently of the logical value tag.
//! - Reference cells carry their uninitialized sentinel in the low word.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::SERPROP_REFERENCE_PROPERTY_FLAG;

/// Follows a flagged native cell and reads its marker without calling or acquiring owners.
pub(crate) fn resolve(
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
            super::resolve_scalar_property(emitter, address, tag, marker, prefix);
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
            super::resolve_scalar_property(emitter, address, tag, marker, prefix);
            emitter.label(&done);
            emitter.instruction(&format!("and {tag}, 255"));                    // keep storage flags out of value dispatch
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::{platform::Target, sentinels::{SERPROP_REFERENCE_PROPERTY_FLAG,
        SERPROP_MIXED_REFERENCE_PROPERTY_FLAG, CANONICAL_SCALAR_PROPERTY_FLAG}};

    /// Serialization rows retain each logical tag independently of native cell indirection.
    fn verify_native_reference_serialization_metadata(name: &str) {
        let source = "<?php class SerializationCellLayout { public string $text = 'text'; \
            public mixed $value = 1; public mixed $missing; public int|string $choice = 1; \
            public function &textRef(): string { return $this->text; } \
            public function &choiceRef(): int|string { return $this->choice; } } \
            class BorrowedSerializationCell { public function __construct(public string &$text) {} } \
            $code = $argc > 0 ? '' : 'echo 1;'; eval($code);";
        let target = Target::parse(name).unwrap();
        let module = crate::ir_lower::tests::lower_source_at_for_target(
            source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
        );
        let class = &module.class_infos["SerializationCellLayout"];
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        let marker = format!("_class_serprop_{}:\n", class.class_id);
        let rows = assembly.split_once(&marker).unwrap().1
            .split_once(".globl _class_serprop_declaring_").unwrap().0;
        assert_eq!(rows.matches(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | SERPROP_MIXED_REFERENCE_PROPERTY_FLAG | 7)).count(), 2,
            "{name}: Mixed value and uninitialized slots remain distinct from their cell pointers");
        assert_eq!(rows.matches(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | 7)).count(), 1,
            "{name}: the constrained union keeps its existing payload representation");
        let scalar_tag = SERPROP_REFERENCE_PROPERTY_FLAG | CANONICAL_SCALAR_PROPERTY_FLAG | 1;
        assert!(rows.contains(&format!("    .quad {scalar_tag}\n")),
            "{name}: owned scalar metadata authenticates physical shape without changing the logical tag");
        let borrowed_class = &module.class_infos["BorrowedSerializationCell"];
        let marker = format!("_class_serprop_{}:\n", borrowed_class.class_id);
        let borrowed = assembly.split_once(&marker).unwrap().1
            .split_once(".globl _class_serprop_declaring_").unwrap().0;
        assert!(borrowed.contains(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | 1)),
            "{name}: borrowed constructor slots must never grant header access");
        let marker = format!("_class_vd_desc_{}:\n", class.class_id);
        let dump = assembly.split_once(&marker).unwrap().1
            .split_once(".globl _class_prop_desc_").unwrap().0;
        assert_eq!(dump.matches(&format!("    .quad {}\n", SERPROP_REFERENCE_PROPERTY_FLAG | 7)).count(), 3,
            "{name}: dump and serialization agree on Mixed cell indirection");
        assert!(dump.contains(&format!("    .quad {scalar_tag}\n")),
            "{name}: dump preserves the raw string's logical tag");
    }

    /// Schedules the fixture once per supported target.
    mod native_reference_serialization_metadata_on_every_target {
        /// Verifies the fixture on macos-aarch64.
        #[test]
        fn macos_aarch64() { super::verify_native_reference_serialization_metadata("macos-aarch64"); }
        /// Verifies the fixture on ios-arm64.
        #[test]
        fn ios_arm64() { super::verify_native_reference_serialization_metadata("ios-arm64"); }
        /// Verifies the fixture on ios-sim-arm64.
        #[test]
        fn ios_sim_arm64() { super::verify_native_reference_serialization_metadata("ios-sim-arm64"); }
        /// Verifies the fixture on linux-aarch64.
        #[test]
        fn linux_aarch64() { super::verify_native_reference_serialization_metadata("linux-aarch64"); }
        /// Verifies the fixture on linux-x86_64.
        #[test]
        fn linux_x86_64() { super::verify_native_reference_serialization_metadata("linux-x86_64"); }
    }
}
