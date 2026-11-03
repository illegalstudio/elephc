//! Purpose:
//! Owns managed canonical cells behind native static-property bindings.
//!
//! Called from:
//! - Explicit native-validation ABI fixtures and the static-storage integration.
//!
//! Key details:
//! - A binding reserves value, initialization and promoted-layout words, in that order.
//! - Only the allocated cell has a managed heap header; the static binding never does.
//! - Promotion transfers the existing Mixed owner, preserving any escaped PHP wrapper.
//! - Retirement detaches the binding and its weak source before PHP cleanup can reenter.

use super::*;

/// Byte offset of the layout word in the complete static binding record.
pub(crate) const LAYOUT_OFFSET: usize = 16;
/// Required static storage size, including the separate promoted-layout word.
pub(crate) const BINDING_BYTES: usize = LAYOUT_OFFSET + 8;

/// Loads the borrowed PHP value owned by a raw or promoted static Mixed binding.
/// The promoted cell owns its wrapper; readers expose the current child without retaining it.
pub(crate) fn load_mixed_value(emitter: &mut Emitter, symbol: &str, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    if native_validation {
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), symbol);
        abi::emit_call_label(emitter, "__rt_reference_static_mixed_payload");
        abi::emit_load_from_address(emitter, result, result, 0);
    } else {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
}

/// Emits the optional static-binding ABI without changing ordinary symbol-backed storage.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_payload(emitter);
    emit_promote(emitter);
    emit_release(emitter);
}

/// Returns a borrowed payload address, accepting a raw binding or its allocated managed cell.
fn emit_payload(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let done = "__rt_reference_static_mixed_payload_done";
    emitter.label_global("__rt_reference_static_mixed_payload");
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    branch_zero(emitter, scratch, done);
    abi::emit_load_from_address(emitter, result, result, 0);
    emitter.label(done);
    abi::emit_return(emitter);
}

/// Gives one initialized Mixed static binding a cell owner without retaining its child twice.
fn emit_promote(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let ready = "__rt_reference_static_mixed_promote_ready";
    let refused = "__rt_reference_static_mixed_promote_refused";
    let done = "__rt_reference_static_mixed_promote_done";
    emitter.label_global("__rt_reference_static_mixed_promote");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    branch_nonzero(emitter, scratch, ready);
    abi::emit_load_from_address(emitter, result, result, 8);
    branch_uninitialized(emitter, result, scratch, refused);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, 16);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, scratch, scratch, 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_store_to_address(emitter, result, scratch, LAYOUT_OFFSET);
    emitter.label(ready);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_jump(emitter, done);
    emitter.label(refused);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Detaches one static owner before releasing either its raw box or its promoted cell.
fn emit_release(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let raw = "__rt_reference_static_mixed_release_raw";
    let done = "__rt_reference_static_mixed_release_done";
    emitter.label_global("__rt_reference_static_mixed_release");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    abi::store_at_offset(emitter, scratch, 24);
    abi::emit_load_from_address(emitter, result, result, 8);
    let initialized = "__rt_reference_static_mixed_release_initialized";
    abi::load_at_offset(emitter, scratch, 24);
    branch_nonzero(emitter, scratch, initialized);
    branch_uninitialized(emitter, result, scratch, done);
    emitter.label(initialized);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::store_at_offset(emitter, result, 16);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, LAYOUT_OFFSET);
    abi::emit_load_int_immediate(emitter, result, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    abi::emit_store_to_address(emitter, result, scratch, 8);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, raw);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    let unbind = emitter.target.extern_symbol("__elephc_reference_sources_unbind_v1");
    abi::emit_call_label(emitter, &unbind);
    super::source_lifecycle::require_success(emitter);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_cell_release");
    abi::emit_jump(emitter, done);
    emitter.label(raw);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    emitter.label(done);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Selects a zero layout word without borrowing the integer-result register.
fn branch_zero(emitter: &mut Emitter, value: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbz {value}, {label}")), // raw bindings expose their existing value address
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // inspect the independent promoted-layout word
            emitter.instruction(&format!("jz {label}"));                        // raw bindings expose their existing value address
        }
    }
}

/// Selects an already published managed layout without interpreting the binding as a heap cell.
fn branch_nonzero(emitter: &mut Emitter, value: &str, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cbnz {value}, {label}")), // a promoted binding owns the managed cell in its low word
        Arch::X86_64 => {
            emitter.instruction(&format!("test {value}, {value}"));             // inspect the independent promoted-layout word
            emitter.instruction(&format!("jnz {label}"));                       // a promoted binding owns the managed cell in its low word
        }
    }
}

/// Leaves an uninitialized raw binding unchanged instead of treating its marker as a PHP box.
fn branch_uninitialized(emitter: &mut Emitter, value: &str, marker: &str, label: &str) {
    abi::emit_load_int_immediate(emitter, marker, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {value}, {marker}"));             // uninitialized raw statics have no value owner to transfer
            emitter.instruction(&format!("b.eq {label}"));                      // preserve the caller's PHP initialization guard
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {value}, {marker}"));             // uninitialized raw statics have no value owner to transfer
            emitter.instruction(&format!("je {label}"));                        // preserve the caller's PHP initialization guard
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every supported ABI allocates a real cell and detaches its binding before PHP retirement.
    #[test]
    fn static_mixed_reference_cells_transfer_and_retire_on_every_target() {
        assert_eq!(BINDING_BYTES, 24);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let (promote, release) = assembly.split_once("__rt_reference_static_mixed_release:").unwrap();
            assert!(promote.find("__rt_reference_cell_new").unwrap()
                < promote.find("__rt_reference_cell_promote_mixed").unwrap(), "{name}: allocate before canonical promotion");
            assert!(!promote.contains("__rt_reference_cell_owner"), "{name}: a BSS binding has no heap header");
            let detached = release.find(if name == "linux-x86_64" {
                "mov QWORD PTR [r10 + 16], 0"
            } else { "str xzr, [x10, #16]" }).unwrap();
            let unbind = release.find("__elephc_reference_sources_unbind_v1").unwrap();
            let retire = release.find("__rt_reference_cell_release").unwrap();
            assert!(detached < unbind && unbind < retire, "{name}: detachment and weak metadata precede PHP cleanup");
        }
    }

    /// Native and Reflection reads share a borrowed payload, while main cleanup retires the binding.
    #[test]
    fn static_mixed_reference_readers_and_main_cleanup_cover_every_target() {
        let source = r#"<?php
class StaticCellReadBase { public static mixed $value = 41; }
class StaticCellReadChild extends StaticCellReadBase {}
function staticCellReadViews(): void {
    echo StaticCellReadBase::$value, '|', StaticCellReadChild::$value, '|';
    $class = new ReflectionClass(StaticCellReadChild::class);
    echo $class->getStaticPropertyValue('value'), '|';
    $values = $class->getStaticProperties(); echo $values['value'], '|';
}
staticCellReadViews();
"#;
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
            crate::ir::validate_module(&module).unwrap();
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(assembly.matches("__rt_reference_static_mixed_payload").count() >= 4,
                "{name}: native, inherited and both Reflection views resolve the binding");
            assert!(assembly.contains("__rt_reference_static_mixed_release"),
                "{name}: main cleanup must release the managed cell rather than its child box");
            assert!(assembly.contains("__rt_cleanup_preserve_exception"),
                "{name}: a throwing child cannot skip later static owners");
        }
    }

}
