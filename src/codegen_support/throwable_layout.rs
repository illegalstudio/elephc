//! Purpose:
//! Shares declared Throwable storage between native allocation and field access, and reserves
//! ordinary Throwable construction metadata outside PHP property storage.
//!
//! Called from:
//! - EIR object lowering and runtime-generated exception paths.
//! - Generic object allocation and runtime class-data emission.
//!
//! Key details:
//! - These are Elephc property slots, not Zend's in-memory zval layout.
//! - The optional creation-line word follows declared properties and the dynamic-property hash.
//! - Property, GC, and dynamic-hash offsets keep their existing layout.


use super::{abi, emit::Emitter, platform::Arch};
use crate::types::ClassInfo;

/// Returns the dedicated creation-line offset for an ordinary Throwable layout.
pub(crate) fn ordinary_creation_line_offset(
    class_name: &str,
    classes: &crate::fast_hash::FastMap<String, ClassInfo>,
) -> Option<usize> {
    let class = classes.get(class_name)?;
    let mut current = Some(class_name);
    while let Some(name) = current {
        let ancestor = classes.get(name)?;
        if ancestor.is_internal && matches!(name, "Exception" | "Error") {
            // The creation line lives in the declared `line` property slot, exactly where the compact
            // (heap kind 6) payload keeps it, so every Throwable has one layout and no tail word.
            let _ = class;
            return Some(LINE_OFFSET);
        }
        current = ancestor.parent.as_deref();
    }
    None
}

/// Resolves ordinary allocation metadata when an emitter carries a numeric class id.
pub(crate) fn ordinary_creation_line_offset_by_id(
    class_id: u64,
    classes: &crate::fast_hash::FastMap<String, ClassInfo>,
) -> Option<usize> {
    let (name, _) = classes.iter().find(|(_, class)| class.class_id == class_id)?;
    ordinary_creation_line_offset(name, classes)
}

// Keep in step with the inherited fields in builtin_types::declarations. Each
// declared property occupies sixteen bytes after the eight-byte class id.
pub(crate) const FILE_OFFSET: usize = 8 + 3 * 16;
pub(crate) const LINE_OFFSET: usize = 8 + 4 * 16;
/// PHP's private `trace` array, filled by `__rt_throwable_capture_trace` when eval frames exist.
pub(crate) const TRACE_OFFSET: usize = 8 + 5 * 16;
pub(crate) const PREVIOUS_OFFSET: usize = 8 + 6 * 16;
pub(crate) const PAYLOAD_SIZE: usize = 8 + 7 * 16;

/// Allocates and clears every declared slot before any property or GC walk.
/// The caller still stamps the class id and heap kind and acquires its handle.
/// User subclasses must pass their actual layout size, including any dynamic tail.
pub(crate) fn emit_allocate(emitter: &mut Emitter, payload_size: usize) {
    assert!(payload_size >= PAYLOAD_SIZE);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, payload_size as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    // -- recycled heap blocks must not expose stale owned property pointers --
    for offset in (8..payload_size).step_by(8) {
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("str xzr, [x0, #{}]", offset)), // initialize the complete declared property region
            Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", offset)), // initialize the complete declared property region
        }
    }
    emit_initial_file(emitter, result);
}

/// Initializes the per-object source-file property with the known module path.
/// This does not invent missing include/stack provenance; callers with a more
/// precise source location can subsequently replace the ordinary property.
pub(crate) fn emit_initial_file(emitter: &mut Emitter, object: &str) {
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_symbol_address(emitter, scratch, "_script_source_file");
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_script_source_file_len", 0);
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET + 8);
}

/// Replaces the initialized default file with `file` (a static data label and its length), or
/// with the module path when `file` is `None`, without leaking the previous owned string.
pub(crate) fn emit_replace_initial_file_with(
    emitter: &mut Emitter,
    object: &str,
    file: Option<(&str, usize)>,
) {
    abi::emit_push_reg(emitter, object);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, result, object, FILE_OFFSET);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_pop_reg(emitter, object);
    let Some((label, len)) = file else {
        emit_initial_file(emitter, object);
        return;
    };
    emit_store_static_file(emitter, object, label, len);
}

/// The three words a `new $class(...)` site publishes for its shared constructor helper: the
/// source file's pointer and length and the line, which a Throwable built there records.
pub(crate) const DYNAMIC_NEW_SITE_SYMBOL: &str = "_dyn_new_site";
/// Bytes of [`DYNAMIC_NEW_SITE_SYMBOL`].
pub(crate) const DYNAMIC_NEW_SITE_BYTES: usize = 24;

