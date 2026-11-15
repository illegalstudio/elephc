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
/// Managed storage whose immutable declaration has also entered the weak source registry.
const BOUND_LAYOUT: i64 = 2;

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
    emit_bound_promote(emitter);
    emit_prepare_store(emitter);
    emit_release(emitter);
}

/// Prepares an initialized or uninitialized binding without exposing initialization before commit.
fn emit_prepare_store(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let bind = "__rt_reference_static_mixed_prepare_store_bind";
    let done = "__rt_reference_static_mixed_prepare_store_done";
    emitter.label_global("__rt_reference_static_mixed_prepare_store");
    abi::emit_frame_prologue(emitter, 64);
    for (index, offset) in [8, 16].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, result, scratch, 8);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_load_from_address(emitter, result, scratch, LAYOUT_OFFSET);
    abi::emit_branch_if_int_result_nonzero(emitter, bind);
    abi::load_at_offset(emitter, result, 24);
    let seed = "__rt_reference_static_mixed_prepare_store_seed";
    branch_uninitialized(emitter, result, scratch, seed);
    abi::emit_jump(emitter, bind);
    emitter.label(seed);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    emitter.label(bind);
    for (index, offset) in [8, 16].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, "__rt_reference_static_mixed_bind");
    abi::store_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, result, 24);
    let restore = "__rt_reference_static_mixed_prepare_store_restore";
    branch_uninitialized(emitter, result, scratch, restore);
    abi::emit_jump(emitter, done);
    emitter.label(restore);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 8);
    emitter.label(done);
    abi::load_at_offset(emitter, result, 32);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Returns a borrowed canonical cell after registering its static declaration exactly once.
fn emit_bound_promote(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let ready = "__rt_reference_static_mixed_bind_ready";
    let done = "__rt_reference_static_mixed_bind_done";
    emitter.label_global("__rt_reference_static_mixed_bind");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter.target, 0));
    abi::emit_load_from_address(emitter, scratch, result, LAYOUT_OFFSET);
    abi::emit_load_int_immediate(emitter, result, BOUND_LAYOUT);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {scratch}, {result}"));           // repeated selections keep the existing declaration generation
            emitter.instruction(&format!("b.eq {ready}"));                      // skip registry mutation for an already bound cell
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {scratch}, {result}"));           // repeated selections keep the existing declaration generation
            emitter.instruction(&format!("je {ready}"));                        // skip registry mutation for an already bound cell
        }
    }
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_call_label(emitter, "__rt_reference_static_mixed_promote");
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::store_at_offset(emitter, result, 24);
    for (index, offset) in [8, 24, 16].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let bind = emitter.target.extern_symbol("__elephc_reference_sources_bind_v1");
    abi::emit_call_label(emitter, &bind);
    super::source_lifecycle::require_binding_token(emitter);
    abi::load_at_offset(emitter, result, 24);
    super::source_lifecycle::associate_result(emitter);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_int_immediate(emitter, result, BOUND_LAYOUT);
    abi::emit_store_to_address(emitter, result, scratch, LAYOUT_OFFSET);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_jump(emitter, done);
    emitter.label(ready);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
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
    use crate::codegen_support::platform::Target;

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

    /// Static declaration registration precedes layout publication and repeated selections keep its generation.
    #[test]
    fn static_mixed_reference_binding_registers_once_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            let binding = assembly.split_once("__rt_reference_static_mixed_bind:").unwrap().1
                .split_once("__rt_reference_static_mixed_release:").unwrap().0;
            let fast = binding.find("__rt_reference_static_mixed_bind_ready").unwrap();
            let promote = binding.find("__rt_reference_static_mixed_promote").unwrap();
            let register = binding.find("__elephc_reference_sources_bind_v1").unwrap();
            let associate = binding.find("__rt_reference_source_associate_native").unwrap();
            let publish = if target.arch == Arch::AArch64 { "str x0, [x10, #16]" }
                else { "mov QWORD PTR [r10 + 16], rax" };
            assert!(fast < promote && promote < register && register < associate, "{name}");
            assert!(associate < binding.find(publish).unwrap(), "{name}: publish a registered layout last");
        }
    }

    /// Real assemblers accept the static binding protocol and its shared registry boundary on every target.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn static_mixed_reference_bindings_assemble_on_all_supported_targets() {
        use std::{io::Write, process::{Command, Stdio}};
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            super::super::source_lifecycle::emit(&mut emitter);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                .spawn().expect("clang is required for explicit cross-target assembly validation");
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
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