/// Replaces the initialized default file with the one the calling site published in
/// [`DYNAMIC_NEW_SITE_SYMBOL`], without leaking the previous owned string.
pub(crate) fn emit_replace_initial_file_from_site(emitter: &mut Emitter, object: &str) {
    abi::emit_push_reg(emitter, object);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, result, object, FILE_OFFSET);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_pop_reg(emitter, object);
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_load_symbol_to_reg(emitter, scratch, DYNAMIC_NEW_SITE_SYMBOL, 0);
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET);
    abi::emit_load_symbol_to_reg(emitter, scratch, DYNAMIC_NEW_SITE_SYMBOL, 8);
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET + 8);
}

/// Stores a static data string as the object's source file, over a static one (no release).
pub(crate) fn emit_store_static_file(emitter: &mut Emitter, object: &str, label: &str, len: usize) {
    let scratch = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };
    abi::emit_symbol_address(emitter, scratch, label);
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET);
    abi::emit_load_int_immediate(emitter, scratch, len as i64);
    abi::emit_store_to_address(emitter, scratch, object, FILE_OFFSET + 8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Extra metadata follows the complete inherited layout without reusing a scalar high word.
    #[test]
    fn ordinary_throwable_metadata_follows_properties_and_dynamic_hash() {
        let tokens = crate::lexer::tokenize("<?php class LocationChild extends Exception { public int $extra = 1; } class PlainLocation { public int $code = 1; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let mut checked = crate::types::check(&program).unwrap();
        let class = &checked.classes["LocationChild"];
        assert_eq!(ordinary_creation_line_offset("LocationChild", &checked.classes), Some(LINE_OFFSET));
        assert_eq!(ordinary_creation_line_offset_by_id(class.class_id, &checked.classes), Some(LINE_OFFSET));
        checked.classes.get_mut("LocationChild").unwrap().allow_dynamic_properties = true;
        assert_eq!(ordinary_creation_line_offset("LocationChild", &checked.classes), Some(LINE_OFFSET));
        assert_eq!(ordinary_creation_line_offset("PlainLocation", &checked.classes), None);
    }

    #[test]
    fn throwable_layout_matches_declared_builtin_properties() {
        let tokens = crate::lexer::tokenize("<?php new Exception(); new Error(); new ReflectionException();").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        for name in ["Exception", "Error", "ReflectionException"] {
            let info = &checked.classes[name];
            assert_eq!(8 + info.properties.len() * 16, PAYLOAD_SIZE, "{name}");
            for (property, expected) in [("file", FILE_OFFSET), ("line", LINE_OFFSET), ("trace", TRACE_OFFSET), ("previous", PREVIOUS_OFFSET)] {
                let index = info.properties.iter().position(|(name, _)| name == property).unwrap();
                assert_eq!(8 + index * 16, expected, "{name}::{property}");
            }
        }
    }

    /// Regression: a compiled throwable subclass owned by eval storage must reserve the trailing
    /// property-hash slot, exactly as `_class_object_payload_sizes` reports it. Allocating eight
    /// bytes short made `__rt_object_free_deep` read the next heap block's word as that hash and
    /// release an unrelated live array (Symfony's `secrets:reveal` lost `$secrets` inside
    /// `EnvNotFoundException`'s cleanup).
    #[test]
    fn throwable_payload_size_matches_the_free_layout_for_eval_owned_subclasses() {
        let tokens = crate::lexer::tokenize("<?php class EvalOwnedThrowable extends RuntimeException {}").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        let mut info = checked.classes["EvalOwnedThrowable"].clone();
        let declared = 8 + info.properties.len() * 16;

        info.eval_property_storage = true;
        assert_eq!(
            crate::types::object_payload_size("EvalOwnedThrowable", &info),
            declared + 8,
            "eval-owned throwable subclasses reserve the dynamic-property tail"
        );

        info.eval_property_storage = false;
        assert_eq!(
            crate::types::object_payload_size("EvalOwnedThrowable", &info),
            declared,
            "a class with no property-hash storage allocates only its declared slots"
        );
    }

    #[test]
    fn throwable_layout_allocation_initializes_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_allocate(&mut emitter, PAYLOAD_SIZE);
            let asm = emitter.output();
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("mov x0, #120"), "{name}: {asm}");
                    for offset in (8..PAYLOAD_SIZE).step_by(8) {
                        assert!(asm.contains(&format!("str xzr, [x0, #{offset}]")), "{name}: {asm}");
                    }
                    assert!(asm.contains("str x10, [x0, #56]"), "{name}: {asm}");
                    assert!(asm.contains("str x10, [x0, #64]"), "{name}: {asm}");
                }
                Arch::X86_64 => {
                    assert!(asm.contains("mov rax, 120"), "{name}: {asm}");
                    for offset in (8..PAYLOAD_SIZE).step_by(8) {
                        assert!(asm.contains(&format!("mov QWORD PTR [rax + {offset}], 0")), "{name}: {asm}");
                    }
                    assert!(asm.contains("mov QWORD PTR [rax + 56], r10"), "{name}: {asm}");
                    assert!(asm.contains("mov QWORD PTR [rax + 64], r10"), "{name}: {asm}");
                }
            }
        }
    }
}
